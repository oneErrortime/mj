//! Storage adapters for the database module.
//!
//! Implemented:
//! - [`SqliteAdapter`] — embedded SQLite using `rusqlite` with `bundled` feature.
//!   Stores every entity as a JSON blob in a single `entities` table so the
//!   schema is 100% schemaless from the caller's perspective (exactly like NeDB).

use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ─── Adapter trait ────────────────────────────────────────────────────────────

/// Common interface all adapters must implement.
#[async_trait]
pub trait DbAdapter: Send + Sync {
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;

    /// Find entities matching `query`. Returns JSON array.
    async fn find(
        &self,
        query:    Option<&Value>,
        sort:     Option<&[String]>,
        limit:    Option<i64>,
        offset:   Option<i64>,
        fields:   Option<&[String]>,
        search:   Option<&str>,
        search_fields: Option<&[String]>,
    ) -> Result<Vec<Value>>;

    async fn count(
        &self,
        query:  Option<&Value>,
        search: Option<&str>,
        search_fields: Option<&[String]>,
    ) -> Result<i64>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Value>>;
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>>;

    async fn insert(&self, entity: Value) -> Result<Value>;
    async fn insert_many(&self, entities: Vec<Value>) -> Result<Vec<Value>>;

    async fn update_by_id(&self, id: &str, changes: Value) -> Result<Option<Value>>;
    async fn replace_by_id(&self, id: &str, entity: Value) -> Result<Option<Value>>;

    async fn remove_by_id(&self, id: &str) -> Result<Option<Value>>;
    async fn remove_many(&self, query: Option<&Value>) -> Result<i64>;
    async fn clear(&self) -> Result<i64>;
}

// ─── SqliteAdapter ────────────────────────────────────────────────────────────

/// Embedded SQLite adapter (schemaless JSON storage, mirrors NeDB / in-memory).
///
/// All entities are stored in a single table:
/// ```sql
/// CREATE TABLE entities (
///     id   TEXT PRIMARY KEY,
///     doc  TEXT NOT NULL   -- JSON blob
/// );
/// ```
pub struct SqliteAdapter {
    conn: Arc<Mutex<Connection>>,
    table: String,
}

