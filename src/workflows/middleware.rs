//! WorkflowsMiddleware — the main Rust equivalent of `@moleculer/workflows` `WorkflowsMiddleware(opts)`.
//!
//! Exposes a service mixin with job-management actions and runs a background
//! worker loop that processes waiting jobs.

use super::{CreateJobOptions, Job, SqliteJobAdapter, WorkflowDef};
use crate::error::{MoleculerError, Result};
use crate::service::{ActionDef, ServiceSchema};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Options for the WorkflowsMiddleware (mirrors `WorkflowsMiddlewareOptions`).
#[derive(Clone, Debug)]
pub struct MiddlewareOptions {
    /// Path to SQLite file (None = in-memory)
    pub db_path:             Option<String>,
    /// How often the worker loop polls (ms)
    pub poll_interval_ms:    u64,
    /// Maximum stalled-job count before marking failed
    pub max_stalled_count:   u32,
    /// Stale job threshold (ms) — jobs Active > this are considered stalled
    pub stall_interval_ms:   u64,
    /// Signal expiry (seconds)
    pub signal_expiry_secs:  Option<i64>,
}

impl Default for MiddlewareOptions {
    fn default() -> Self {
        Self {
            db_path:            None,
            poll_interval_ms:   500,
            max_stalled_count:  1,
            stall_interval_ms:  30_000,
            signal_expiry_secs: Some(3600),
        }
    }
}

/// Runtime state shared between the service actions and the worker loop.
struct WorkflowsState {
    adapter:   Arc<SqliteJobAdapter>,
    workflows: HashMap<String, WorkflowDef>,
    opts:      MiddlewareOptions,
}

/// The workflows middleware — registers job management actions and runs a worker.
pub struct WorkflowsMiddleware {
    state: Arc<RwLock<WorkflowsState>>,
}

impl WorkflowsMiddleware {
    pub fn new(opts: MiddlewareOptions) -> Self {
        let adapter = if let Some(ref path) = opts.db_path {
            SqliteJobAdapter::file(path).expect("open workflows DB")
        } else {
            SqliteJobAdapter::memory()
        };

        Self {
            state: Arc::new(RwLock::new(WorkflowsState {
                adapter:   Arc::new(adapter),
                workflows: HashMap::new(),
                opts,
            })),
        }
    }

    /// Register a workflow definition (must be called before start).
    pub async fn register(&self, def: WorkflowDef) {
        let mut s = self.state.write().await;
        s.workflows.insert(def.name.clone(), def);
    }

