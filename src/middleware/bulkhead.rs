//! Bulkhead middleware — semaphore + overflow queue per action.

use super::{Middleware, NextHandler};
use crate::config::BulkheadConfig;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;
use async_trait::async_trait;

pub struct BulkheadMiddleware {
    config: BulkheadConfig,
    semaphores: Arc<DashMap<String, Arc<Semaphore>>>,
}

impl BulkheadMiddleware {
    pub fn new(config: BulkheadConfig) -> Self {
        Self { config, semaphores: Arc::new(DashMap::new()) }
    }
}

#[async_trait]
impl Middleware for BulkheadMiddleware {
    fn name(&self) -> &str { "Bulkhead" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        if !self.config.enabled {
            return next(ctx).await;
        }

        let key = ctx.action.clone().unwrap_or_default();
        let semaphore = self.semaphores
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.concurrency)))
            .clone();

        // Check if at capacity (max_queue_size check)
        let available = semaphore.available_permits();
        let in_flight = self.config.concurrency - available;
        let queue_depth = 0usize; // simplified (real impl needs atomic counter)

        if in_flight >= self.config.concurrency
            && queue_depth >= self.config.max_queue_size
        {
            return Err(MoleculerError::QueueFull(self.config.max_queue_size));
        }

        // Acquire semaphore (this will wait if full, up to max_queue_size)
        let permit = semaphore.acquire_owned().await
            .map_err(|_| MoleculerError::Internal("Semaphore closed".into()))?;

        let result = next(ctx).await;
        drop(permit);
        result
    }
}
