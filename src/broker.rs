//! ServiceBroker — the central orchestrator.
//!
//! The broker:
//!  - manages services (add / start / stop)
//!  - routes action calls to the right handler
//!  - dispatches events (balanced & broadcast)
//!  - runs middlewares
//!  - exposes metrics and tracing

use crate::config::BrokerConfig;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use crate::metrics::MetricsRegistry;
use crate::middleware::Middleware;
use crate::registry::ServiceRegistry;
use crate::service::ServiceSchema;
use crate::tracing::{Span, SpanStore};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// The ServiceBroker.
pub struct ServiceBroker {
    pub config: Arc<BrokerConfig>,
    pub node_id: String,
    pub registry: Arc<ServiceRegistry>,
    pub metrics: Arc<MetricsRegistry>,
    pub spans: Arc<SpanStore>,
    middlewares: Arc<RwLock<Vec<Arc<dyn Middleware>>>>,
    running: Arc<RwLock<bool>>,
}

impl ServiceBroker {
    /// Create a new broker from config.
    pub fn new(config: BrokerConfig) -> Arc<Self> {
        let node_id = config.node_id.clone().unwrap_or_else(|| {
            format!("{}-{}", hostname(), std::process::id())
        });

        let registry = Arc::new(ServiceRegistry::new(
            node_id.clone(),
            config.strategy.clone(),
        ));

        Arc::new(Self {
            node_id: node_id.clone(),
            registry,
            metrics: Arc::new(MetricsRegistry::new()),
            spans: Arc::new(SpanStore::new()),
            middlewares: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            config: Arc::new(config),
        })
    }

    /// Register a service schema with this broker.
    pub async fn add_service(self: &Arc<Self>, schema: ServiceSchema) {
        log::info!(
            "[{}] Registering service: {}",
            self.node_id,
            schema.full_name()
        );
        self.registry.register_local_service(&schema);
    }

    /// Add a middleware.
    pub async fn add_middleware(self: &Arc<Self>, mw: Arc<dyn Middleware>) {
        log::debug!("[{}] Adding middleware: {}", self.node_id, mw.name());
        self.middlewares.write().await.push(mw);
    }

    /// Start the broker — calls `started` hooks on all services.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        log::info!(
            "[{}] Starting broker (namespace: \"{}\")",
            self.node_id,
            self.config.namespace
        );

        *self.running.write().await = true;

        // Fire started hooks
        for entry in self.registry.services.iter() {
            let schema = entry.value().clone();
            let hook = schema.on_started.clone();
            if let Err(e) = hook().await {
                log::error!("Error in started hook for {}: {}", schema.name, e);
            }
        }

        log::info!("[{}] Broker started. Services: {:?}", self.node_id, self.registry.service_names());
        Ok(())
    }

    /// Stop the broker gracefully.
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        log::info!("[{}] Stopping broker...", self.node_id);

        for entry in self.registry.services.iter() {
            let schema = entry.value().clone();
            let hook = schema.on_stopped.clone();
            if let Err(e) = hook().await {
                log::error!("Error in stopped hook for {}: {}", schema.name, e);
            }
        }

        *self.running.write().await = false;
        log::info!("[{}] Broker stopped.", self.node_id);
        Ok(())
    }

    /// Call an action by name, e.g. `"math.add"`.
    pub async fn call(
        self: &Arc<Self>,
        action: impl Into<String>,
        params: Value,
    ) -> Result<Value> {
        let action = action.into();
        let ctx = Context::new(action.clone(), params);
        self.call_with_context(ctx).await
    }

    /// Call an action with a pre-built context.
    pub async fn call_with_context(self: &Arc<Self>, ctx: Context) -> Result<Value> {
        let action_key = ctx.action.clone().unwrap_or_default();

        self.metrics.increment(
            "moleculer.request.total",
            1.0,
            vec![("action".into(), action_key.clone())],
        );

        let ep = self.registry.resolve_action(&action_key)?;

        // Build a span if tracing is enabled
        let mut span = if self.config.tracing.enabled {
            let s = Span::new(action_key.clone(), ctx.id.clone())
                .tag("nodeId", self.node_id.clone())
                .tag("action", action_key.clone());
            Some(s)
        } else {
            None
        };

        let handler = ep.action.handler.clone();
        let middlewares = self.middlewares.read().await.clone();

        // Build the middleware chain (innermost = actual handler)
        let base: crate::middleware::NextHandler = Arc::new(move |ctx| {
            let handler = handler.clone();
            Box::pin(async move { handler(ctx).await })
        });

        let chain = middlewares.iter().rev().fold(base, |next, mw| {
            let mw = mw.clone();
            let next = next.clone();
            Arc::new(move |ctx: Context| {
                let mw = mw.clone();
                let next = next.clone();
                Box::pin(async move { mw.call_action(ctx, next).await })
                    as std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>>
            })
        });

        let result = chain(ctx).await;

        if let Some(ref mut s) = span {
            if let Err(ref e) = result {
                s.set_error(e.to_string());
            }
            s.finish();
            self.spans.record(s.clone());
        }

        match &result {
            Ok(_) => self.metrics.increment(
                "moleculer.request.success",
                1.0,
                vec![("action".into(), action_key)],
            ),
            Err(_) => self.metrics.increment(
                "moleculer.request.error",
                1.0,
                vec![("action".into(), action_key)],
            ),
        }

        result
    }

    /// Emit a balanced event (one instance per service receives it).
    pub async fn emit(self: &Arc<Self>, event: impl Into<String>, payload: Value) -> Result<()> {
        let event = event.into();
        let ctx = Context::for_event(event.clone(), payload);

        let listeners = self.registry.resolve_event_listeners(&event);
        if listeners.is_empty() {
            return Ok(());
        }

        // Balanced: group by service name and pick one from each
        let mut by_service: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
        for ep in listeners {
            by_service.entry(ep.service_name.clone()).or_default().push(ep);
        }

        for (_, eps) in by_service {
            if let Some(ep) = eps.first() {
                let handler = ep.handler.clone();
                let ctx_clone = ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handler(ctx_clone).await {
                        log::error!("Error in event handler: {}", e);
                    }
                });
            }
        }

        Ok(())
    }

    /// Broadcast an event — all listeners receive it.
    pub async fn broadcast(
        self: &Arc<Self>,
        event: impl Into<String>,
        payload: Value,
    ) -> Result<()> {
        let event = event.into();
        let ctx = Context::for_event(event.clone(), payload);

        for ep in self.registry.resolve_event_listeners(&event) {
            let handler = ep.handler.clone();
            let ctx_clone = ctx.clone();
            tokio::spawn(async move {
                if let Err(e) = handler(ctx_clone).await {
                    log::error!("Error in broadcast handler: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Ping the broker (returns latency in ms).
    pub async fn ping(self: &Arc<Self>) -> u64 {
        let start = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_nanos(1)).await;
        start.elapsed().as_micros() as u64
    }

    pub fn is_running(self: &Arc<Self>) -> bool {
        self.running.try_read().map(|g| *g).unwrap_or(false)
    }

    /// Wait for a service to become available.
    pub async fn wait_for_services(
        self: &Arc<Self>,
        services: &[&str],
        timeout_ms: u64,
    ) -> Result<()> {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(timeout_ms);

        loop {
            let all_available = services
                .iter()
                .all(|s| self.registry.services.contains_key(*s));

            if all_available {
                return Ok(());
            }

            if std::time::Instant::now() >= deadline {
                return Err(MoleculerError::ServiceNotFound(services.join(", ")));
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "localhost".into())
}