    /// Start the background worker loop (spawns a Tokio task).
    pub(crate) fn start_worker(state: Arc<RwLock<WorkflowsState>>) {
        tokio::spawn(async move {
            loop {
                let poll_ms = {
                    let s = state.read().await;
                    s.opts.poll_interval_ms
                };

                // Process one job per registered workflow per tick
                let (workflow_names, adapter, opts) = {
                    let s = state.read().await;
                    let names: Vec<String> = s.workflows.keys().cloned().collect();
                    (names, Arc::clone(&s.adapter), s.opts.clone())
                };

                for wf_name in &workflow_names {
                    // Check for stalled jobs first
                    Self::reclaim_stalled_jobs(&adapter, wf_name, &opts).await;

                    // Claim next waiting job
                    match adapter.claim_next_job(wf_name) {
                        Ok(Some(job)) => {
                            let adapter2 = Arc::clone(&adapter);
                            let job_id   = job.id.clone();
                            let wf_name2 = wf_name.clone();

                            // Look up handler
                            let handler = {
                                let s = state.read().await;
                                s.workflows.get(wf_name).map(|w| Arc::clone(&w.handler))
                            };

                            if let Some(h) = handler {
                                tokio::spawn(async move {
                                    match h(job).await {
                                        Ok(result) => {
                                            if let Err(e) = adapter2.complete_job(&wf_name2, &job_id, result) {
                                                log::error!("[workflows] complete_job {job_id}: {e}");
                                            }
                                        }
                                        Err(e) => {
                                            if let Err(e2) = adapter2.fail_job(&wf_name2, &job_id, &e.to_string()) {
                                                log::error!("[workflows] fail_job {job_id}: {e2}");
                                            }
                                        }
                                    }
                                });
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log::error!("[workflows] claim_next_job {wf_name}: {e}"),
                    }
                }

                tokio::time::sleep(Duration::from_millis(poll_ms)).await;
            }
        });
    }

    async fn reclaim_stalled_jobs(adapter: &SqliteJobAdapter, workflow: &str, opts: &MiddlewareOptions) {
        // Jobs that have been Active for too long are considered stalled
        let now = chrono::Utc::now().timestamp_millis();
        if let Ok(active_jobs) = adapter.list_jobs(workflow, Some("active"), 50, 0) {
            for mut job in active_jobs {
                let stalled = job.started_at
                    .map_or(false, |s| now - s > opts.stall_interval_ms as i64);
                if stalled {
                    job.stalled_count += 1;
                    if job.stalled_count >= opts.max_stalled_count {
                        let _ = adapter.fail_job(workflow, &job.id, "Job stalled");
                        log::warn!("[workflows] Stalled job {} marked failed", job.id);
                    } else {
                        // Re-queue
                        job.status = super::JobStatus::Waiting;
                        job.started_at = None;
                        // Adapter will save via fail + retry path
                    }
                }
            }
        }
    }

    /// Build a `ServiceSchema` that exposes job management actions.
    pub async fn into_service_schema(self, service_name: &str) -> ServiceSchema {
        let state  = Arc::clone(&self.state);

        // Start the worker
        Self::start_worker(Arc::clone(&state));

        let mut schema = ServiceSchema::new(service_name);

        // ── createJob ─────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("createJob", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let workflow = ctx.params["workflow"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing workflow".into()))?
                        .to_string();
                    let params = ctx.params["params"].clone();
                    let opts: CreateJobOptions = serde_json::from_value(ctx.params.clone())
                        .unwrap_or_default();
                    let s   = state.read().await;
                    let job = s.adapter.create_job(&workflow, params, opts)?;
                    Ok(serde_json::to_value(&job)
                        .map_err(|e| MoleculerError::Internal(e.to_string()))?)
                }
            }));
        }

        // ── getJob ────────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("getJob", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let workflow = ctx.params["workflow"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing workflow".into()))?
                        .to_string();
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();
                    let s   = state.read().await;
                    let job = s.adapter.get_job(&workflow, &id)?
                        .ok_or_else(|| MoleculerError::NotFound(format!("Job {id} not found")))?;
                    Ok(serde_json::to_value(&job)
                        .map_err(|e| MoleculerError::Internal(e.to_string()))?)
                }
            }));
        }

        // ── listJobs ──────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("listJobs", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let workflow = ctx.params["workflow"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing workflow".into()))?
                        .to_string();
                    let status = ctx.params["status"].as_str().map(String::from);
                    let limit  = ctx.params["limit"].as_i64().unwrap_or(20);
                    let offset = ctx.params["offset"].as_i64().unwrap_or(0);
                    let s    = state.read().await;
                    let jobs = s.adapter.list_jobs(&workflow, status.as_deref(), limit, offset)?;
                    Ok(json!(jobs))
                }
            }));
        }

        // ── cancelJob ─────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("cancelJob", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let workflow = ctx.params["workflow"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing workflow".into()))?
                        .to_string();
                    let id = ctx.params["id"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing id".into()))?
                        .to_string();
                    let s = state.read().await;
                    let ok = s.adapter.cancel_job(&workflow, &id)?;
                    Ok(json!({ "cancelled": ok }))
                }
            }));
        }

        // ── jobCounts ─────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("jobCounts", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let workflow = ctx.params["workflow"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing workflow".into()))?
                        .to_string();
                    let s = state.read().await;
                    s.adapter.job_counts(&workflow)
                }
            }));
        }

        // ── triggerSignal ─────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("triggerSignal", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let name    = ctx.params["name"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing name".into()))?
                        .to_string();
                    let key     = ctx.params["key"].as_str().map(String::from);
                    let payload = ctx.params.get("payload").cloned();
                    let s = state.read().await;
                    let exp = s.opts.signal_expiry_secs;
                    s.adapter.trigger_signal(&name, key.as_deref(), payload, exp)?;
                    Ok(json!({ "ok": true }))
                }
            }));
        }

        // ── getSignal ─────────────────────────────────────────────────────────
        {
            let state = Arc::clone(&state);
            schema = schema.action(ActionDef::new("getSignal", move |ctx| {
                let state = Arc::clone(&state);
                async move {
                    let name = ctx.params["name"].as_str()
                        .ok_or_else(|| MoleculerError::Validation("Missing name".into()))?
                        .to_string();
                    let key  = ctx.params["key"].as_str().map(String::from);
                    let s    = state.read().await;
                    let val  = s.adapter.get_signal(&name, key.as_deref())?;
                    Ok(val.unwrap_or(Value::Null))
                }
            }));
        }

        schema
    }
}
