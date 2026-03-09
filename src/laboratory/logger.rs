//! EventLogger — captures log events and buffers them for the Lab frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub module: Option<String>,
    pub node_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// In-memory ring-buffer log collector.
pub struct EventLogger {
    pub node_id: String,
    buffer: Arc<Mutex<Vec<LogEntry>>>,
    capacity: usize,
}

impl EventLogger {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            capacity: 5_000,
        }
    }

    pub fn log(&self, level: impl Into<String>, message: impl Into<String>, module: Option<String>) {
        let entry = LogEntry {
            level: level.into(),
            message: message.into(),
            module,
            node_id: Some(self.node_id.clone()),
            timestamp: Utc::now(),
        };
        let mut buf = self.buffer.lock().unwrap();
        if buf.len() >= self.capacity {
            buf.remove(0);
        }
        buf.push(entry);
    }

    pub fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let buf = self.buffer.lock().unwrap();
        let start = buf.len().saturating_sub(limit);
        buf[start..].to_vec()
    }

    pub fn clear(&self) {
        self.buffer.lock().unwrap().clear();
    }

    pub fn count(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }
}
