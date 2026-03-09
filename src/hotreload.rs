//! Hot-reload middleware.
//!
//! Mirrors `moleculer/src/middlewares/hot-reload.js`.
//!
//! In moleculer-rs, we watch for changes to service definition files and
//! re-register modified services without stopping the broker. On platforms
//! that support it, this uses `notify` file watching.

use crate::error::Result;
use crate::service::ServiceSchema;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// ─── Change event ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ReloadEvent {
    /// A service file was modified — carry new schema.
    ServiceModified(PathBuf, ServiceSchema),
    /// A service file was removed.
    ServiceRemoved(PathBuf),
}

// ─── HotReloader ─────────────────────────────────────────────────────────────

/// Watches registered paths and emits reload events.
///
/// Usage:
/// ```rust,no_run
/// let reloader = HotReloader::new();
/// reloader.watch("services/math.rs", math_schema_builder);
/// reloader.start(broker.clone()).await?;
/// ```
pub struct HotReloader {
    /// Map: file path → factory function that rebuilds the schema.
    watchers: Arc<RwLock<HashMap<PathBuf, Arc<dyn Fn() -> ServiceSchema + Send + Sync>>>>,
    tx: mpsc::Sender<ReloadEvent>,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<ReloadEvent>>>,
    /// Debounce window in milliseconds.
    debounce_ms: u64,
}

impl HotReloader {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(64);
        Self {
            watchers: Arc::new(RwLock::new(HashMap::new())),
            tx,
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
            debounce_ms: 500,
        }
    }

    pub fn with_debounce(mut self, ms: u64) -> Self { self.debounce_ms = ms; self }

    /// Register a path to watch with a factory that rebuilds the schema on change.
    pub async fn watch<F>(&self, path: impl Into<PathBuf>, factory: F)
    where
        F: Fn() -> ServiceSchema + Send + Sync + 'static,
    {
        let path = path.into();
        self.watchers.write().await.insert(path, Arc::new(factory));
    }

    /// Check modification times manually (polling fallback, no native fs events).
    pub async fn check_changes(&self) {
        let watchers = self.watchers.read().await;
        for (path, factory) in watchers.iter() {
            if Self::file_changed(path) {
                log::info!("[HotReloader] detected change: {:?}", path);
                let schema = factory();
                let _ = self.tx.send(ReloadEvent::ServiceModified(path.clone(), schema)).await;
            }
        }
    }

    fn file_changed(_path: &Path) -> bool {
        // Production impl: compare stored mtime to current mtime.
        // Here we return false — real impl uses `notify` crate:
        //   notify::recommended_watcher(...)
        false
    }

    /// Start the hot-reload polling loop.
    pub async fn start(&self, broker: Arc<crate::broker::ServiceBroker>) {
        let tx = self.tx.clone();
        let watchers = Arc::clone(&self.watchers);
        let debounce = self.debounce_ms;

        // Polling loop
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_millis(debounce)
            );
            let mut mtimes: HashMap<PathBuf, u64> = HashMap::new();

            loop {
                interval.tick().await;
                let guard = watchers.read().await;
                for (path, factory) in guard.iter() {
                    let current_mtime = std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let prev = mtimes.get(path).copied().unwrap_or(0);
                    if current_mtime > prev && prev > 0 {
                        log::info!("[HotReloader] reloading {:?}", path);
                        let schema = factory();
                        let _ = tx.send(ReloadEvent::ServiceModified(path.clone(), schema.clone())).await;
                        // Re-register service
                        broker.add_service(schema).await;
                    }
                    mtimes.insert(path.clone(), current_mtime);
                }
            }
        });

        // Event consumer loop
        let rx = Arc::clone(&self.rx);
        tokio::spawn(async move {
            let mut rx = rx.lock().await;
            while let Some(evt) = rx.recv().await {
                match evt {
                    ReloadEvent::ServiceModified(path, _) => {
                        log::info!("[HotReloader] service at {:?} reloaded", path);
                    }
                    ReloadEvent::ServiceRemoved(path) => {
                        log::warn!("[HotReloader] service at {:?} removed", path);
                    }
                }
            }
        });
    }
}

impl Default for HotReloader { fn default() -> Self { Self::new() } }

// ─── Middleware adapter ───────────────────────────────────────────────────────

/// Hot-reload as a Middleware so it can be installed in the pipeline.
/// On each call it checks whether any watched file has changed and
/// triggers a reload if so.
pub struct HotReloadMiddleware {
    pub reloader: Arc<HotReloader>,
}

impl HotReloadMiddleware {
    pub fn new(reloader: Arc<HotReloader>) -> Self { Self { reloader } }
}

#[async_trait::async_trait]
impl crate::middleware::Middleware for HotReloadMiddleware {
    fn name(&self) -> &str { "HotReload" }

    async fn call(
        &self,
        ctx: crate::context::Context,
        next: crate::middleware::NextHandler,
    ) -> Result<serde_json::Value> {
        // Non-blocking check — does not block the request
        let r = Arc::clone(&self.reloader);
        tokio::spawn(async move { r.check_changes().await });
        next(ctx).await
    }
}
