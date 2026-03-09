//! Dead-Letter Queue — stores messages that exceeded `max_retries`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntry {
    pub id: String,
    pub channel: String,
    pub original_channel: String,
    pub original_group: String,
    pub payload: Value,
    pub headers: HashMap<String, String>,
    pub failed_at: DateTime<Utc>,
    pub attempts: u32,
}

pub struct DeadLetterQueue {
    entries: Mutex<Vec<DlqEntry>>,
    capacity: usize,
}

impl DeadLetterQueue {
    pub fn new() -> Self { Self { entries: Mutex::new(Vec::new()), capacity: 10_000 } }

    pub fn push(&self, entry: DlqEntry) {
        let mut buf = self.entries.lock().unwrap();
        if buf.len() >= self.capacity { buf.remove(0); }
        log::error!("[DLQ] Message '{}' dead-lettered from '{}' after {} attempts",
            entry.id, entry.original_channel, entry.attempts);
        buf.push(entry);
    }

    pub fn snapshot(&self) -> Vec<DlqEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn drain(&self) -> Vec<DlqEntry> {
        let mut buf = self.entries.lock().unwrap();
        std::mem::take(&mut *buf)
    }

    /// Retry a DLQ entry — re-injects it (caller provides re-publish function).
    pub fn pop_by_id(&self, id: &str) -> Option<DlqEntry> {
        let mut buf = self.entries.lock().unwrap();
        if let Some(pos) = buf.iter().position(|e| e.id == id) {
            Some(buf.remove(pos))
        } else {
            None
        }
    }
}

impl Default for DeadLetterQueue { fn default() -> Self { Self::new() } }
