//! Middleware system — hooks around action calls and event dispatches.

use crate::context::Context;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type NextHandler = Arc<
    dyn Fn(Context) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync,
>;

/// Middleware trait — implement to hook into the request/response pipeline.
#[async_trait]
pub trait Middleware: Send + Sync {
    fn name(&self) -> &str;

    async fn call_action(
        &self,
        ctx: Context,
        next: NextHandler,
    ) -> Result<Value> {
        next(ctx).await
    }
}

/// Logging middleware — logs every action call and its result.
pub struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    fn name(&self) -> &str { "LoggingMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_else(|| "unknown".into());
        let id = ctx.id.clone();
        log::debug!("[{}] Action called: {}", id, action);
        let start = std::time::Instant::now();
        let result = next(ctx).await;
        let elapsed = start.elapsed().as_millis();
        match &result {
            Ok(_) => log::debug!("[{}] Action {} finished in {}ms", id, action, elapsed),
            Err(e) => log::warn!("[{}] Action {} failed in {}ms: {}", id, action, elapsed, e),
        }
        result
    }
}

/// Metrics middleware — increments counters on each call.
pub struct MetricsMiddleware;

#[async_trait]
impl Middleware for MetricsMiddleware {
    fn name(&self) -> &str { "MetricsMiddleware" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        let start = std::time::Instant::now();
        let result = next(ctx).await;
        let elapsed = start.elapsed().as_micros() as f64 / 1000.0;
        log::trace!("metrics: action={} duration_ms={:.3} ok={}", action, elapsed, result.is_ok());
        result
    }
}
