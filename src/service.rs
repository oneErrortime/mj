//! Service schema — defines actions and event listeners.

use crate::context::Context;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Boxed async action handler signature.
pub type ActionHandler =
    Arc<dyn Fn(Context) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync>;

/// Boxed async event handler signature.
pub type EventHandler =
    Arc<dyn Fn(Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

/// Definition of a single action.
#[derive(Clone)]
pub struct ActionDef {
    pub name: String,
    pub handler: ActionHandler,
    /// Optional parameter schema (JSON Schema).
    pub params: Option<Value>,
    /// Whether results can be cached.
    pub cache: bool,
    /// Action-level timeout override in ms.
    pub timeout: u64,
    /// Number of retries.
    pub retries: u32,
    /// Visibility: "published" | "public" | "protected" | "private"
    pub visibility: String,
}

impl ActionDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let handler = Arc::new(move |ctx| {
            let fut = handler(ctx);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Result<Value>> + Send>>
        });
        Self {
            name: name.into(),
            handler,
            params: None,
            cache: false,
            timeout: 0,
            retries: 0,
            visibility: "published".to_string(),
        }
    }

    pub fn params(mut self, schema: Value) -> Self { self.params = Some(schema); self }
    pub fn cache(mut self, v: bool) -> Self { self.cache = v; self }
    pub fn timeout(mut self, ms: u64) -> Self { self.timeout = ms; self }
    pub fn retries(mut self, n: u32) -> Self { self.retries = n; self }
    pub fn visibility(mut self, v: impl Into<String>) -> Self { self.visibility = v.into(); self }
}

/// Definition of a single event listener.
#[derive(Clone)]
pub struct EventDef {
    pub name: String,
    pub handler: EventHandler,
    /// Optional group name for balanced events.
    pub group: Option<String>,
}

impl EventDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let handler = Arc::new(move |ctx| {
            let fut = handler(ctx);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Result<()>> + Send>>
        });
        Self { name: name.into(), handler, group: None }
    }

    pub fn group(mut self, g: impl Into<String>) -> Self { self.group = Some(g.into()); self }
}

/// Lifecycle hook type.
pub type LifecycleHook =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

fn noop_hook() -> LifecycleHook {
    Arc::new(|| Box::pin(async { Ok(()) }))
}

/// Service schema — the blueprint for a Moleculer service.
///
/// ```rust
/// use moleculer::{ServiceSchema, ActionDef};
/// use serde_json::json;
///
/// let svc = ServiceSchema::new("math")
///     .version("2")
///     .action(ActionDef::new("add", |ctx| async move {
///         let a = ctx.params["a"].as_f64().unwrap_or(0.0);
///         let b = ctx.params["b"].as_f64().unwrap_or(0.0);
///         Ok(json!({ "result": a + b }))
///     }));
/// ```
#[derive(Clone)]
pub struct ServiceSchema {
    pub name: String,
    pub version: Option<String>,
    pub settings: Value,
    pub metadata: Value,
    pub actions: HashMap<String, ActionDef>,
    pub events: HashMap<String, EventDef>,
    pub on_created: LifecycleHook,
    pub on_started: LifecycleHook,
    pub on_stopped: LifecycleHook,
}

impl ServiceSchema {
    /// Create a new service schema.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            settings: Value::Object(Default::default()),
            metadata: Value::Object(Default::default()),
            actions: HashMap::new(),
            events: HashMap::new(),
            on_created: noop_hook(),
            on_started: noop_hook(),
            on_stopped: noop_hook(),
        }
    }

    pub fn version(mut self, v: impl Into<String>) -> Self { self.version = Some(v.into()); self }
    pub fn settings(mut self, s: Value) -> Self { self.settings = s; self }
    pub fn metadata(mut self, m: Value) -> Self { self.metadata = m; self }

    /// Register an action.
    pub fn action(mut self, def: ActionDef) -> Self {
        self.actions.insert(def.name.clone(), def);
        self
    }

    /// Register an event listener.
    pub fn event(mut self, def: EventDef) -> Self {
        self.events.insert(def.name.clone(), def);
        self
    }

    /// Set a started lifecycle hook.
    pub fn on_started<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.on_started = Arc::new(move || Box::pin(f()));
        self
    }

    /// Set a stopped lifecycle hook.
    pub fn on_stopped<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.on_stopped = Arc::new(move || Box::pin(f()));
        self
    }

    /// Full service name including optional version prefix.
    pub fn full_name(&self) -> String {
        match &self.version {
            Some(v) => format!("v{}.{}", v, self.name),
            None => self.name.clone(),
        }
    }
}

impl std::fmt::Debug for ActionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionDef")
            .field("name", &self.name)
            .field("cache", &self.cache)
            .field("timeout", &self.timeout)
            .field("visibility", &self.visibility)
            .finish()
    }
}
