//! DatabaseMixin — the main Rust equivalent of `@moleculer/database` `DatabaseMixin(opts)`.
//!
//! Generates a complete set of CRUD [`ActionDef`]s that delegate to a [`DbAdapter`].

use super::{FieldDef, FindParams, ScopeParam};
use super::adapter::DbAdapter;
use crate::error::{MoleculerError, Result};
use crate::service::{ActionDef, ServiceSchema};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Options for [`DatabaseMixin`] (mirrors `mixinOpts` in the JS version).
#[derive(Debug, Clone)]
pub struct MixinOptions {
    /// Generate REST CRUD actions (default: true)
    pub create_actions:      bool,
    /// Default page size for `list` action
    pub default_page_size:   i64,
    /// Maximum allowed `limit` (-1 = no limit)
    pub max_limit:           i64,
    /// Soft-delete field name. None = hard delete
    pub soft_delete_field:   Option<String>,
    /// Broadcast entity-change events ("broadcast"|"emit"|None)
    pub entity_changed_event: Option<String>,
    /// Field definitions (None = schema-less)
    pub fields:              Option<HashMap<String, FieldDef>>,
    /// Named scope definitions
    pub scopes:              HashMap<String, Value>,
    /// Default scopes applied to every read
    pub default_scopes:      Vec<String>,
}

impl Default for MixinOptions {
    fn default() -> Self {
        Self {
            create_actions:       true,
            default_page_size:    10,
            max_limit:            -1,
            soft_delete_field:    None,
            entity_changed_event: Some("broadcast".into()),
            fields:               None,
            scopes:               HashMap::new(),
            default_scopes:       Vec::new(),
        }
    }
}

/// The database mixin — wraps an adapter and exposes Moleculer service actions.
pub struct DatabaseMixin {
    name:    String,
    adapter: Arc<dyn DbAdapter>,
    opts:    MixinOptions,
}

impl DatabaseMixin {
    pub fn new(
        name:    impl Into<String>,
        adapter: impl DbAdapter + 'static,
        opts:    MixinOptions,
    ) -> Self {
        Self {
            name:    name.into(),
            adapter: Arc::new(adapter),
            opts,
        }
    }

    /// Convert into a [`ServiceSchema`] with all CRUD actions attached.
    pub fn into_service_schema(self) -> ServiceSchema {
        let adapter = Arc::clone(&self.adapter);
        let opts    = self.opts.clone();

        let mut schema = ServiceSchema::new(&self.name);

        if !opts.create_actions { return schema; }

        // ── find ──────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            let opts    = opts.clone();
            schema = schema.action(ActionDef::new("find", move |ctx| {
                let adapter = Arc::clone(&adapter);
                let opts    = opts.clone();
                async move {
                    let params = FindParams::from_ctx_params(&ctx.params);
                    let limit  = resolve_limit(params.limit, opts.max_limit);
                    let docs   = adapter.find(
                        params.query.as_ref(),
                        params.sort.as_deref(),
                        limit,
                        params.offset,
                        params.fields.as_deref(),
                        params.search.as_deref(),
                        params.search_fields.as_deref(),
                    ).await?;
                    Ok(json!(docs))
                }
            }));
        }

