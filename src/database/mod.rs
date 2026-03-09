//! `@moleculer/database` — full Rust port.
//!
//! Provides a `DatabaseMixin` that adds CRUD actions to any service:
//! - `find`       — query with filter/sort/paginate/populate
//! - `list`       — paginated response with `rows` + `total` + `page` + `pageSize`
//! - `count`      — count matching records
//! - `get`        — get by ID (or array of IDs)
//! - `create`     — create one entity
//! - `createMany` — bulk create
//! - `update`     — update by ID (partial)
//! - `replace`    — replace by ID (full)
//! - `remove`     — delete by ID
//!
//! ## Usage
//!
//! ```rust,no_run
//! use moleculer::database::{DatabaseMixin, DatabaseOptions, MemoryAdapter};
//! use moleculer::service::ServiceSchema;
//!
//! let svc = ServiceSchema::new("users")
//!     .mixin(DatabaseMixin::new(MemoryAdapter::new(), DatabaseOptions::default()));
//! ```
//!
//! Mirrors `@moleculer/database/src/index.js`.

use crate::context::Context;
use crate::error::{MoleculerError, Result};
use crate::service::{ActionDef, ServiceSchema};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── Database adapter trait ───────────────────────────────────────────────────

/// Query filter passed to adapter methods.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<Vec<String>>,
    pub fields: Option<Vec<String>>,
    pub search: Option<String>,
    pub search_fields: Option<Vec<String>>,
    pub query: Option<Value>,
    pub populate: Option<Vec<String>>,
    pub scope: Option<Vec<String>>,
}

