//! Metrics registry — Counter, Gauge, Histogram.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType { Counter, Gauge, Histogram, Info }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub labels: Vec<(String, String)>,
    pub timestamp: DateTime<Utc>,
    pub description: Option<String>,
    pub unit: Option<String>,
}

struct HistBucket {
    sum: Mutex<f64>,
    count: AtomicU64,
}

pub struct MetricsRegistry {
    pub entries: DashMap<String, MetricEntry>,
    histograms: DashMap<String, HistBucket>,
}

impl MetricsRegistry {
    pub fn new() -> Self { Self { entries: DashMap::new(), histograms: DashMap::new() } }

    fn key(name: &str, labels: &[(String, String)]) -> String {
        if labels.is_empty() { name.to_string() }
        else {
            let l = labels.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(",");
            format!("{}|{}", name, l)
        }
    }

    pub fn increment(&self, name: &str, delta: f64, labels: Vec<(String, String)>) {
        let k = Self::key(name, &labels);
        self.entries.entry(k).and_modify(|e| { e.value += delta; e.timestamp = Utc::now(); })
            .or_insert_with(|| MetricEntry {
                name: name.to_string(), metric_type: MetricType::Counter,
                value: delta, labels, timestamp: Utc::now(), description: None, unit: None,
            });
    }

    pub fn set_gauge(&self, name: &str, value: f64, labels: Vec<(String, String)>) {
        let k = Self::key(name, &labels);
        self.entries.entry(k).and_modify(|e| { e.value = value; e.timestamp = Utc::now(); })
            .or_insert_with(|| MetricEntry {
                name: name.to_string(), metric_type: MetricType::Gauge,
                value, labels, timestamp: Utc::now(), description: None, unit: None,
            });
    }

    /// Record a histogram observation (stores avg for simplicity).
    pub fn observe_histogram(&self, name: &str, value: f64, labels: Vec<(String, String)>) {
        let k = Self::key(name, &labels);
        let hb = self.histograms.entry(k.clone()).or_insert_with(|| HistBucket {
            sum: Mutex::new(0.0), count: AtomicU64::new(0),
        });
        *hb.sum.lock().unwrap() += value;
        let count = hb.count.fetch_add(1, Ordering::Relaxed) + 1;
        let avg = *hb.sum.lock().unwrap() / count as f64;

        self.entries.entry(k).and_modify(|e| { e.value = avg; e.timestamp = Utc::now(); })
            .or_insert_with(|| MetricEntry {
                name: name.to_string(), metric_type: MetricType::Histogram,
                value: avg, labels, timestamp: Utc::now(), description: None, unit: Some("ms".into()),
            });
    }

    pub fn snapshot(&self) -> Vec<MetricEntry> {
        self.entries.iter().map(|e| e.value().clone()).collect()
    }

    /// Prometheus text format (basic).
    pub fn prometheus_text(&self) -> String {
        let mut out = String::new();
        for e in self.snapshot() {
            let labels = if e.labels.is_empty() { String::new() } else {
                let l = e.labels.iter().map(|(k, v)| format!("{}=\"{}\"", k, v)).collect::<Vec<_>>().join(",");
                format!("{{{}}}", l)
            };
            let tname = e.name.replace('.', "_").replace('-', "_");
            out.push_str(&format!("# TYPE {} {}\n", tname, match e.metric_type {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "gauge",
                MetricType::Info => "gauge",
            }));
            out.push_str(&format!("{}{} {}\n", tname, labels, e.value));
        }
        out
    }
}

impl Default for MetricsRegistry { fn default() -> Self { Self::new() } }

pub mod reporters;
pub use reporters::{
    MetricsReporter, ReporterRegistry,
    ConsoleReporter, CsvReporter, StatsdReporter, DatadogMetricsReporter, EventReporter,
};
