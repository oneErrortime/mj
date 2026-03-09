//! Action hook middleware.
//!
//! Mirrors `moleculer/src/middlewares/action-hook.js`.
//!
//! Runs `before`, `after`, and `error` hooks defined on a ServiceSchema
//! around every action invocation. Hooks can be:
//!
//! - **Global** — applies to every action in the service.
//! - **Per-action** — applies only to a named action.
//! - **After** — receives the result and can modify/replace it.
//! - **Error** — receives the error and can transform or swallow it.
//!
//! ## Example (service-level definition)
//!
//! ```rust,no_run
//! ServiceSchema::new("users")
//!     .before_all(|ctx| async move {
//!         log::debug!("before all actions: {}", ctx.action.as_deref().unwrap_or("?"));
//!         Ok(ctx)
//!     })
//!     .after("create", |ctx, result| async move {
//!         // e.g. enrich result with extra fields
//!         Ok(result)
//!     })
//!     .error_hook("create", |ctx, err| async move {
//!         log::error!("create failed: {}", err);
//!         Err(err)
//!     })
//! ```

use super::{Middleware, NextHandler};
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type HookFn<T> = Arc<dyn Fn(Context, T) -> Pin<Box<dyn Future<Output = Result<T>> + Send>> + Send + Sync>;
pub type BeforeHookFn  = Arc<dyn Fn(Context) -> Pin<Box<dyn Future<Output = Result<Context>> + Send>> + Send + Sync>;
pub type AfterHookFn   = HookFn<Value>;
pub type ErrorHookFn   = Arc<dyn Fn(Context, MoleculerError) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync>;

// ─── HookSet ─────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct HookSet {
    /// Runs before the action; can modify context.
    pub before: Vec<BeforeHookFn>,
    /// Runs after a successful action; can modify the result.
    pub after: Vec<AfterHookFn>,
    /// Runs after a failed action; can transform or swallow the error.
    pub error: Vec<ErrorHookFn>,
}

// ─── ActionHookMiddleware ─────────────────────────────────────────────────────

/// Middleware that runs before/after/error hooks on each action call.
pub struct ActionHookMiddleware {
    /// Global hooks (run for every action).
    global: HookSet,
    /// Per-action hooks keyed by action name (short or full).
    per_action: HashMap<String, HookSet>,
}

impl ActionHookMiddleware {
    pub fn new() -> Self {
        Self { global: HookSet::default(), per_action: HashMap::new() }
    }

    // ── Builder helpers ───────────────────────────────────────────────────────

    pub fn before_all<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Context>> + Send + 'static,
    {
        self.global.before.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    pub fn after_all<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Context, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        self.global.after.push(Arc::new(move |ctx, v| Box::pin(f(ctx, v))));
        self
    }

    pub fn error_all<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Context, MoleculerError) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        self.global.error.push(Arc::new(move |ctx, e| Box::pin(f(ctx, e))));
        self
    }

    pub fn before<F, Fut>(mut self, action: &str, f: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Context>> + Send + 'static,
    {
        let set = self.per_action.entry(action.to_string()).or_default();
        set.before.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    pub fn after<F, Fut>(mut self, action: &str, f: F) -> Self
    where
        F: Fn(Context, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let set = self.per_action.entry(action.to_string()).or_default();
        set.after.push(Arc::new(move |ctx, v| Box::pin(f(ctx, v))));
        self
    }

    pub fn error_hook<F, Fut>(mut self, action: &str, f: F) -> Self
    where
        F: Fn(Context, MoleculerError) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let set = self.per_action.entry(action.to_string()).or_default();
        set.error.push(Arc::new(move |ctx, e| Box::pin(f(ctx, e))));
        self
    }

    // ── Execution helpers ─────────────────────────────────────────────────────

    async fn run_before_hooks(&self, mut ctx: Context, action: &str) -> Result<Context> {
        // Global before
        for hook in &self.global.before {
            ctx = hook(ctx).await?;
        }
        // Per-action before
        if let Some(set) = self.per_action.get(action) {
            for hook in &set.before {
                ctx = hook(ctx).await?;
            }
        }
        Ok(ctx)
    }

    async fn run_after_hooks(&self, ctx: Context, mut result: Value, action: &str) -> Result<Value> {
        // Per-action after (innermost first)
        if let Some(set) = self.per_action.get(action) {
            for hook in &set.after {
                result = hook(ctx.clone(), result).await?;
            }
        }
        // Global after
        for hook in &self.global.after {
            result = hook(ctx.clone(), result).await?;
        }
        Ok(result)
    }

    async fn run_error_hooks(&self, ctx: Context, err: MoleculerError, action: &str) -> Result<Value> {
        // Per-action error hooks
        if let Some(set) = self.per_action.get(action) {
            for hook in &set.error {
                let r = hook(ctx.clone(), err.clone()).await;
                if r.is_ok() { return r; } // hook swallowed the error
            }
        }
        // Global error hooks
        for hook in &self.global.error {
            let r = hook(ctx.clone(), err.clone()).await;
            if r.is_ok() { return r; }
        }
        Err(err)
    }
}

#[async_trait]
impl Middleware for ActionHookMiddleware {
    fn name(&self) -> &str { "ActionHook" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        // Short name (last segment after dot)
        let short = action.rsplit('.').next().unwrap_or(&action).to_string();

        // Run before hooks (may return Err which short-circuits)
        let ctx = match self.run_before_hooks(ctx, &short).await {
            Ok(c) => c,
            Err(e) => return Err(e),
        };

        // Call next middleware / action handler
        let result = next(ctx.clone()).await;

        // Run after/error hooks
        match result {
            Ok(value) => self.run_after_hooks(ctx, value, &short).await,
            Err(err)  => self.run_error_hooks(ctx, err, &short).await,
        }
    }
}

impl Default for ActionHookMiddleware { fn default() -> Self { Self::new() } }
