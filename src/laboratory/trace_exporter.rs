//! TraceExporter — exposes tracing spans to the Laboratory Agent.

use crate::tracing::{Span, SpanStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingSnapshot {
    pub node_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub spans: Vec<Span>,
}

pub struct TraceExporter {
    node_id: String,
    store: Arc<SpanStore>,
}

impl TraceExporter {
    pub fn new(node_id: impl Into<String>, store: Arc<SpanStore>) -> Self {
        Self { node_id: node_id.into(), store }
    }

    pub fn snapshot(&self, limit: usize) -> TracingSnapshot {
        TracingSnapshot {
            node_id: self.node_id.clone(),
            timestamp: chrono::Utc::now(),
            spans: self.store.recent(limit),
        }
    }
}
