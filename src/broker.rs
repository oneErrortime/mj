//! ServiceBroker — the central hub.
//!
//! Wires together:
//! - ServiceRegistry (action routing + topology)
//! - Middleware pipeline (CB / Retry / Bulkhead / Timeout / Cacher / Tracing)
//! - Cacher (LRU)
//! - Channels (durable queues via adapter)
//! - Metrics + Tracing stores

use crate::cache::MemoryLruCacher;
use crate::channels::{adapter::Adapter, ChannelDef, ChannelStats, SendOptions};
use crate::config::BrokerConfig;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use crate::metrics::MetricsRegistry;
use crate::middleware::{
    BulkheadMiddleware, CircuitBreakerMiddleware, LoggingMiddleware,
    MetricsMiddleware, RetryMiddleware, TimeoutMiddleware, TracingMiddleware,
    Middleware, NextHandler,
};
use crate::registry::ServiceRegistry;
use crate::service::{CacheOptions, ServiceSchema};
use crate::tracing::SpanStore;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct ServiceBroker {
    pub config: Arc<BrokerConfig>,
    pub node_id: String,
    pub instance_id: String,
    pub registry: Arc<ServiceRegistry>,
    pub metrics: Arc<MetricsRegistry>,
    pub spans: Arc<SpanStore>,
    pub cacher: Option<Arc<MemoryLruCacher>>,
    pub channel_adapter: Option<Arc<dyn Adapter>>,
    pub cb_middleware: Option<Arc<CircuitBreakerMiddleware>>,
    middlewares: Arc<RwLock<Vec<Arc<dyn Middleware>>>>,
    running: Arc<RwLock<bool>>,
}

impl ServiceBroker {
    pub fn new(config: BrokerConfig) -> Arc<Self> {
        let node_id = config.node_id.clone()
            .unwrap_or_else(|| format!("{}-{}", hostname(), std::process::id()));

        let registry = Arc::new(ServiceRegistry::new(
            node_id.clone(),
            config.strategy.clone(),
        ));

        let cacher = if config.cacher.enabled {
            Some(Arc::new(MemoryLruCacher::new(config.cacher.max_size)
                .with_ttl(if config.cacher.ttl > 0 { config.cacher.ttl } else { 0 })))
        } else { None };

        let channel_adapter: Option<Arc<dyn Adapter>> = if config.channels.enabled {
            match config.channels.adapter.as_str() {
                "memory" | _ => Some(Arc::new(crate::channels::InMemoryAdapter::new())),
            }
        } else { None };

        let cb_middleware: Option<Arc<CircuitBreakerMiddleware>> = if config.circuit_breaker.enabled {
            Some(Arc::new(CircuitBreakerMiddleware::new(config.circuit_breaker.clone())))
        } else { None };

        Arc::new(Self {
            node_id: node_id.clone(),
            instance_id: Uuid::new_v4().to_string(),
            registry,
            metrics: Arc::new(MetricsRegistry::new()),
            spans: Arc::new(SpanStore::new()),
            cacher,
            channel_adapter,
            cb_middleware,
            middlewares: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            config: Arc::new(config),
        })
    }

    /// Build the default internal middleware stack (mirrors Moleculer's INTERNAL_MIDDLEWARES).
    pub async fn install_default_middlewares(self: &Arc<Self>) {
        let cfg = &self.config;

        // Order matters: outermost first in the chain
        // Logging → Metrics → Tracing → Timeout → Retry → CB → Bulkhead → (Cacher) → handler

        self.add_middleware(Arc::new(LoggingMiddleware)).await;
        self.add_middleware(Arc::new(MetricsMiddleware)).await;

        if cfg.tracing.enabled {
            self.add_middleware(Arc::new(TracingMiddleware::new(Arc::clone(&self.spans)))).await;
        }

        if cfg.request_timeout > 0 {
            self.add_middleware(Arc::new(TimeoutMiddleware::new(cfg.request_timeout))).await;
        }

        if cfg.retry.enabled {
            self.add_middleware(Arc::new(RetryMiddleware::new(cfg.retry.clone()))).await;
        }

        if cfg.circuit_breaker.enabled {
            if let Some(ref cb) = self.cb_middleware {
                self.add_middleware(Arc::clone(cb)).await;
            }
        }

        if cfg.bulkhead.enabled {
            self.add_middleware(Arc::new(BulkheadMiddleware::new(cfg.bulkhead.clone()))).await;
        }

        if let Some(ref cacher) = self.cacher {
            use crate::middleware::CacherMiddleware;
            self.add_middleware(Arc::new(CacherMiddleware::new(Arc::clone(cacher)))).await;
        }
    }

