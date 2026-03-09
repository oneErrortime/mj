//! Distributed tracing — spans, parent/child relationships, SpanStore.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub id: String,
    pub trace_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub service: Option<String>,
    pub action: Option<String>,
    pub node_id: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<f64>,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
    pub error: Option<String>,
    pub sampled: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLog {
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

impl Span {
    pub fn new(name: impl Into<String>, trace_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trace_id: trace_id.into(),
            parent_id: None,
            name: name.into(),
            service: None, action: None, node_id: None,
            start_time: Utc::now(),
            end_time: None, duration_ms: None,
            tags: HashMap::new(),
            logs: Vec::new(),
            error: None,
            sampled: true,
            finished: false,
        }
    }

    pub fn finish(&mut self) {
        let now = Utc::now();
        self.end_time = Some(now);
        self.duration_ms = Some(
            (now - self.start_time).num_microseconds().unwrap_or(0) as f64 / 1000.0,
        );
        self.finished = true;
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into()); self
    }

    pub fn set_error(&mut self, err: impl Into<String>) { self.error = Some(err.into()); }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.logs.push(SpanLog { timestamp: Utc::now(), message: msg.into() });
    }
}

/// In-memory span store — Laboratory reads from here.
pub struct SpanStore {
    spans: DashMap<String, Span>,
    max_size: usize,
}

impl SpanStore {
    pub fn new() -> Self { Self { spans: DashMap::new(), max_size: 2000 } }

    pub fn record(&self, span: Span) {
        if self.spans.len() >= self.max_size {
            // Evict oldest finished span
            let oldest = self.spans.iter()
                .filter(|e| e.finished)
                .min_by_key(|e| e.start_time)
                .map(|e| e.key().clone());
            if let Some(k) = oldest { self.spans.remove(&k); }
        }
        self.spans.insert(span.id.clone(), span);
    }

    pub fn snapshot(&self) -> Vec<Span> {
        self.spans.iter().map(|e| e.value().clone()).collect()
    }

    pub fn recent(&self, limit: usize) -> Vec<Span> {
        let mut all = self.snapshot();
        all.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        all.truncate(limit);
        all
    }

    pub fn by_trace(&self, trace_id: &str) -> Vec<Span> {
        self.spans.iter()
            .filter(|e| e.trace_id == trace_id)
            .map(|e| e.value().clone())
            .collect()
    }
}

impl Default for SpanStore { fn default() -> Self { Self::new() } }

pub mod exporters;
pub use exporters::{
    SpanExporter, ExporterRegistry,
    ConsoleExporter, JaegerExporter, ZipkinExporter, DatadogExporter, EventExporter,
};

impl SpanStore {
    /// Drain spans that are finished and return them, leaving unfinished spans in place.
    /// Used by the tracing flush loop in the broker.
    pub fn drain_finished(&self) -> Vec<Span> {
        let finished: Vec<String> = self.spans.iter()
            .filter(|e| e.value().finished)
            .map(|e| e.key().clone())
            .collect();
        let mut result = Vec::with_capacity(finished.len());
        for id in finished {
            if let Some((_, span)) = self.spans.remove(&id) {
                result.push(span);
            }
        }
        result
    }
}