        // ── count ─────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("count", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let params = FindParams::from_ctx_params(&ctx.params);
                    let n = adapter.count(
                        params.query.as_ref(),
                        params.search.as_deref(),
                        params.search_fields.as_deref(),
                    ).await?;
                    Ok(json!({ "count": n }))
                }
            }));
        }

        // ── list (paginated) ──────────────────────────────────────────────────
        {
            let adapter        = Arc::clone(&adapter);
            let page_size_def  = opts.default_page_size;
            let max_limit      = opts.max_limit;
            schema = schema.action(ActionDef::new("list", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let params    = FindParams::from_ctx_params(&ctx.params);
                    let page      = params.page.unwrap_or(1).max(1);
                    let page_size = params.page_size.unwrap_or(page_size_def).max(1);
                    let offset    = (page - 1) * page_size;
                    let limit     = resolve_limit(Some(page_size), max_limit);

                    let total = adapter.count(
                        params.query.as_ref(),
                        params.search.as_deref(),
                        params.search_fields.as_deref(),
                    ).await?;

                    let rows = adapter.find(
                        params.query.as_ref(),
                        params.sort.as_deref(),
                        limit,
                        Some(offset),
                        params.fields.as_deref(),
                        params.search.as_deref(),
                        params.search_fields.as_deref(),
                    ).await?;

                    Ok(json!({
                        "rows":      rows,
                        "total":     total,
                        "page":      page,
                        "pageSize":  page_size,
                        "totalPages": (total as f64 / page_size as f64).ceil() as i64,
                    }))
                }
            }));
        }

        // ── get ───────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("get", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();
                    let doc = adapter.find_by_id(&id).await?
                        .ok_or_else(|| MoleculerError::NotFound(format!("Entity {} not found", id)))?;
                    Ok(doc)
                }
            }));
        }

        // ── resolve (get multiple by ids array) ───────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("resolve", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let ids: Vec<String> = match &ctx.params["id"] {
                        Value::Array(arr) => arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect(),
                        Value::String(s) => vec![s.clone()],
                        _ => return Err(MoleculerError::Validation("Missing id(s)".into())),
                    };
                    let docs = adapter.find_by_ids(&ids).await?;
                    // Return object if single id, array if multiple
                    if ids.len() == 1 {
                        Ok(docs.into_iter().next().unwrap_or(Value::Null))
                    } else {
                        Ok(json!(docs))
                    }
                }
            }));
        }

        // ── create ────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("create", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let entity = ctx.params.clone();
                    let saved  = adapter.insert(entity).await?;
                    Ok(saved)
                }
            }));
        }

        // ── createMany ────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("createMany", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let arr = ctx.params.as_array()
                        .cloned()
                        .ok_or_else(|| MoleculerError::Validation("Expected array of entities".into()))?;
                    let saved = adapter.insert_many(arr).await?;
                    Ok(json!(saved))
                }
            }));
        }

        // ── update ────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("update", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();
                    let changes = ctx.params.clone();
                    let doc = adapter.update_by_id(&id, changes).await?
                        .ok_or_else(|| MoleculerError::NotFound(format!("Entity {} not found", id)))?;
                    Ok(doc)
                }
            }));
        }

        // ── replace ───────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            schema = schema.action(ActionDef::new("replace", move |ctx| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();
                    let entity = ctx.params.clone();
                    let doc = adapter.replace_by_id(&id, entity).await?
                        .ok_or_else(|| MoleculerError::NotFound(format!("Entity {} not found", id)))?;
                    Ok(doc)
                }
            }));
        }

        // ── remove ────────────────────────────────────────────────────────────
        {
            let adapter = Arc::clone(&adapter);
            let opts    = opts.clone();
            schema = schema.action(ActionDef::new("remove", move |ctx| {
                let adapter = Arc::clone(&adapter);
                let soft_field = opts.soft_delete_field.clone();
                async move {
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();

                    if let Some(ref sf) = soft_field {
                        // Soft delete: set the field to current timestamp
                        let now = chrono::Utc::now().timestamp_millis();
                        let changes = json!({ sf: now });
                        let doc = adapter.update_by_id(&id, changes).await?
                            .ok_or_else(|| MoleculerError::NotFound(format!("Entity {} not found", id)))?;
                        Ok(doc)
                    } else {
                        let doc = adapter.remove_by_id(&id).await?
                            .ok_or_else(|| MoleculerError::NotFound(format!("Entity {} not found", id)))?;
                        Ok(doc)
                    }
                }
            }));
        }

        schema
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_limit(limit: Option<i64>, max_limit: i64) -> Option<i64> {
    match (limit, max_limit) {
        (Some(l), max) if max > 0 => Some(l.min(max)),
        (Some(l), _) => Some(l),
        (None, max) if max > 0 => Some(max),
        _ => None,
    }
}

impl FindParams {
    /// Parse from serde_json::Value (ctx.params)
    pub fn from_ctx_params(params: &Value) -> Self {
        serde_json::from_value(params.clone()).unwrap_or_default()
    }
}
