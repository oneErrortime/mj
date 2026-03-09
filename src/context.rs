//! Context — created for each action call / event dispatch.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Context {
    pub id: String,
    pub request_id: String,
    pub action: Option<String>,
    pub event: Option<String>,
    pub node_id: Option<String>,
    /// Caller service full name.
    pub caller: Option<String>,
    /// Call level (root = 1, incremented on every sub-call).
    pub level: u32,
    pub params: Value,
    pub meta: HashMap<String, Value>,
    /// Custom headers (channels support).
    pub headers: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    /// Timeout override in ms (0 = broker default).
    pub timeout: u64,
    /// Remaining retries.
    pub retries_left: u32,
    /// Number of retry attempts so far.
    pub retry_attempts: u32,
    // --- tracing ---
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub trace_id: Option<String>,
    pub sampled: bool,
    // --- channel context ---
    pub channel_name: Option<String>,
    pub channel_group: Option<String>,
}

impl Context {
    pub fn new(action: impl Into<String>, params: Value) -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            request_id: id.clone(),
            id,
            action: Some(action.into()),
            event: None,
            node_id: None,
            caller: None,
            level: 1,
            params,
            meta: HashMap::new(),
            headers: HashMap::new(),
            created_at: Utc::now(),
            timeout: 0,
            retries_left: 0,
            retry_attempts: 0,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: None,
            trace_id: Some(Uuid::new_v4().to_string()),
            sampled: true,
            channel_name: None,
            channel_group: None,
        }
    }

    /// Create a child context for sub-calls — level++ and inherits meta / trace.
    pub fn child(&self, action: impl Into<String>, params: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            request_id: self.request_id.clone(),
            action: Some(action.into()),
            event: None,
            node_id: self.node_id.clone(),
            caller: self.caller.clone(),
            level: self.level + 1,
            params,
            meta: self.meta.clone(),
            headers: self.headers.clone(),
            created_at: Utc::now(),
            timeout: self.timeout,
            retries_left: self.retries_left,
            retry_attempts: 0,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: self.span_id.clone(),
            trace_id: self.trace_id.clone(),
            sampled: self.sampled,
            channel_name: None,
            channel_group: None,
        }
    }

    pub fn for_event(event: impl Into<String>, payload: Value) -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            id: id.clone(), request_id: id,
            action: None,
            event: Some(event.into()),
            node_id: None, caller: None, level: 1,
            params: payload,
            meta: HashMap::new(), headers: HashMap::new(),
            created_at: Utc::now(), timeout: 0,
            retries_left: 0, retry_attempts: 0,
            span_id: Some(Uuid::new_v4().to_string()),
            parent_span_id: None,
            trace_id: Some(Uuid::new_v4().to_string()),
            sampled: true,
            channel_name: None, channel_group: None,
        }
    }

    pub fn set_meta(&mut self, key: impl Into<String>, value: Value) -> &mut Self {
        self.meta.insert(key.into(), value); self
    }
    pub fn get_meta(&self, key: &str) -> Option<&Value> { self.meta.get(key) }
    pub fn set_header(&mut self, key: impl Into<String>, value: impl Into<String>) { self.headers.insert(key.into(), value.into()); }
    pub fn get_header(&self, key: &str) -> Option<&str> { self.headers.get(key).map(|s| s.as_str()) }
    pub fn elapsed_ms(&self) -> i64 { (Utc::now() - self.created_at).num_milliseconds() }

    pub fn copy(&self) -> Self { self.clone() }

    pub fn snapshot(&self) -> ContextSnapshot {
        ContextSnapshot {
            id: self.id.clone(), request_id: self.request_id.clone(),
            action: self.action.clone(), event: self.event.clone(),
            node_id: self.node_id.clone(), caller: self.caller.clone(),
            level: self.level, span_id: self.span_id.clone(),
            parent_span_id: self.parent_span_id.clone(),
            trace_id: self.trace_id.clone(),
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: String,
    pub request_id: String,
    pub action: Option<String>,
    pub event: Option<String>,
    pub node_id: Option<String>,
    pub caller: Option<String>,
    pub level: u32,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
