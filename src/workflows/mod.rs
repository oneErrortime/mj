//! # moleculer-rs workflows module
//!
//! Rust port of `@moleculer/workflows` — a durable job queue backed by SQLite.
//!
//! ## Features
//! - Create / cancel / retry jobs
//! - CRON-style repeat jobs
//! - Parent → child job trees
//! - Signals (inter-job communication)
//! - Configurable retry policy with exponential back-off
//! - Automatic stalled-job detection
//!
//! ## Usage
//! ```rust,no_run
//! use moleculer::workflows::{WorkflowsMiddleware, MiddlewareOptions, WorkflowDef};
//! use moleculer::prelude::*;
//!
//! let mw = WorkflowsMiddleware::new(MiddlewareOptions::default());
//! // Register mw with broker, then define workflows in your ServiceSchema.
//! ```

pub mod adapter;
pub mod middleware;

pub use adapter::SqliteJobAdapter;
pub use middleware::{WorkflowsMiddleware, MiddlewareOptions};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Job model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Waiting,
    Active,
    Completed,
    Failed,
    Delayed,
    Paused,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Waiting   => "waiting",
            Self::Active    => "active",
            Self::Completed => "completed",
            Self::Failed    => "failed",
            Self::Delayed   => "delayed",
            Self::Paused    => "paused",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for JobStatus {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        Ok(match s {
            "active"    => Self::Active,
            "completed" => Self::Completed,
            "failed"    => Self::Failed,
            "delayed"   => Self::Delayed,
            "paused"    => Self::Paused,
            _           => Self::Waiting,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id:            String,
    pub workflow:      String,
    pub params:        serde_json::Value,
    pub status:        JobStatus,
    pub retries:       u32,
    pub max_retries:   u32,
    pub delay_ms:      i64,            // 0 = immediate
    pub promote_at:    Option<i64>,    // epoch ms when to become active
    pub started_at:    Option<i64>,
    pub finished_at:   Option<i64>,
    pub result:        Option<serde_json::Value>,
    pub error:         Option<String>,
    pub parent_id:     Option<String>,
    pub progress:      f64,
    pub created_at:    i64,
    pub updated_at:    i64,
    pub cron:          Option<String>,
    pub repeat_limit:  Option<u32>,
    pub repeat_count:  u32,
    pub stalled_count: u32,
}

/// Options when creating a new job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateJobOptions {
    pub job_id:     Option<String>,
    pub delay:      Option<i64>,       // ms delay before running
    pub retries:    Option<u32>,
    pub cron:       Option<String>,
    pub repeat_limit: Option<u32>,
    pub parent_id:  Option<String>,
    pub timeout:    Option<i64>,
}

/// A workflow definition (schema attached to a service).
pub struct WorkflowDef {
    pub name:       String,
    pub handler:    WorkflowHandler,
    pub max_retries: u32,
    pub timeout_ms: Option<i64>,
    pub concurrency: usize,
    pub backoff_ms: u64,
}

pub type WorkflowHandler = Arc<
    dyn Fn(Job) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::error::Result<serde_json::Value>> + Send>>
    + Send + Sync,
>;

use std::sync::Arc;

impl WorkflowDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Job) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = crate::error::Result<serde_json::Value>> + Send + 'static,
    {
        Self {
            name:        name.into(),
            handler:     Arc::new(move |job| Box::pin(handler(job))),
            max_retries: 3,
            timeout_ms:  None,
            concurrency: 1,
            backoff_ms:  1000,
        }
    }

    pub fn max_retries(mut self, n: u32)          -> Self { self.max_retries = n; self }
    pub fn concurrency(mut self, n: usize)         -> Self { self.concurrency = n; self }
    pub fn timeout_ms(mut self, ms: i64)           -> Self { self.timeout_ms  = Some(ms); self }
    pub fn backoff_ms(mut self, ms: u64)           -> Self { self.backoff_ms  = ms; self }
}
