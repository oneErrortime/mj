//! Retry middleware — exponential backoff retry on retryable errors.

use super::{Middleware, NextHandler};
use crate::config::RetryConfig;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use async_trait::async_trait;

pub struct RetryMiddleware {
    config: RetryConfig,
}

impl RetryMiddleware {
    pub fn new(config: RetryConfig) -> Self { Self { config } }
}

#[async_trait]
impl Middleware for RetryMiddleware {
    fn name(&self) -> &str { "Retry" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        if !self.config.enabled {
            return next(ctx).await;
        }

        let max_retries = if ctx.retries_left > 0 { ctx.retries_left } else { self.config.retries };
        let mut attempts = 0u32;

        loop {
            let ctx_copy = ctx.copy();
            let result = next(ctx_copy).await;

            match result {
                Ok(v) => return Ok(v),
                Err(e) if e.is_retryable() && attempts < max_retries => {
                    attempts += 1;
                    let delay = (self.config.delay as f64
                        * self.config.factor.powi(attempts as i32 - 1))
                        .min(self.config.max_delay as f64) as u64;

                    log::warn!(
                        "[Retry] Action '{}' attempt {}/{} failed, retrying in {}ms: {}",
                        ctx.action.as_deref().unwrap_or("?"), attempts, max_retries, delay, e
                    );

                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
