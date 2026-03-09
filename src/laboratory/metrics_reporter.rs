use crate::metrics::{MetricEntry, MetricsRegistry};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot { pub node_id: String, pub timestamp: chrono::DateTime<chrono::Utc>, pub metrics: Vec<MetricEntry> }
pub struct MetricReporter { node_id: String, registry: Arc<MetricsRegistry> }
impl MetricReporter {
    pub fn new(node_id: impl Into<String>, registry: Arc<MetricsRegistry>) -> Self { Self { node_id: node_id.into(), registry } }
    pub fn snapshot(&self) -> MetricsSnapshot { MetricsSnapshot { node_id: self.node_id.clone(), timestamp: chrono::Utc::now(), metrics: self.registry.snapshot() } }
}