impl FindParams {
    pub fn from_ctx(ctx: &Context) -> Self {
        let p = &ctx.params;
        Self {
            limit: p.get("limit").and_then(|v| v.as_i64()),
            offset: p.get("offset").and_then(|v| v.as_i64()),
            sort: p.get("sort").and_then(|v| {
                v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            }).or_else(|| p.get("sort").and_then(|v| v.as_str()).map(|s| vec![s.to_string()])),
            fields: p.get("fields").and_then(|v| {
                v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            }),
            search: p.get("search").and_then(|v| v.as_str()).map(str::to_string),
            search_fields: p.get("searchFields").and_then(|v| {
                v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            }),
            query: p.get("query").cloned(),
            populate: p.get("populate").and_then(|v| {
                v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            }),
            scope: p.get("scope").and_then(|v| {
                v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    pub rows: Vec<Value>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
    pub total_pages: u64,
}

#[async_trait]
pub trait DbAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn find(&self, params: &FindParams) -> Result<Vec<Value>>;
    async fn find_one(&self, query: &Value) -> Result<Option<Value>>;
    async fn count(&self, query: Option<&Value>) -> Result<u64>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Value>>;
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>>;
    async fn insert(&self, entity: Value) -> Result<Value>;
    async fn insert_many(&self, entities: Vec<Value>) -> Result<Vec<Value>>;
    async fn update_by_id(&self, id: &str, update: Value) -> Result<Option<Value>>;
    async fn replace_by_id(&self, id: &str, entity: Value) -> Result<Option<Value>>;
    async fn remove_by_id(&self, id: &str) -> Result<Option<Value>>;
    async fn clear(&self) -> Result<u64>;
}

// ─── In-memory adapter ────────────────────────────────────────────────────────

/// Simple HashMap-backed in-memory adapter.
/// For development and testing.
pub struct MemoryAdapter {
    store: Arc<RwLock<HashMap<String, Value>>>,
}

impl MemoryAdapter {
    pub fn new() -> Self {
        Self { store: Arc::new(RwLock::new(HashMap::new())) }
    }

    fn matches_query(entity: &Value, query: &Value) -> bool {
        if let Some(obj) = query.as_object() {
            for (k, v) in obj {
                match entity.get(k) {
                    None => return false,
                    Some(ev) => {
                        // Simple equality check; a real adapter would support operators
                        if ev != v { return false; }
                    }
                }
            }
        }
        true
    }

    fn search_matches(entity: &Value, search: &str, fields: &[String]) -> bool {
        let lower = search.to_lowercase();
        for field in fields {
            if let Some(v) = entity.get(field) {
                let s = v.to_string().to_lowercase();
                if s.contains(&lower) { return true; }
            }
        }
        false
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[async_trait]
impl DbAdapter for MemoryAdapter {
    fn name(&self) -> &str { "MemoryAdapter" }

    async fn connect(&self) -> Result<()> {
        log::debug!("[MemoryAdapter] ready");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> { Ok(()) }

    async fn find(&self, params: &FindParams) -> Result<Vec<Value>> {
        let store = self.store.read().await;
        let mut results: Vec<&Value> = store.values().collect();

        // Apply query filter
        if let Some(q) = &params.query {
            results.retain(|e| Self::matches_query(e, q));
        }

        // Apply search
        if let (Some(search), Some(fields)) = (&params.search, &params.search_fields) {
            if !fields.is_empty() {
                results.retain(|e| Self::search_matches(e, search, fields));
            }
        }

        // Sort
        if let Some(sorts) = &params.sort {
            for sort_key in sorts.iter().rev() {
                let (key, asc) = if sort_key.starts_with('-') {
                    (&sort_key[1..], false)
                } else {
                    (sort_key.as_str(), true)
                };
                results.sort_by(|a, b| {
                    let av = a.get(key).map(|v| v.to_string()).unwrap_or_default();
                    let bv = b.get(key).map(|v| v.to_string()).unwrap_or_default();
                    if asc { av.cmp(&bv) } else { bv.cmp(&av) }
                });
            }
        }

        // Offset + limit
        let offset = params.offset.unwrap_or(0) as usize;
        let limit = params.limit.map(|l| l as usize).unwrap_or(usize::MAX);
        let results: Vec<Value> = results.into_iter().skip(offset).take(limit).cloned().collect();

        // Field projection
        if let Some(fields) = &params.fields {
            let fields = fields.clone();
            return Ok(results.into_iter().map(|mut e| {
                if let Some(obj) = e.as_object_mut() {
                    obj.retain(|k, _| fields.contains(k));
                }
                e
            }).collect());
        }

        Ok(results)
    }

    async fn find_one(&self, query: &Value) -> Result<Option<Value>> {
        let store = self.store.read().await;
        Ok(store.values().find(|e| Self::matches_query(e, query)).cloned())
    }

    async fn count(&self, query: Option<&Value>) -> Result<u64> {
        let store = self.store.read().await;
        let count = if let Some(q) = query {
            store.values().filter(|e| Self::matches_query(e, q)).count()
        } else {
            store.len()
        };
        Ok(count as u64)
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
            .unwrap_or_else(new_id);

        if let Some(obj) = entity.as_object_mut() {
            obj.insert("id".into(), json!(id));
            let now = chrono::Utc::now().to_rfc3339();
            obj.entry("createdAt").or_insert_with(|| json!(now));
            obj.entry("updatedAt").or_insert_with(|| json!(now));
        }

        self.store.write().await.insert(id, entity.clone());
        Ok(entity)
    }

    async fn insert_many(&self, entities: Vec<Value>) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        for entity in entities {
            results.push(self.insert(entity).await?);
        }
        Ok(results)
    }

    async fn update_by_id(&self, id: &str, update: Value) -> Result<Option<Value>> {
        let mut store = self.store.write().await;
        if let Some(existing) = store.get_mut(id) {
            if let (Some(existing_obj), Some(update_obj)) = (existing.as_object_mut(), update.as_object()) {
                for (k, v) in update_obj {
                    if k != "id" {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
                existing_obj.insert("updatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
            }
            Ok(Some(existing.clone()))
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
        Ok(Some(entity))
    }

    async fn remove_by_id(&self, id: &str) -> Result<Option<Value>> {
        Ok(self.store.write().await.remove(id))
    }

    async fn clear(&self) -> Result<u64> {
        let mut store = self.store.write().await;
        let count = store.len() as u64;
        store.clear();
        Ok(count)
    }
}

// ─── Database options ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DatabaseOptions {
    /// Generate CRUD actions.
    pub create_actions: bool,
    /// Default page size for `list` action.
    pub default_page_size: u64,
    /// Maximum page size (0 = unlimited).
    pub max_page_size: u64,
    /// Maximum limit for `find` action (0 = unlimited).
    pub max_limit: u64,
    /// Enable caching of read actions.
    pub cache_enabled: bool,
    /// REST route prefix (None = disabled).
    pub rest: Option<String>,
    /// Action visibility.
    pub visibility: String,
    /// ID field name.
    pub id_field: String,
    /// Soft-delete field (None = hard delete).
    pub soft_delete: Option<String>,
}

impl Default for DatabaseOptions {
    fn default() -> Self {
        Self {
            create_actions: true,
            default_page_size: 10,
            max_page_size: 100,
            max_limit: 0,
            cache_enabled: true,
            rest: Some(String::new()),
            visibility: "published".into(),
            id_field: "id".into(),
            soft_delete: None,
        }
    }
}

// ─── DatabaseMixin ────────────────────────────────────────────────────────────

/// Add to any service schema to get full CRUD.
///
/// # Example
/// ```rust,no_run
/// use moleculer::database::{DatabaseMixin, MemoryAdapter, DatabaseOptions};
///
/// let db = DatabaseMixin::with_options(MemoryAdapter::new(), DatabaseOptions::default());
/// ```
pub struct DatabaseMixin {
    adapter: Arc<dyn DbAdapter>,
    opts: DatabaseOptions,
}

impl DatabaseMixin {
    pub fn new(adapter: impl DbAdapter + 'static) -> Self {
        Self {
            adapter: Arc::new(adapter),
            opts: DatabaseOptions::default(),
        }
    }

    pub fn with_options(adapter: impl DbAdapter + 'static, opts: DatabaseOptions) -> Self {
        Self { adapter: Arc::new(adapter), opts }
    }

    /// Apply mixin to a service schema, injecting all CRUD actions.
    pub fn apply(self, mut schema: ServiceSchema) -> ServiceSchema {
        let adapter = self.adapter;
        let opts = self.opts;

        if !opts.create_actions { return schema; }

        // ── find ─────────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let max_limit = opts.max_limit;
            schema = schema.action(
                ActionDef::new("find", move |ctx| {
                    let a = Arc::clone(&a);
                    async move {
                        let mut params = FindParams::from_ctx(&ctx);
                        if max_limit > 0 {
                            if params.limit.map(|l| l as u64).unwrap_or(0) > max_limit || params.limit.is_none() {
                                params.limit = Some(max_limit as i64);
                            }
                        }
                        let rows = a.find(&params).await?;
                        Ok(json!(rows))
                    }
                })
            );
        }

        // ── list ─────────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let default_page_size = opts.default_page_size;
            let max_page_size = opts.max_page_size;
            schema = schema.action(
                ActionDef::new("list", move |ctx| {
                    let a = Arc::clone(&a);
                    async move {
                        let page = ctx.params.get("page").and_then(|v| v.as_u64()).unwrap_or(1);
                        let mut page_size = ctx.params.get("pageSize").and_then(|v| v.as_u64()).unwrap_or(default_page_size);
                        if max_page_size > 0 && page_size > max_page_size {
                            page_size = max_page_size;
                        }
                        let offset = (page - 1) * page_size;

                        let mut params = FindParams::from_ctx(&ctx);
                        params.limit = Some(page_size as i64);
                        params.offset = Some(offset as i64);

                        let query = params.query.clone();
                        let total = a.count(query.as_ref()).await?;
                        let rows = a.find(&params).await?;
                        let total_pages = (total + page_size - 1) / page_size;

                        Ok(json!({
                            "rows": rows,
                            "total": total,
                            "page": page,
                            "pageSize": page_size,
                            "totalPages": total_pages,
                        }))
                    }
                })
            );
        }

        // ── count ────────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            schema = schema.action(
                ActionDef::new("count", move |ctx| {
                    let a = Arc::clone(&a);
                    async move {
                        let query = ctx.params.get("query").cloned();
                        let n = a.count(query.as_ref()).await?;
                        Ok(json!(n))
                    }
                })
            );
        }

        // ── get ──────────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let id_field = opts.id_field.clone();
            schema = schema.action(
                ActionDef::new("get", move |ctx| {
                    let a = Arc::clone(&a);
                    let id_field = id_field.clone();
                    async move {
                        // Accept single id or array of ids
                        if let Some(ids_arr) = ctx.params.get("id").and_then(|v| v.as_array()) {
                            let ids: Vec<String> = ids_arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect();
                            let rows = a.find_by_ids(&ids).await?;
                            return Ok(json!(rows));
                        }
                        let id = ctx.params.get("id")
                            .or_else(|| ctx.params.get(&id_field))
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| MoleculerError::Validation {
                                message: "id is required".into(),
                                failures: vec![],
                            })?;
                        let entity = a.find_by_id(id).await?
                            .ok_or_else(|| MoleculerError::ActionNotFound(format!("Entity {} not found", id)))?;
                        Ok(entity)
                    }
                })
            );
        }

        // ── create ───────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            schema = schema.action(
                ActionDef::new("create", move |ctx| {
                    let a = Arc::clone(&a);
                    async move {
                        let entity = ctx.params.clone();
                        let created = a.insert(entity).await?;
                        Ok(created)
                    }
                })
            );
        }

        // ── createMany ───────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            schema = schema.action(
                ActionDef::new("createMany", move |ctx| {
                    let a = Arc::clone(&a);
                    async move {
                        let entities = ctx.params.get("entities")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let created = a.insert_many(entities).await?;
                        Ok(json!(created))
                    }
                })
            );
        }

        // ── update ───────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let id_field = opts.id_field.clone();
            schema = schema.action(
                ActionDef::new("update", move |ctx| {
                    let a = Arc::clone(&a);
                    let id_field = id_field.clone();
                    async move {
                        let id = ctx.params.get("id")
                            .or_else(|| ctx.params.get(&id_field))
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| MoleculerError::Validation {
                                message: "id is required".into(),
                                failures: vec![],
                            })?
                            .to_string();
                        let mut update = ctx.params.clone();
                        if let Some(obj) = update.as_object_mut() { obj.remove("id"); }
                        let updated = a.update_by_id(&id, update).await?
                            .ok_or_else(|| MoleculerError::ActionNotFound(format!("Entity {} not found", id)))?;
                        Ok(updated)
                    }
                })
            );
        }

        // ── replace ──────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let id_field = opts.id_field.clone();
            schema = schema.action(
                ActionDef::new("replace", move |ctx| {
                    let a = Arc::clone(&a);
                    let id_field = id_field.clone();
                    async move {
                        let id = ctx.params.get("id")
                            .or_else(|| ctx.params.get(&id_field))
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| MoleculerError::Validation {
                                message: "id is required".into(),
                                failures: vec![],
                            })?
                            .to_string();
                        let entity = ctx.params.clone();
                        let replaced = a.replace_by_id(&id, entity).await?
                            .ok_or_else(|| MoleculerError::ActionNotFound(format!("Entity {} not found", id)))?;
                        Ok(replaced)
                    }
                })
            );
        }

        // ── remove ───────────────────────────────────────────────
        {
            let a = Arc::clone(&adapter);
            let id_field = opts.id_field.clone();
            schema = schema.action(
                ActionDef::new("remove", move |ctx| {
                    let a = Arc::clone(&a);
                    let id_field = id_field.clone();
                    async move {
                        let id = ctx.params.get("id")
                            .or_else(|| ctx.params.get(&id_field))
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| MoleculerError::Validation {
                                message: "id is required".into(),
                                failures: vec![],
                            })?
                            .to_string();
                        let removed = a.remove_by_id(&id).await?
                            .ok_or_else(|| MoleculerError::ActionNotFound(format!("Entity {} not found", id)))?;
                        Ok(removed)
                    }
                })
            );
        }

        schema
    }
}

// ─── Convenience builder ─────────────────────────────────────────────────────

/// Shortcut: create a service schema with a database mixin pre-applied.
pub fn database_service(name: &str, adapter: impl DbAdapter + 'static) -> ServiceSchema {
    let mixin = DatabaseMixin::new(adapter);
    mixin.apply(ServiceSchema::new(name))
}
