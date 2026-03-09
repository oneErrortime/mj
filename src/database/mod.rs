//! # moleculer-rs database module
//!
//! Rust port of `@moleculer/database` — a full CRUD mixin with pluggable
//! storage adapters, field validation, scopes, populate, and entity-change events.
//!
//! ## Implemented adapters
//! - [`SqliteAdapter`] — embedded SQLite (no external server required)
//!
//! ## Usage
//! ```rust,no_run
//! use moleculer::database::{DatabaseMixin, SqliteAdapter, MixinOptions};
//! use moleculer::prelude::*;
//!
//! let adapter = SqliteAdapter::memory();
//! let mixin   = DatabaseMixin::new("posts", adapter, MixinOptions::default());
//! let schema  = mixin.into_service_schema();
//! ```

pub mod adapter;
pub mod mixin;

pub use adapter::SqliteAdapter;
pub use mixin::{DatabaseMixin, MixinOptions};

// ─── field definition types ───────────────────────────────────────────────────
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Single field definition (mirrors moleculer-database `fields` object).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    /// JSON Schema type: "string"|"number"|"boolean"|"object"|"array"
    pub field_type:  FieldType,
    pub required:    bool,
    pub primary_key: bool,
    pub read_only:   bool,
    pub virtual_f:   bool,          // computed, not stored
    pub default:     Option<serde_json::Value>,
    pub min:         Option<f64>,
    pub max:         Option<f64>,
    pub min_length:  Option<usize>,
    pub max_length:  Option<usize>,
    pub hidden:      bool,          // strip from GET results
    pub secure:      bool,          // strip unless owner/admin
    pub populate:    Option<PopulateDef>,
    pub immutable:   bool,          // can't be changed after creation
    pub set:         Option<String>, // JS-style "set" hook name (future)
    pub get:         Option<String>, // JS-style "get" hook name (future)
}

impl Default for FieldDef {
    fn default() -> Self {
        Self {
            field_type:  FieldType::Any,
            required:    false,
            primary_key: false,
            read_only:   false,
            virtual_f:   false,
            default:     None,
            min:         None,
            max:         None,
            min_length:  None,
            max_length:  None,
            hidden:      false,
            secure:      false,
            populate:    None,
            immutable:   false,
            set:         None,
            get:         None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType { String, Number, Boolean, Object, Array, Any }

/// Populate definition — fetches a related entity from another service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulateDef {
    /// Target service action to call (e.g. "users.get")
    pub action:     String,
    /// Field on the current entity that holds the foreign key (default: field name)
    pub key_field:  Option<String>,
    /// Params to merge into the call
    pub params:     Option<serde_json::Value>,
}

/// Scope definition — pre-set query filters for security / soft-delete.
#[derive(Debug, Clone)]
pub struct ScopeDef {
    pub name:  String,
    pub query: serde_json::Value,
}

/// Find / list parameters — mirrors the JS `findEntities` params.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindParams {
    pub limit:        Option<i64>,
    pub offset:       Option<i64>,
    pub sort:         Option<Vec<String>>,
    pub fields:       Option<Vec<String>>,
    pub search:       Option<String>,
    pub search_fields: Option<Vec<String>>,
    pub scope:        Option<ScopeParam>,
    pub populate:     Option<Vec<String>>,
    pub query:        Option<serde_json::Value>,
    pub page:         Option<i64>,
    pub page_size:    Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScopeParam {
    Bool(bool),
    Name(String),
    Names(Vec<String>),
}