    pub async fn add_middleware(self: &Arc<Self>, mw: Arc<dyn Middleware>) {
        log::debug!("[{}] Middleware added: {}", self.node_id, mw.name());
        self.middlewares.write().await.push(mw);
    }

    pub async fn add_service(self: &Arc<Self>, schema: ServiceSchema) {
        log::info!("[{}] Registering service '{}'", self.node_id, schema.full_name());
        // Register channel subscriptions
        if let Some(ref adapter) = self.channel_adapter {
            // In a full impl the schema would have channel defs; for now they are registered separately
        }
        self.registry.register_local_service(&schema);
    }

    /// Subscribe a channel consumer. Can be called at any time (before or after start).
    pub async fn subscribe_channel(self: &Arc<Self>, def: ChannelDef) -> Result<()> {
        let adapter = self.channel_adapter.as_ref()
            .ok_or_else(|| MoleculerError::ChannelAdapter("No channel adapter configured".into()))?;
        adapter.subscribe(&def).await?;
        log::info!("[{}] Subscribed to channel '{}'", self.node_id, def.name);
        Ok(())
    }

    /// Publish a message to a channel.
    pub async fn send_to_channel(
        self: &Arc<Self>,
        channel: impl Into<String>,
        payload: Value,
    ) -> Result<()> {
        self.send_to_channel_with_opts(channel, payload, SendOptions::default()).await
    }

    pub async fn send_to_channel_with_opts(
        self: &Arc<Self>,
        channel: impl Into<String>,
        payload: Value,
        opts: SendOptions,
    ) -> Result<()> {
        let channel = channel.into();
        let adapter = self.channel_adapter.as_ref()
            .ok_or_else(|| MoleculerError::ChannelAdapter("No channel adapter configured".into()))?;

        self.metrics.increment("moleculer.channels.messages.sent", 1.0,
            vec![("channel".into(), channel.clone())]);

        adapter.publish(&channel, payload, opts).await
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        log::info!("[{}] Starting broker (namespace='{}')", self.node_id, self.config.namespace);

        // Connect channel adapter
        if let Some(ref adapter) = self.channel_adapter {
            adapter.connect().await?;
        }

        *self.running.write().await = true;

        // Fire started hooks
        for entry in self.registry.services.iter() {
            let hook = entry.value().on_started.clone();
            if let Err(e) = hook().await {
                log::error!("started hook error in '{}': {}", entry.key(), e);
            }
        }

        log::info!("[{}] ✓ Broker started. Services: {:?}", self.node_id, self.registry.service_names());
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        log::info!("[{}] Stopping broker...", self.node_id);

        for entry in self.registry.services.iter() {
            let hook = entry.value().on_stopped.clone();
            if let Err(e) = hook().await { log::error!("stopped hook error: {}", e); }
        }

        if let Some(ref adapter) = self.channel_adapter {
            adapter.disconnect().await?;
        }

        *self.running.write().await = false;
        log::info!("[{}] Broker stopped.", self.node_id);
        Ok(())
    }

    /// Call an action — main entry point.
    pub async fn call(
        self: &Arc<Self>,
        action: impl Into<String>,
        params: Value,
    ) -> Result<Value> {
        let mut ctx = Context::new(action, params);
        ctx.node_id = Some(self.node_id.clone());
        self.call_with_context(ctx).await
    }

