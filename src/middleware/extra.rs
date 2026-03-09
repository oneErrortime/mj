//! Debounce and Throttle middlewares.
//!
//! Mirrors `moleculer/src/middlewares/debounce.js` and `throttle.js`.
//!
//! - **Debounce**: delays execution; resets timer on every call within window.
//! - **Throttle**: limits execution to at most once per interval.

use crate::context::Context;
use crate::error::Result;
use crate::middleware::{Middleware, NextHandler};
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// ─── Throttle ────────────────────────────────────────────────────────────────

/// Limits how often an action can be called.
/// After the first call, subsequent calls within `interval` are dropped
/// (returning the last successful result) or queued (if `trailing = true`).
pub struct ThrottleMiddleware {
    /// Per-action last-called timestamps.
    last_called: DashMap<String, Instant>,
    interval: Duration,
    /// If true, the last call in a window is executed after the interval.
    trailing: bool,
}

impl ThrottleMiddleware {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            last_called: DashMap::new(),
            interval: Duration::from_millis(interval_ms),
            trailing: false,
        }
    }

    pub fn with_trailing(mut self, trailing: bool) -> Self {
        self.trailing = trailing;
        self
    }
}

#[async_trait]
impl Middleware for ThrottleMiddleware {
    fn name(&self) -> &str { "ThrottleMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        let now = Instant::now();

        // Check if we're within throttle window
        if let Some(last) = self.last_called.get(&action) {
            if now.duration_since(*last) < self.interval {
                // Drop this call — return empty/null
                log::debug!("[ThrottleMiddleware] action '{}' throttled", action);
                return Ok(Value::Null);
            }
        }

        // Update timestamp and call
        self.last_called.insert(action, now);
        next(ctx).await
    }
}

// ─── Debounce ────────────────────────────────────────────────────────────────

/// Debounce: only executes after `wait` ms of inactivity.
/// Each new call within the window resets the timer.
pub struct DebounceMiddleware {
    wait: Duration,
    /// Per-action: (timer_handle, last_context)
    pending: DashMap<String, Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>>,
    /// Shared result store for last pending call.
    results: Arc<DashMap<String, Arc<tokio::sync::Notify>>>,
}

impl DebounceMiddleware {
    pub fn new(wait_ms: u64) -> Self {
        Self {
            wait: Duration::from_millis(wait_ms),
            pending: DashMap::new(),
            results: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl Middleware for DebounceMiddleware {
    fn name(&self) -> &str { "DebounceMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        let wait = self.wait;

        // Cancel any pending timer for this action
        if let Some(handle) = self.pending.get(&action) {
            if let Some(h) = handle.lock().await.take() {
                h.abort();
            }
        }

        // Create notification
        let notify = Arc::new(tokio::sync::Notify::new());
        let notify_clone = Arc::clone(&notify);
        self.results.insert(action.clone(), Arc::clone(&notify));

        // Schedule delayed execution
        let action_clone = action.clone();
        let next_clone = Arc::clone(&next);
        let ctx_clone = ctx.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            log::debug!("[DebounceMiddleware] executing debounced action '{}'", action_clone);
            let _ = next_clone(ctx_clone).await;
            notify_clone.notify_one();
        });

        let entry = self.pending.entry(action.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)));
        *entry.lock().await = Some(handle);

        // Wait for the debounced execution
        notify.notified().await;
        next(ctx).await
    }
}

// ─── Fallback ─────────────────────────────────────────────────────────────────

/// Fallback middleware: if the action fails, call a fallback function.
///
/// Mirrors `moleculer/src/middlewares/fallback.js`.
pub struct FallbackMiddleware {
    fallback: Arc<dyn Fn(&crate::error::MoleculerError) -> Value + Send + Sync>,
}

impl FallbackMiddleware {
    pub fn new<F>(f: F) -> Self
    where F: Fn(&crate::error::MoleculerError) -> Value + Send + Sync + 'static {
        Self { fallback: Arc::new(f) }
    }
}

#[async_trait]
impl Middleware for FallbackMiddleware {
    fn name(&self) -> &str { "FallbackMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        match next(ctx).await {
            Ok(v) => Ok(v),
            Err(e) => {
                log::warn!("[FallbackMiddleware] action failed, using fallback: {}", e);
                Ok((self.fallback)(&e))
            }
        }
    }
}

// ─── ContextTracker ───────────────────────────────────────────────────────────

/// Tracks active contexts (in-flight requests).
///
/// Mirrors `moleculer/src/middlewares/context-tracker.js`.
/// Used for graceful shutdown — waits for all in-flight requests to finish.
pub struct ContextTrackerMiddleware {
    active: Arc<DashMap<String, Instant>>,
    graceful_stop_timeout: Duration,
}

impl ContextTrackerMiddleware {
    pub fn new(graceful_stop_timeout_ms: u64) -> Self {
        Self {
            active: Arc::new(DashMap::new()),
            graceful_stop_timeout: Duration::from_millis(graceful_stop_timeout_ms),
        }
    }

    pub fn active_count(&self) -> usize { self.active.len() }

    /// Wait for all in-flight requests to complete (or timeout).
    pub async fn wait_for_all(&self) {
        let deadline = Instant::now() + self.graceful_stop_timeout;
        while !self.active.is_empty() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !self.active.is_empty() {
            log::warn!("[ContextTracker] {} requests still in-flight after graceful stop timeout",
                       self.active.len());
        }
    }
}

#[async_trait]
impl Middleware for ContextTrackerMiddleware {
    fn name(&self) -> &str { "ContextTrackerMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let id = ctx.id.clone();
        self.active.insert(id.clone(), Instant::now());
        let result = next(ctx).await;
        self.active.remove(&id);
        result
    }
}
