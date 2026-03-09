//! Context — created for each action call or event, carries params and metadata.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Context represents a single action call or event dispatch.
/// Every handler receives a Context containing params, metadata,
/// caller info, and convenience methods for sub-calls.
#[derive(Debug, Clone)]
pub struct Context {
    /// Unique context ID.
    pub id: String,
    /// Name of the called action or event.
    pub action: Option<String>,
    /// Name of the event (if this context is for an event).
    pub event: Option<String>,
    /// Node ID of the originator.
    pub node_id: Option<String>,
    /// Call depth (starts at 0).
    pub level: u32,
    /// Action parameters / event payload.
    pub params: Value,
    /// Arbitrary metadata passed between services.
    pub meta: HashMap<String, Value>,
    /// When this context was created.
    pub created_at: DateTime<Utc>,
    /// Request timeout in ms (0 = use broker default).
    pub timeout: u64,
    /// How many retries remain.
    pub retries_left: u32,
    /// Tracing span ID (if tracing is enabled).
    pub span_id: Option<String>,
    /// Parent span ID.
    pub parent_span_id: Option<String>,
    /// Whether this is a streaming context.
    pub is_streaming: bool,
}

impl Context {
    /// Create a new root context.
    pub fn new(action: impl Into<String>, params: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action: Some(action.into()),
            event: None,
            node_id: None,
            level: 0,
            params,
            meta: HashMap::new(),
            created_at: Utc::now(),
            timeout: 0,
            retries_left: 0,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: None,
            is_streaming: false,
        }
    }

    /// Create a child context for sub-calls.
    pub fn child(&self, action: impl Into<String>, params: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action: Some(action.into()),
            event: None,
            node_id: self.node_id.clone(),
            level: self.level + 1,
            params,
            meta: self.meta.clone(),
            created_at: Utc::now(),
            timeout: self.timeout,
            retries_left: self.retries_left,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: self.span_id.clone(),
            is_streaming: false,
        }
    }

    /// Create an event context.
    pub fn for_event(event: impl Into<String>, payload: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action: None,
            event: Some(event.into()),
            node_id: None,
            level: 0,
            params: payload,
            meta: HashMap::new(),
            created_at: Utc::now(),
            timeout: 0,
            retries_left: 0,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: None,
            is_streaming: false,
        }
    }

    /// Set a metadata value.
    pub fn set_meta(&mut self, key: impl Into<String>, value: Value) -> &mut Self {
        self.meta.insert(key.into(), value);
        self
    }

    /// Get a metadata value.
    pub fn get_meta(&self, key: &str) -> Option<&Value> {
        self.meta.get(key)
    }

    /// Elapsed time in milliseconds since context creation.
    pub fn elapsed_ms(&self) -> i64 {
        (Utc::now() - self.created_at).num_milliseconds()
    }

    /// Convert to a serializable snapshot for logging / tracing.
    pub fn snapshot(&self) -> ContextSnapshot {
        ContextSnapshot {
            id: self.id.clone(),
            action: self.action.clone(),
            event: self.event.clone(),
            node_id: self.node_id.clone(),
            level: self.level,
            span_id: self.span_id.clone(),
            parent_span_id: self.parent_span_id.clone(),
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: String,
    pub action: Option<String>,
    pub event: Option<String>,
    pub node_id: Option<String>,
    pub level: u32,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
