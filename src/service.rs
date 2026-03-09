//! Service schema — defines actions, events, channels, hooks, and mixins.

use crate::context::Context;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub type ActionHandler = Arc<dyn Fn(Context) -> BoxFuture<Result<Value>> + Send + Sync>;
pub type EventHandler  = Arc<dyn Fn(Context) -> BoxFuture<Result<()>> + Send + Sync>;
pub type LifecycleHook = Arc<dyn Fn() -> BoxFuture<Result<()>> + Send + Sync>;

fn noop_hook() -> LifecycleHook {
    Arc::new(|| Box::pin(async { Ok(()) }))
}

// ─────────────────────────────────────────────────────────────────────────────
// ActionDef
// ─────────────────────────────────────────────────────────────────────────────

/// Definition of a single action.
#[derive(Clone)]
pub struct ActionDef {
    pub name: String,
    pub handler: ActionHandler,
    /// JSON Schema for parameter validation.
    pub params: Option<Value>,
    /// Cache key template — None = no caching.
    pub cache: Option<CacheOptions>,
    /// Action-level timeout override in ms (0 = broker default).
    pub timeout: u64,
    /// Action-level retry override.
    pub retries: u32,
    /// Action-level circuit breaker override.
    pub circuit_breaker: Option<ActionCircuitBreaker>,
    /// Action-level bulkhead override.
    pub bulkhead: Option<ActionBulkhead>,
    /// Visibility: "published" | "public" | "protected" | "private"
    pub visibility: Visibility,
    /// Before / after / error hooks.
    pub hooks: ActionHooks,
    /// Tracing options.
    pub tracing: Option<ActionTracingOptions>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheOptions {
    pub keys: Vec<String>,
    pub ttl: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ActionCircuitBreaker {
    pub enabled: bool,
    pub threshold: f64,
    pub half_open_time: u64,
}

#[derive(Debug, Clone)]
pub struct ActionBulkhead {
    pub enabled: bool,
    pub concurrency: usize,
    pub max_queue_size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Published,
    Public,
    Protected,
    Private,
}

#[derive(Clone, Default)]
pub struct ActionHooks {
    pub before: Option<ActionHandler>,
    pub after:  Option<Arc<dyn Fn(Context, Value) -> BoxFuture<Result<Value>> + Send + Sync>>,
    pub error:  Option<Arc<dyn Fn(Context, String) -> BoxFuture<Result<Value>> + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub struct ActionTracingOptions {
    pub enabled: bool,
    pub tags: Option<Value>,
}

impl ActionDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let handler: ActionHandler = Arc::new(move |ctx| Box::pin(handler(ctx)));
        Self {
            name: name.into(), handler,
            params: None, cache: None,
            timeout: 0, retries: 0,
            circuit_breaker: None, bulkhead: None,
            visibility: Visibility::Published,
            hooks: ActionHooks::default(),
            tracing: None,
        }
    }

    pub fn params(mut self, schema: Value) -> Self { self.params = Some(schema); self }
    pub fn cache(mut self, keys: Vec<&str>) -> Self {
        self.cache = Some(CacheOptions { keys: keys.into_iter().map(|s| s.to_string()).collect(), ttl: None });
        self
    }
    pub fn timeout(mut self, ms: u64) -> Self { self.timeout = ms; self }
    pub fn retries(mut self, n: u32) -> Self { self.retries = n; self }
    pub fn visibility(mut self, v: Visibility) -> Self { self.visibility = v; self }
}

impl std::fmt::Debug for ActionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionDef")
            .field("name", &self.name)
            .field("visibility", &self.visibility)
            .field("cache", &self.cache.is_some())
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EventDef
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct EventDef {
    pub name: String,
    pub handler: EventHandler,
    /// Balanced group name.
    pub group: Option<String>,
    pub tracing: Option<ActionTracingOptions>,
}

impl EventDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        Self {
            name: name.into(),
            handler: Arc::new(move |ctx| Box::pin(handler(ctx))),
            group: None, tracing: None,
        }
    }
    pub fn group(mut self, g: impl Into<String>) -> Self { self.group = Some(g.into()); self }
}

// ─────────────────────────────────────────────────────────────────────────────
// ServiceSchema + Mixins
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ServiceSchema {
    pub name: String,
    pub version: Option<String>,
    pub settings: Value,
    pub metadata: Value,
    pub actions: HashMap<String, ActionDef>,
    pub events: HashMap<String, EventDef>,
    pub mixins: Vec<ServiceSchema>,
    pub on_created: LifecycleHook,
    pub on_started: LifecycleHook,
    pub on_stopped: LifecycleHook,
    pub on_merged:  LifecycleHook,
    /// Dependencies — wait for these services before starting.
    pub dependencies: Vec<String>,
}

impl ServiceSchema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            settings: Value::Object(Default::default()),
            metadata: Value::Object(Default::default()),
            actions: HashMap::new(),
            events: HashMap::new(),
            mixins: Vec::new(),
            on_created: noop_hook(),
            on_started: noop_hook(),
            on_stopped: noop_hook(),
            on_merged:  noop_hook(),
            dependencies: Vec::new(),
        }
    }

    pub fn version(mut self, v: impl Into<String>) -> Self { self.version = Some(v.into()); self }
    pub fn settings(mut self, s: Value) -> Self { self.settings = s; self }
    pub fn metadata(mut self, m: Value) -> Self { self.metadata = m; self }
    pub fn depends_on(mut self, svc: impl Into<String>) -> Self { self.dependencies.push(svc.into()); self }

    pub fn action(mut self, def: ActionDef) -> Self {
        self.actions.insert(def.name.clone(), def); self
    }
    pub fn event(mut self, def: EventDef) -> Self {
        self.events.insert(def.name.clone(), def); self
    }

    /// Add a mixin. The mixin's actions/events are merged into this schema,
    /// own definitions take priority.
    pub fn mixin(mut self, other: ServiceSchema) -> Self {
        self.mixins.push(other); self
    }

    pub fn on_started<F, Fut>(mut self, f: F) -> Self
    where F: Fn() -> Fut + Send + Sync + 'static, Fut: Future<Output = Result<()>> + Send + 'static {
        self.on_started = Arc::new(move || Box::pin(f())); self
    }
    pub fn on_stopped<F, Fut>(mut self, f: F) -> Self
    where F: Fn() -> Fut + Send + Sync + 'static, Fut: Future<Output = Result<()>> + Send + 'static {
        self.on_stopped = Arc::new(move || Box::pin(f())); self
    }

    /// Resolve the final merged schema (own definitions override mixin definitions).
    pub fn merged(&self) -> Self {
        let mut base = self.clone();
        // Walk mixins in order — earlier mixins are weaker
        for mixin in &self.mixins {
            let mixin_resolved = mixin.merged();
            for (k, v) in mixin_resolved.actions {
                base.actions.entry(k).or_insert(v);
            }
            for (k, v) in mixin_resolved.events {
                base.events.entry(k).or_insert(v);
            }
            for dep in mixin_resolved.dependencies {
                if !base.dependencies.contains(&dep) {
                    base.dependencies.push(dep);
                }
            }
        }
        base
    }

    /// Full versioned name — e.g. "v2.posts"
    pub fn full_name(&self) -> String {
        match &self.version {
            Some(v) => format!("v{}.{}", v, self.name),
            None => self.name.clone(),
        }
    }
}