impl SqliteAdapter {
    /// Create an in-memory SQLite database (for testing / development).
    pub fn memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory SQLite failed");
        let adapter = Self {
            conn:  Arc::new(Mutex::new(conn)),
            table: "entities".into(),
        };
        adapter.init_schema().expect("schema init failed");
        adapter
    }

    /// Create (or open) a file-backed SQLite database.
    pub fn file(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| MoleculerError::Internal(format!("SQLite open {path}: {e}")))?;
        let adapter = Self {
            conn:  Arc::new(Mutex::new(conn)),
            table: "entities".into(),
        };
        adapter.init_schema()?;
        Ok(adapter)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id   TEXT PRIMARY KEY,
                doc  TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS {}_id ON {}(id);",
            self.table, self.table, self.table
        ))
        .map_err(|e| MoleculerError::Internal(format!("SQLite schema: {e}")))
    }

    fn rows_to_docs(&self, conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<Value>> {
        let mut stmt = conn.prepare(sql)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p.iter()), |row| {
            let blob: String = row.get(0)?;
            Ok(blob)
        }).map_err(|e| MoleculerError::Internal(e.to_string()))?;

        let mut out = Vec::new();
        for r in rows {
            let blob = r.map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let v: Value = serde_json::from_str(&blob)
                .map_err(|e| MoleculerError::Internal(format!("JSON parse: {e}")))?;
            out.push(v);
        }
        Ok(out)
    }

    fn matches_query(doc: &Value, query: &Value) -> bool {
        if let Some(obj) = query.as_object() {
            for (key, expected) in obj {
                match key.as_str() {
                    "$and" => {
                        if let Some(arr) = expected.as_array() {
                            if !arr.iter().all(|q| Self::matches_query(doc, q)) { return false; }
                        }
                    }
                    "$or" => {
                        if let Some(arr) = expected.as_array() {
                            if !arr.iter().any(|q| Self::matches_query(doc, q)) { return false; }
                        }
                    }
                    "$nor" => {
                        if let Some(arr) = expected.as_array() {
                            if arr.iter().any(|q| Self::matches_query(doc, q)) { return false; }
                        }
                    }
                    field => {
                        let doc_val = Self::get_nested(doc, field);
                        if !Self::matches_condition(&doc_val, expected) { return false; }
                    }
                }
            }
        }
        true
    }

    fn get_nested<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
        let mut cur = doc;
        for part in path.split('.') {
            cur = cur.get(part)?;
        }
        Some(cur)
    }

    fn matches_condition(val: &Option<&Value>, cond: &Value) -> bool {
        if let Some(obj) = cond.as_object() {
            // MongoDB-style operators
            for (op, operand) in obj {
                let matched = match op.as_str() {
                    "$eq"  => val.map_or(false, |v| v == operand),
                    "$ne"  => val.map_or(true,  |v| v != operand),
                    "$gt"  => val.and_then(|v| v.as_f64()).zip(operand.as_f64()).map_or(false, |(a, b)| a > b),
                    "$gte" => val.and_then(|v| v.as_f64()).zip(operand.as_f64()).map_or(false, |(a, b)| a >= b),
                    "$lt"  => val.and_then(|v| v.as_f64()).zip(operand.as_f64()).map_or(false, |(a, b)| a < b),
                    "$lte" => val.and_then(|v| v.as_f64()).zip(operand.as_f64()).map_or(false, |(a, b)| a <= b),
                    "$in"  => {
                        if let (Some(v), Some(arr)) = (val, operand.as_array()) {
                            arr.contains(v)
                        } else { false }
                    }
                    "$nin" => {
                        if let (Some(v), Some(arr)) = (val, operand.as_array()) {
                            !arr.contains(v)
                        } else { true }
                    }
                    "$exists" => {
                        let exists = val.is_some();
                        operand.as_bool().map_or(exists, |b| b == exists)
                    }
                    "$regex" => {
                        // Basic prefix/contains check (no full regex dep)
                        if let (Some(Value::String(s)), Some(Value::String(pat))) = (val, Some(operand)) {
                            s.contains(pat.trim_matches('/'))
                        } else { false }
                    }
                    _ => false,
                };
                if !matched { return false; }
            }
            true
        } else {
            // Direct equality
            val.map_or(false, |v| v == cond)
        }
    }

    fn search_matches(doc: &Value, search: &str, fields: &Option<&[String]>) -> bool {
        let lower = search.to_lowercase();
        if let Some(fields) = fields {
            for f in *fields {
                if let Some(Value::String(s)) = Self::get_nested(doc, f) {
                    if s.to_lowercase().contains(&lower) { return true; }
                }
            }
        } else {
            // Search all string fields
            if let Some(obj) = doc.as_object() {
                for v in obj.values() {
                    if let Value::String(s) = v {
                        if s.to_lowercase().contains(&lower) { return true; }
                    }
                }
            }
        }
        false
    }

    fn project(doc: Value, fields: &[String]) -> Value {
        if fields.is_empty() { return doc; }
        if let Value::Object(obj) = doc {
            let mut out = Map::new();
            for f in fields {
                if let Some(v) = obj.get(f) {
                    out.insert(f.clone(), v.clone());
                }
            }
            Value::Object(out)
        } else {
            doc
        }
    }

    fn apply_sort(docs: &mut Vec<Value>, sort: &[String]) {
        docs.sort_by(|a, b| {
            for field_spec in sort {
                let (field, asc) = if let Some(f) = field_spec.strip_prefix('-') {
                    (f, false)
                } else {
                    (field_spec.as_str(), true)
                };

                let va = Self::get_nested(a, field).cloned().unwrap_or(Value::Null);
                let vb = Self::get_nested(b, field).cloned().unwrap_or(Value::Null);

                let cmp = Self::cmp_values(&va, &vb);
                if cmp != std::cmp::Ordering::Equal {
                    return if asc { cmp } else { cmp.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    fn cmp_values(a: &Value, b: &Value) -> std::cmp::Ordering {
        match (a, b) {
            (Value::Number(na), Value::Number(nb)) => {
                na.as_f64().unwrap_or(0.0).partial_cmp(&nb.as_f64().unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::String(sa), Value::String(sb)) => sa.cmp(sb),
            (Value::Bool(ba), Value::Bool(bb)) => ba.cmp(bb),
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    }
}

#[async_trait]
impl DbAdapter for SqliteAdapter {
    async fn connect(&self) -> Result<()> { Ok(()) }
    async fn disconnect(&self) -> Result<()> { Ok(()) }

    async fn find(
        &self,
        query:    Option<&Value>,
        sort:     Option<&[String]>,
        limit:    Option<i64>,
        offset:   Option<i64>,
        fields:   Option<&[String]>,
        search:   Option<&str>,
        search_fields: Option<&[String]>,
    ) -> Result<Vec<Value>> {
        let conn = self.conn.lock().unwrap();
        let sql  = format!("SELECT doc FROM {} ", self.table);
        let mut docs = self.rows_to_docs(&conn, &sql, &[])?;

        // Filter
        if let Some(q) = query {
            docs.retain(|d| Self::matches_query(d, q));
        }
        if let Some(s) = search {
            docs.retain(|d| Self::search_matches(d, s, &search_fields));
        }

        // Sort
        if let Some(s) = sort {
            if !s.is_empty() { Self::apply_sort(&mut docs, s); }
        }

        // Offset + limit
        let start = offset.unwrap_or(0).max(0) as usize;
        let docs: Vec<Value> = docs.into_iter().skip(start).collect();
        let docs: Vec<Value> = if let Some(lim) = limit {
            docs.into_iter().take(lim.max(0) as usize).collect()
        } else {
            docs
        };

        // Project fields
        let docs = if let Some(fs) = fields {
            docs.into_iter().map(|d| Self::project(d, fs)).collect()
        } else {
            docs
        };

        Ok(docs)
    }

    async fn count(
        &self,
        query:  Option<&Value>,
        search: Option<&str>,
        search_fields: Option<&[String]>,
    ) -> Result<i64> {
        let all = self.find(query, None, None, None, None, search, search_fields).await?;
        Ok(all.len() as i64)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Value>> {
        let conn = self.conn.lock().unwrap();
        let sql  = format!("SELECT doc FROM {} WHERE id = ?1", self.table);
        let mut stmt = conn.prepare(&sql)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let mut rows = stmt.query(params![id])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| MoleculerError::Internal(e.to_string()))? {
            let blob: String = row.get(0).map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let v: Value = serde_json::from_str(&blob)
                .map_err(|e| MoleculerError::Internal(e.to_string()))?;
            return Ok(Some(v));
        }
        Ok(None)
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for id in ids {
            if let Some(v) = self.find_by_id(id).await? {
                out.push(v);
            }
        }
        Ok(out)
    }

    async fn insert(&self, mut entity: Value) -> Result<Value> {
        // Auto-generate id if not present
        if entity.get("id").is_none() || entity["id"].is_null() {
            let id = Uuid::new_v4().to_string();
            entity["id"] = Value::String(id);
        }
        let id = entity["id"].as_str().unwrap().to_string();
        let blob = serde_json::to_string(&entity)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            &format!("INSERT INTO {} (id, doc) VALUES (?1, ?2)", self.table),
            params![id, blob],
        ).map_err(|e| MoleculerError::Internal(format!("INSERT: {e}")))?;
        Ok(entity)
    }

    async fn insert_many(&self, entities: Vec<Value>) -> Result<Vec<Value>> {
        let mut out = Vec::with_capacity(entities.len());
        for e in entities {
            out.push(self.insert(e).await?);
        }
        Ok(out)
    }

    async fn update_by_id(&self, id: &str, changes: Value) -> Result<Option<Value>> {
        let existing = self.find_by_id(id).await?;
        if let Some(mut doc) = existing {
            if let (Some(doc_obj), Some(changes_obj)) = (doc.as_object_mut(), changes.as_object()) {
                for (k, v) in changes_obj {
                    if k == "id" { continue; }
                    if v.is_null() {
                        doc_obj.remove(k);
                    } else {
                        doc_obj.insert(k.clone(), v.clone());
                    }
                }
            }
            let blob = serde_json::to_string(&doc)
                .map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let conn = self.conn.lock().unwrap();
            conn.execute(
                &format!("UPDATE {} SET doc = ?1 WHERE id = ?2", self.table),
                params![blob, id],
            ).map_err(|e| MoleculerError::Internal(format!("UPDATE: {e}")))?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    async fn replace_by_id(&self, id: &str, mut entity: Value) -> Result<Option<Value>> {
        if self.find_by_id(id).await?.is_none() { return Ok(None); }
        entity["id"] = json!(id);
        let blob = serde_json::to_string(&entity)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            &format!("UPDATE {} SET doc = ?1 WHERE id = ?2", self.table),
            params![blob, id],
        ).map_err(|e| MoleculerError::Internal(format!("REPLACE: {e}")))?;
        Ok(Some(entity))
    }

    async fn remove_by_id(&self, id: &str) -> Result<Option<Value>> {
        let existing = self.find_by_id(id).await?;
        if existing.is_some() {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                &format!("DELETE FROM {} WHERE id = ?1", self.table),
                params![id],
            ).map_err(|e| MoleculerError::Internal(format!("DELETE: {e}")))?;
        }
        Ok(existing)
    }

    async fn remove_many(&self, query: Option<&Value>) -> Result<i64> {
        let docs = self.find(query, None, None, None, Some(&["id".into()]), None, None).await?;
        let count = docs.len() as i64;
        let conn  = self.conn.lock().unwrap();
        for doc in docs {
            if let Some(id) = doc["id"].as_str() {
                conn.execute(
                    &format!("DELETE FROM {} WHERE id = ?1", self.table),
                    params![id],
                ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
            }
        }
        Ok(count)
    }

    async fn clear(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", self.table), [], |r| r.get(0)
        ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
        conn.execute(&format!("DELETE FROM {}", self.table), [])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        Ok(n)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_find() {
        let a = SqliteAdapter::memory();
        let doc = a.insert(serde_json::json!({ "name": "Alice", "age": 30 })).await.unwrap();
        assert!(doc["id"].is_string());
        let id = doc["id"].as_str().unwrap().to_string();

        let found = a.find_by_id(&id).await.unwrap().unwrap();
        assert_eq!(found["name"], "Alice");
    }

    #[tokio::test]
    async fn test_find_with_query() {
        let a = SqliteAdapter::memory();
        a.insert(serde_json::json!({ "city": "Rotterdam", "pop": 650000 })).await.unwrap();
        a.insert(serde_json::json!({ "city": "Amsterdam", "pop": 850000 })).await.unwrap();

        let results = a.find(
            Some(&serde_json::json!({ "city": "Rotterdam" })),
            None, None, None, None, None, None,
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["city"], "Rotterdam");
    }

    #[tokio::test]
    async fn test_update() {
        let a = SqliteAdapter::memory();
        let doc = a.insert(serde_json::json!({ "x": 1 })).await.unwrap();
        let id  = doc["id"].as_str().unwrap().to_string();
        let updated = a.update_by_id(&id, serde_json::json!({ "x": 99 })).await.unwrap().unwrap();
        assert_eq!(updated["x"], 99);
    }

    #[tokio::test]
    async fn test_remove() {
        let a = SqliteAdapter::memory();
        let doc = a.insert(serde_json::json!({ "y": 42 })).await.unwrap();
        let id  = doc["id"].as_str().unwrap().to_string();
        let removed = a.remove_by_id(&id).await.unwrap();
        assert!(removed.is_some());
        assert!(a.find_by_id(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sort_and_limit() {
        let a = SqliteAdapter::memory();
        for i in 0..5u64 {
            a.insert(serde_json::json!({ "n": i })).await.unwrap();
        }
        let results = a.find(
            None,
            Some(&["-n".to_string()]),
            Some(3), Some(0), None, None, None,
        ).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["n"], 4);
    }

    #[tokio::test]
    async fn test_count() {
        let a = SqliteAdapter::memory();
        a.insert(serde_json::json!({ "tag": "a" })).await.unwrap();
        a.insert(serde_json::json!({ "tag": "b" })).await.unwrap();
        assert_eq!(a.count(None, None, None).await.unwrap(), 2);
        assert_eq!(
            a.count(Some(&serde_json::json!({ "tag": "a" })), None, None).await.unwrap(), 1
        );
    }

    #[tokio::test]
    async fn test_operators() {
        let a = SqliteAdapter::memory();
        a.insert(serde_json::json!({ "score": 10 })).await.unwrap();
        a.insert(serde_json::json!({ "score": 20 })).await.unwrap();
        a.insert(serde_json::json!({ "score": 30 })).await.unwrap();

        let r = a.find(
            Some(&serde_json::json!({ "score": { "$gte": 20 } })),
            None, None, None, None, None, None,
        ).await.unwrap();
        assert_eq!(r.len(), 2);

        let r2 = a.find(
            Some(&serde_json::json!({ "score": { "$in": [10, 30] } })),
            None, None, None, None, None, None,
        ).await.unwrap();
        assert_eq!(r2.len(), 2);
    }

    #[tokio::test]
    async fn test_search() {
        let a = SqliteAdapter::memory();
        a.insert(serde_json::json!({ "name": "Alice Smith" })).await.unwrap();
        a.insert(serde_json::json!({ "name": "Bob Jones" })).await.unwrap();

        let r = a.find(None, None, None, None, None, Some("alice"), None).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["name"], "Alice Smith");
    }

    #[tokio::test]
    async fn test_clear() {
        let a = SqliteAdapter::memory();
        a.insert(serde_json::json!({ "x": 1 })).await.unwrap();
        a.insert(serde_json::json!({ "x": 2 })).await.unwrap();
        let n = a.clear().await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(a.count(None, None, None).await.unwrap(), 0);
    }
}
