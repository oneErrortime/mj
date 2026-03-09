//! Database adapters — NeDB-like (in-memory with persistence) and stubs
//! for MongoDB and SQL (via feature flags).
//!
//! Mirrors `@moleculer/database/src/adapters/`:
//! - nedb.js   → NeDbAdapter
//! - mongodb.js → MongoAdapter (stub, feature `mongodb`)
//! - knex.js   → KnexAdapter  (stub, feature `sqlx`)

use super::{DbAdapter, FindParams};
use crate::error::{MoleculerError, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── NeDB-style adapter (JSON file persistence) ───────────────────────────────
//
// Stores all records in memory (HashMap) and optionally flushes to a JSON file.
// This mirrors NeDB's "datafile" mode used in @moleculer/database.
//
// Data file format: one JSON object per line (JSON Lines / ndjson).

pub struct NeDbAdapter {
    /// In-memory store: id → document
    store: Arc<RwLock<HashMap<String, Value>>>,
    /// Optional path to persist data (JSON Lines format)
    datafile: Option<PathBuf>,
    auto_load: bool,
    auto_compact_interval: Option<u64>,
}

impl NeDbAdapter {
    /// In-memory only.
    pub fn new() -> Self {
        Self { store: Arc::new(RwLock::new(HashMap::new())), datafile: None,
               auto_load: false, auto_compact_interval: None }
    }

    /// With file persistence.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        Self { store: Arc::new(RwLock::new(HashMap::new())), datafile: Some(path.into()),
               auto_load: true, auto_compact_interval: Some(60_000) }
    }

    async fn load_from_file(&self) -> Result<()> {
        let path = match &self.datafile { Some(p) => p, None => return Ok(()) };
        if !path.exists() { return Ok(()); }
        let content = tokio::fs::read_to_string(path).await
            .map_err(MoleculerError::Io)?;
        let mut store = self.store.write().await;
        for line in content.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(doc) = serde_json::from_str::<Value>(line) {
                if let Some(id) = doc.get("id").and_then(|v| v.as_str()) {
                    store.insert(id.to_string(), doc);
                }
            }
        }
        log::info!("[NeDbAdapter] loaded {} records from {:?}", store.len(), path);
        Ok(())
    }

    async fn persist(&self) -> Result<()> {
        let path = match &self.datafile { Some(p) => p, None => return Ok(()) };
        let store = self.store.read().await;
        let mut lines = String::new();
        for doc in store.values() {
            lines.push_str(&serde_json::to_string(doc)?);
            lines.push('\n');
        }
        tokio::fs::write(path, lines).await.map_err(MoleculerError::Io)?;
        Ok(())
    }

    fn matches_query(entity: &Value, query: &Value) -> bool {
        if let Some(obj) = query.as_object() {
            for (k, v) in obj {
                // Support MongoDB-style operators
                if let Some(doc_val) = entity.get(k) {
                    if let Some(ops) = v.as_object() {
                        for (op, op_val) in ops {
                            let ok = match op.as_str() {
                                "$eq"  => doc_val == op_val,
                                "$ne"  => doc_val != op_val,
                                "$gt"  => doc_val.as_f64().zip(op_val.as_f64()).map(|(a,b)| a > b).unwrap_or(false),
                                "$gte" => doc_val.as_f64().zip(op_val.as_f64()).map(|(a,b)| a >= b).unwrap_or(false),
                                "$lt"  => doc_val.as_f64().zip(op_val.as_f64()).map(|(a,b)| a < b).unwrap_or(false),
                                "$lte" => doc_val.as_f64().zip(op_val.as_f64()).map(|(a,b)| a <= b).unwrap_or(false),
                                "$in"  => op_val.as_array().map(|arr| arr.contains(doc_val)).unwrap_or(false),
                                "$nin" => op_val.as_array().map(|arr| !arr.contains(doc_val)).unwrap_or(false),
                                "$exists" => op_val.as_bool().map(|b| b).unwrap_or(true),
                                "$regex" => {
                                    let pat = op_val.as_str().unwrap_or("");
                                    let s = doc_val.as_str().unwrap_or("");
                                    // Simple contains-based regex stub
                                    s.contains(pat)
                                }
                                _ => true,
                            };
                            if !ok { return false; }
                        }
                    } else if doc_val != v {
                        return false;
                    }
                } else if v != &Value::Null {
                    return false;
                }
            }
        }
        true
    }

    fn new_id() -> String { uuid::Uuid::new_v4().to_string() }
}

#[async_trait::async_trait]
impl DbAdapter for NeDbAdapter {
    fn name(&self) -> &str { "NeDbAdapter" }

    async fn connect(&self) -> Result<()> {
        if self.auto_load { self.load_from_file().await?; }
        log::debug!("[NeDbAdapter] ready (datafile={:?})", self.datafile);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        if self.datafile.is_some() { self.persist().await?; }
        Ok(())
    }

