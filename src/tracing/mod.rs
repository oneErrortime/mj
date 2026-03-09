//! Distributed tracing — spans and exporters.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
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
    pub tags: Vec<(String, String)>,
    pub error: Option<String>,
    pub finished: bool,
}

impl Span {
    pub fn new(name: impl Into<String>, trace_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trace_id: trace_id.into(),
            parent_id: None,
            name: name.into(),
            service: None,
            action: None,
            node_id: None,
            start_time: Utc::now(),
            end_time: None,
            duration_ms: None,
            tags: Vec::new(),
            error: None,
            finished: false,
        }
    }

    pub fn finish(&mut self) {
        let now = Utc::now();
        self.end_time = Some(now);
        self.duration_ms = Some((now - self.start_time).num_microseconds().unwrap_or(0) as f64 / 1000.0);
        self.finished = true;
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.push((key.into(), value.into()));
        self
    }

    pub fn set_error(&mut self, err: impl Into<String>) {
        self.error = Some(err.into());
    }
}

/// In-memory span store (Laboratory exporter reads from here).
pub struct SpanStore {
    pub spans: DashMap<String, Span>,
}

impl SpanStore {
    pub fn new() -> Self { Self { spans: DashMap::new() } }

    pub fn record(&self, span: Span) {
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
}

impl Default for SpanStore {
    fn default() -> Self { Self::new() }
}
