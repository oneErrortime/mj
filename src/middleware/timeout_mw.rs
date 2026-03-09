//! Timeout middleware.
use super::{Middleware, NextHandler};
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use async_trait::async_trait;

pub struct TimeoutMiddleware { pub default_timeout_ms: u64 }
impl TimeoutMiddleware { pub fn new(ms: u64) -> Self { Self { default_timeout_ms: ms } } }

#[async_trait]
impl Middleware for TimeoutMiddleware {
    fn name(&self) -> &str { "Timeout" }
    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let timeout_ms = if ctx.timeout > 0 { ctx.timeout } else { self.default_timeout_ms };
        if timeout_ms == 0 { return next(ctx).await; }
        tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            next(ctx),
        ).await.map_err(|_| MoleculerError::RequestTimeout(timeout_ms))?
    }
}
