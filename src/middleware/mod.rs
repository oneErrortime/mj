//! Middleware pipeline — hooks around every action call and event dispatch.
//!
//! ## Built-in middlewares (mirrors Moleculer JS INTERNAL_MIDDLEWARES)
//!
//! 1. `LoggingMiddleware`   — logs every call + duration
//! 2. `MetricsMiddleware`   — increments counters, records histograms
//! 3. `CircuitBreaker`      — CLOSED → HALF_OPEN → OPEN state machine
//! 4. `RetryMiddleware`     — exponential backoff retry on retryable errors
//! 5. `BulkheadMiddleware`  — semaphore + overflow queue per action
//! 6. `TimeoutMiddleware`   — enforces request timeout
//! 7. `CacherMiddleware`    — get/set cache on cacheable actions
//! 8. `TracingMiddleware`   — records distributed tracing spans

pub mod circuit_breaker;
pub mod retry;
pub mod bulkhead;
pub mod timeout_mw;
pub mod cacher_mw;
pub mod tracing_mw;

pub use circuit_breaker::CircuitBreakerMiddleware;
pub use retry::RetryMiddleware;
pub use bulkhead::BulkheadMiddleware;
pub use timeout_mw::TimeoutMiddleware;
pub use cacher_mw::CacherMiddleware;
pub use tracing_mw::TracingMiddleware;

use crate::context::Context;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type NextHandler = Arc<
    dyn Fn(Context) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
    + Send + Sync,
>;

/// Core middleware trait.
#[async_trait]
pub trait Middleware: Send + Sync {
    fn name(&self) -> &str;

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        next(ctx).await
    }
}

// ─── Logging ──────────────────────────────────────────────────────────────────
pub struct LoggingMiddleware;
#[async_trait]
impl Middleware for LoggingMiddleware {
    fn name(&self) -> &str { "LoggingMiddleware" }
    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        let id = ctx.id.clone();
        let start = std::time::Instant::now();
        let result = next(ctx).await;
        let elapsed = start.elapsed().as_millis();
        match &result {
            Ok(_) => log::debug!("[{}] {:<40} {:>6}ms ✓", id, action, elapsed),
            Err(e) => log::warn!( "[{}] {:<40} {:>6}ms ✗ {}", id, action, elapsed, e),
        }
        result
    }
}

// ─── Metrics ──────────────────────────────────────────────────────────────────
pub struct MetricsMiddleware;
#[async_trait]
impl Middleware for MetricsMiddleware {
    fn name(&self) -> &str { "MetricsMiddleware" }
    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let action = ctx.action.clone().unwrap_or_default();
        let start = std::time::Instant::now();
        let result = next(ctx).await;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        log::trace!("metrics action={} duration_ms={:.3} ok={}", action, elapsed_ms, result.is_ok());
        result
    }
}

pub mod extra;
pub mod transmit;

pub use extra::{ThrottleMiddleware, DebounceMiddleware, FallbackMiddleware, ContextTrackerMiddleware};
pub use transmit::{TransmitCodec, CompressorCodec, EncryptorCodec, CompressionMethod};

pub mod validator_mw;
pub use validator_mw::ValidatorMiddleware;

pub mod action_hook;
pub use action_hook::ActionHookMiddleware;
