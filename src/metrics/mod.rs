//! Metrics collection — mirrors Moleculer's metrics module.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub labels: Vec<(String, String)>,
    pub timestamp: DateTime<Utc>,
    pub description: Option<String>,
}

/// Simple in-memory metrics registry.
pub struct MetricsRegistry {
    pub entries: DashMap<String, MetricEntry>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self { entries: DashMap::new() }
    }

    pub fn increment(&self, name: &str, delta: f64, labels: Vec<(String, String)>) {
        let key = Self::make_key(name, &labels);
        self.entries
            .entry(key)
            .and_modify(|e| e.value += delta)
            .or_insert_with(|| MetricEntry {
                name: name.to_string(),
                metric_type: MetricType::Counter,
                value: delta,
                labels,
                timestamp: Utc::now(),
                description: None,
            });
    }

    pub fn set_gauge(&self, name: &str, value: f64, labels: Vec<(String, String)>) {
        let key = Self::make_key(name, &labels);
        self.entries
            .entry(key)
            .and_modify(|e| { e.value = value; e.timestamp = Utc::now(); })
            .or_insert_with(|| MetricEntry {
                name: name.to_string(),
                metric_type: MetricType::Gauge,
                value,
                labels,
                timestamp: Utc::now(),
                description: None,
            });
    }

    pub fn snapshot(&self) -> Vec<MetricEntry> {
        self.entries.iter().map(|e| e.value().clone()).collect()
    }

    fn make_key(name: &str, labels: &[(String, String)]) -> String {
        if labels.is_empty() {
            name.to_string()
        } else {
            let label_str = labels.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}|{}", name, label_str)
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self { Self::new() }
}
