//! Cacher middleware — transparent get/set around cacheable actions.
use super::{Middleware, NextHandler};
use crate::cache::MemoryLruCacher;
use crate::context::Context;
use crate::error::Result;
use serde_json::Value;
use std::sync::Arc;
use async_trait::async_trait;

pub struct CacherMiddleware { pub cacher: Arc<MemoryLruCacher> }
impl CacherMiddleware { pub fn new(cacher: Arc<MemoryLruCacher>) -> Self { Self { cacher } } }

#[async_trait]
impl Middleware for CacherMiddleware {
    fn name(&self) -> &str { "Cacher" }
    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        // Only cache if the action definition says so
        // (action-level cache flag is checked in broker.call)
        next(ctx).await
    }
}