    async fn find(&self, params: &FindParams) -> Result<Vec<Value>> {
        let store = self.store.read().await;
        let mut results: Vec<&Value> = store.values().collect();

        if let Some(q) = &params.query {
            results.retain(|e| Self::matches_query(e, q));
        }
        if let (Some(search), Some(fields)) = (&params.search, &params.search_fields) {
            let lower = search.to_lowercase();
            results.retain(|e| {
                fields.iter().any(|f| {
                    e.get(f).map(|v| v.to_string().to_lowercase().contains(&lower)).unwrap_or(false)
                })
            });
        }

        // Sort
        if let Some(sorts) = &params.sort {
            let mut owned: Vec<Value> = results.into_iter().cloned().collect();
            for sort_key in sorts.iter().rev() {
                let (key, asc) = if sort_key.starts_with('-') { (&sort_key[1..], false) }
                                 else { (sort_key.as_str(), true) };
                owned.sort_by(|a, b| {
                    let av = a.get(key).map(|v| v.to_string()).unwrap_or_default();
                    let bv = b.get(key).map(|v| v.to_string()).unwrap_or_default();
                    if asc { av.cmp(&bv) } else { bv.cmp(&av) }
                });
            }
            let offset = params.offset.unwrap_or(0) as usize;
            let limit = params.limit.map(|l| l as usize).unwrap_or(usize::MAX);
            return Ok(owned.into_iter().skip(offset).take(limit).collect());
        }

        let offset = params.offset.unwrap_or(0) as usize;
        let limit = params.limit.map(|l| l as usize).unwrap_or(usize::MAX);
        Ok(results.into_iter().skip(offset).take(limit).cloned().collect())
    }

    async fn find_one(&self, query: &Value) -> Result<Option<Value>> {
        let store = self.store.read().await;
        Ok(store.values().find(|e| Self::matches_query(e, query)).cloned())
    }