    /// Call with a pre-built context (sub-call from within a service).
    pub async fn call_with_context(self: &Arc<Self>, ctx: Context) -> Result<Value> {
        let action_key = ctx.action.clone().unwrap_or_default();

        // Max call level guard
        if ctx.level > self.config.max_call_level {
            return Err(MoleculerError::MaxCallLevelReached(self.config.max_call_level));
        }

        self.metrics.increment("moleculer.request.total", 1.0,
            vec![("action".into(), action_key.clone())]);

        let ep = self.registry.resolve_action(&action_key)?;
        let caller = ctx.caller.clone().unwrap_or_else(|| self.node_id.clone());
        let start_us = std::time::Instant::now();

        // Cacher check
        if let Some(ref cache_opts) = ep.action.cache {
            if let Some(ref cacher) = self.cacher {
                let key = MemoryLruCacher::make_key(&action_key, &ctx.params, &cache_opts.keys);
                if let Some(cached) = cacher.get(&key) {
                    self.metrics.increment("moleculer.cache.hit", 1.0,
                        vec![("action".into(), action_key.clone())]);
                    return Ok(cached);
                }
                self.metrics.increment("moleculer.cache.miss", 1.0,
                    vec![("action".into(), action_key.clone())]);
            }
        }

        // Build middleware chain
        let handler = ep.action.handler.clone();
        let base: NextHandler = Arc::new(move |ctx| {
            let h = handler.clone();
            Box::pin(async move { h(ctx).await })
        });

        let middlewares = self.middlewares.read().await.clone();
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
        let latency_us = start_us.elapsed().as_micros() as u64;

        // Record topology
        self.registry.record_topology_call(&caller, &action_key, latency_us);

        match &result {
            Ok(v) => {
                self.metrics.increment("moleculer.request.success", 1.0,
                    vec![("action".into(), action_key.clone())]);
                self.metrics.observe_histogram("moleculer.request.duration_ms", latency_us as f64 / 1000.0,
                    vec![("action".into(), action_key.clone())]);

                // Cache result
                if let Some(ref cache_opts) = ep.action.cache {
                    if let Some(ref cacher) = self.cacher {
                        let key = MemoryLruCacher::make_key(&action_key, &serde_json::Value::Null, &cache_opts.keys);
                        cacher.set(key, v.clone(), cache_opts.ttl);
                    }
                }
            }
            Err(_) => {
                self.metrics.increment("moleculer.request.error", 1.0,
                    vec![("action".into(), action_key)]);
            }
        }

        result
    }

    /// Emit a balanced event — one consumer per service group receives it.
    pub async fn emit(self: &Arc<Self>, event: impl Into<String>, payload: Value) -> Result<()> {
        let event = event.into();
        let ctx = Context::for_event(event.clone(), payload);
        let listeners = self.registry.resolve_event_listeners(&event);

        // Group by service name → pick first from each group
        let mut by_service: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
        for ep in listeners {
            by_service.entry(ep.service_name.clone()).or_default().push(ep);
        }
        for (_, eps) in by_service {
            if let Some(ep) = eps.first() {
                let handler = ep.handler.clone();
                let ctx_c = ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handler(ctx_c).await { log::error!("[emit] {}", e); }
                });
            }
        }
        Ok(())
    }

    /// Broadcast — all listeners of all groups receive it.
    pub async fn broadcast(self: &Arc<Self>, event: impl Into<String>, payload: Value) -> Result<()> {
        let event = event.into();
        let ctx = Context::for_event(event.clone(), payload);
        for ep in self.registry.resolve_event_listeners(&event) {
            let handler = ep.handler.clone();
            let ctx_c = ctx.clone();
            tokio::spawn(async move {
                if let Err(e) = handler(ctx_c).await { log::error!("[broadcast] {}", e); }
            });
        }
        Ok(())
    }

    pub async fn ping(self: &Arc<Self>) -> u64 {
        let s = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_nanos(1)).await;
        s.elapsed().as_micros() as u64
    }

    pub fn is_running(&self) -> bool {
        self.running.try_read().map(|g| *g).unwrap_or(false)
    }

    pub async fn wait_for_services(
        self: &Arc<Self>,
        services: &[&str],
        timeout_ms: u64,
    ) -> Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if services.iter().all(|s| self.registry.services.contains_key(*s)) { return Ok(()); }
            if std::time::Instant::now() >= deadline {
                return Err(MoleculerError::ServiceNotFound(services.join(", ")));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub fn channel_stats(&self) -> Vec<ChannelStats> {
        self.channel_adapter.as_ref().map_or_else(Vec::new, |a| a.stats())
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
