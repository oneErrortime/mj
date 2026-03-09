//! Tracing middleware — opens/closes a span around each action call.
use super::{Middleware, NextHandler};
use crate::context::Context;
use crate::error::Result;
use crate::tracing::{Span, SpanStore};
use serde_json::Value;
use std::sync::Arc;
use async_trait::async_trait;

pub struct TracingMiddleware { pub store: Arc<SpanStore> }
impl TracingMiddleware { pub fn new(store: Arc<SpanStore>) -> Self { Self { store } } }

#[async_trait]
impl Middleware for TracingMiddleware {
    fn name(&self) -> &str { "Tracing" }
    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        let trace_id = ctx.trace_id.clone().unwrap_or_else(|| ctx.id.clone());
        let action = ctx.action.clone().unwrap_or_default();
        let mut span = Span::new(action.clone(), trace_id)
            .tag("action", action)
            .tag("node_id", ctx.node_id.clone().unwrap_or_default())
            .tag("call_level", ctx.level.to_string());
        if let Some(pid) = &ctx.parent_span_id { span.parent_id = Some(pid.clone()); }
        span.start_time = chrono::Utc::now();

        let result = next(ctx).await;

        if let Err(ref e) = result { span.set_error(e.to_string()); }
        span.finish();
        self.store.record(span);

        result
    }
}