    async fn count(&self, query: Option<&Value>) -> Result<u64> {
        let store = self.store.read().await;
        let n = match query {
            Some(q) => store.values().filter(|e| Self::matches_query(e, q)).count(),
            None    => store.len(),
        };
        Ok(n as u64)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Value>> {
        Ok(self.store.read().await.get(id).cloned())
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>> {
        let store = self.store.read().await;
        Ok(ids.iter().filter_map(|id| store.get(id.as_str())).cloned().collect())
    }

    async fn insert(&self, mut entity: Value) -> Result<Value> {
        let id = entity.get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(Self::new_id);
        if let Some(obj) = entity.as_object_mut() {
            obj.insert("id".into(), json!(id));
            let now = chrono::Utc::now().to_rfc3339();
            obj.entry("createdAt").or_insert_with(|| json!(now));
            obj.entry("updatedAt").or_insert_with(|| json!(now));
        }
        self.store.write().await.insert(id, entity.clone());
        if self.datafile.is_some() { let _ = self.persist().await; }
        Ok(entity)
    }

    async fn insert_many(&self, entities: Vec<Value>) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        for e in entities { results.push(self.insert(e).await?); }
        Ok(results)
    }

    async fn update_by_id(&self, id: &str, update: Value) -> Result<Option<Value>> {
        let mut store = self.store.write().await;
        if let Some(existing) = store.get_mut(id) {
            if let (Some(existing_obj), Some(update_obj)) = (existing.as_object_mut(), update.as_object()) {
                for (k, v) in update_obj {
                    if k == "id" { continue; }
                    // Support $set, $unset, $inc operators
                    if k == "$set" {
                        if let Some(set_obj) = v.as_object() {
                            for (sk, sv) in set_obj { existing_obj.insert(sk.clone(), sv.clone()); }
                        }
                    } else if k == "$unset" {
                        if let Some(unset_obj) = v.as_object() {
                            for sk in unset_obj.keys() { existing_obj.remove(sk); }
                        }
                    } else if k == "$inc" {
                        if let Some(inc_obj) = v.as_object() {
                            for (ik, iv) in inc_obj {
                                if let Some(delta) = iv.as_f64() {
                                    let current = existing_obj.get(ik).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    existing_obj.insert(ik.clone(), json!(current + delta));
                                }
                            }
                        }
                    } else {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
                existing_obj.insert("updatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
            }
            let result = existing.clone();
            drop(store);
            if self.datafile.is_some() { let _ = self.persist().await; }
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn replace_by_id(&self, id: &str, mut entity: Value) -> Result<Option<Value>> {
        let mut store = self.store.write().await;
        if !store.contains_key(id) { return Ok(None); }
        if let Some(obj) = entity.as_object_mut() {
            obj.insert("id".into(), json!(id));
            obj.insert("updatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
        }
        store.insert(id.to_string(), entity.clone());
        drop(store);
        if self.datafile.is_some() { let _ = self.persist().await; }
        Ok(Some(entity))
    }

    async fn remove_by_id(&self, id: &str) -> Result<Option<Value>> {
        let result = self.store.write().await.remove(id);
        if result.is_some() && self.datafile.is_some() { let _ = self.persist().await; }
        Ok(result)
    }

    async fn clear(&self) -> Result<u64> {
        let mut store = self.store.write().await;
        let count = store.len() as u64;
        store.clear();
        drop(store);
        if self.datafile.is_some() { let _ = self.persist().await; }
        Ok(count)
    }
}

// ─── MongoDB adapter stub ─────────────────────────────────────────────────────

/// MongoDB adapter. Full implementation requires `mongodb` crate.
///
/// Mirrors `@moleculer/database/src/adapters/mongodb.js`.
///
/// To enable: add `mongodb = "2"` to Cargo.toml and implement the trait methods
/// using `mongodb::Collection<Document>`.
pub struct MongoAdapter {
    pub uri: String,
    pub db_name: String,
    pub collection_name: String,
}

impl MongoAdapter {
    pub fn new(uri: impl Into<String>, db: impl Into<String>, collection: impl Into<String>) -> Self {
        Self { uri: uri.into(), db_name: db.into(), collection_name: collection.into() }
    }
}

#[async_trait::async_trait]
impl DbAdapter for MongoAdapter {
    fn name(&self) -> &str { "MongoAdapter" }

    async fn connect(&self) -> Result<()> {
        log::info!("[MongoAdapter] connecting to {} / {} / {}",
            self.uri, self.db_name, self.collection_name);
        // Real impl: let client = mongodb::Client::with_uri_str(&self.uri).await?;
        log::warn!("[MongoAdapter] stub — add `mongodb` crate for full implementation");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> { Ok(()) }

    async fn find(&self, _params: &FindParams) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub — not yet implemented".into()))
    }
    async fn find_one(&self, _query: &Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn count(&self, _query: Option<&Value>) -> Result<u64> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn find_by_id(&self, _id: &str) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn find_by_ids(&self, _ids: &[String]) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn insert(&self, _entity: Value) -> Result<Value> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn insert_many(&self, _entities: Vec<Value>) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn update_by_id(&self, _id: &str, _update: Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn replace_by_id(&self, _id: &str, _entity: Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn remove_by_id(&self, _id: &str) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
    async fn clear(&self) -> Result<u64> {
        Err(MoleculerError::Internal("MongoAdapter stub".into()))
    }
}

// ─── SQL adapter stub (sqlx-backed) ──────────────────────────────────────────

/// SQL adapter using `sqlx`. Supports PostgreSQL, MySQL, SQLite.
///
/// Mirrors `@moleculer/database/src/adapters/knex.js`.
///
/// To enable: add `sqlx = { version = "0.7", features = ["runtime-tokio", "postgres"] }`
/// to Cargo.toml.
pub struct SqlAdapter {
    pub database_url: String,
    pub table_name: String,
    pub id_column: String,
}

impl SqlAdapter {
    pub fn new(database_url: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            table_name: table.into(),
            id_column: "id".into(),
        }
    }

    pub fn sqlite(path: impl Into<String>, table: impl Into<String>) -> Self {
        Self::new(format!("sqlite:{}", path.into()), table)
    }

    pub fn postgres(url: impl Into<String>, table: impl Into<String>) -> Self {
        Self::new(url.into(), table)
    }
}

#[async_trait::async_trait]
impl DbAdapter for SqlAdapter {
    fn name(&self) -> &str { "SqlAdapter" }

    async fn connect(&self) -> Result<()> {
        log::info!("[SqlAdapter] connecting to {} / {}", self.database_url, self.table_name);
        log::warn!("[SqlAdapter] stub — add `sqlx` crate for full implementation");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> { Ok(()) }

    // Stubs — real implementation would use sqlx::query_as!() macros
    async fn find(&self, _params: &FindParams) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub — add sqlx crate".into()))
    }
    async fn find_one(&self, _query: &Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn count(&self, _query: Option<&Value>) -> Result<u64> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn find_by_id(&self, _id: &str) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn find_by_ids(&self, _ids: &[String]) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn insert(&self, _entity: Value) -> Result<Value> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn insert_many(&self, _entities: Vec<Value>) -> Result<Vec<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn update_by_id(&self, _id: &str, _update: Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn replace_by_id(&self, _id: &str, _entity: Value) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn remove_by_id(&self, _id: &str) -> Result<Option<Value>> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
    async fn clear(&self) -> Result<u64> {
        Err(MoleculerError::Internal("SqlAdapter stub".into()))
    }
}
