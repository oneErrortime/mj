//! Service Registry Discoverers.
//!
//! Mirrors `moleculer/src/registry/discoverers/`.
//!
//! Discoverers handle node discovery and heartbeat in distributed clusters.
//! - `LocalDiscoverer` — heartbeat over transporter (default)
//! - `RedisDiscoverer` — discovery via Redis key-value store
//! - `Etcd3Discoverer` — discovery via etcd v3

use crate::error::{MoleculerError, Result};
use crate::registry::node::NodeInfo;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// ─── Trait ───────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Discoverer: Send + Sync {
    fn name(&self) -> &str;
    async fn register_local_node(&self, node: &NodeInfo) -> Result<()>;
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>>;
    async fn send_heartbeat(&self, node: &NodeInfo) -> Result<()>;
    async fn discard_node(&self, node_id: &str) -> Result<()>;
    async fn stop(&self) -> Result<()> { Ok(()) }
}

// ─── Local discoverer (transporter-based) ────────────────────────────────────

pub struct LocalDiscoverer {
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    nodes: Arc<dashmap::DashMap<String, NodeInfo>>,
}

impl LocalDiscoverer {
    pub fn new(heartbeat_interval_ms: u64, heartbeat_timeout_ms: u64) -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
            heartbeat_timeout: Duration::from_millis(heartbeat_timeout_ms),
            nodes: Arc::new(dashmap::DashMap::new()),
        }
    }
}

#[async_trait]
impl Discoverer for LocalDiscoverer {
    fn name(&self) -> &str { "LocalDiscoverer" }

    async fn register_local_node(&self, node: &NodeInfo) -> Result<()> {
        self.nodes.insert(node.id.clone(), node.clone());
        log::debug!("[LocalDiscoverer] registered local node: {}", node.id);
        Ok(())
    }

    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        Ok(self.nodes.iter().map(|e| e.value().clone()).collect())
    }

    async fn send_heartbeat(&self, node: &NodeInfo) -> Result<()> {
        // In a real cluster, this broadcasts HEARTBEAT packet over transporter.
        if let Some(mut entry) = self.nodes.get_mut(&node.id) {
            entry.last_heartbeat_time = chrono::Utc::now();
        }
        Ok(())
    }

    async fn discard_node(&self, node_id: &str) -> Result<()> {
        self.nodes.remove(node_id);
        log::info!("[LocalDiscoverer] discarded node: {}", node_id);
        Ok(())
    }
}

// ─── Redis discoverer ────────────────────────────────────────────────────────

/// Uses Redis as a coordination backend for node discovery.
///
/// Mirrors `moleculer/src/registry/discoverers/redis.js`.
///
/// Keys layout:
/// - `MOL-DISC:{namespace}:INFO:{nodeId}`   — node info (JSON, TTL = heartbeatTimeout)
/// - `MOL-DISC:{namespace}:HB:{nodeId}`    — heartbeat timestamp
pub struct RedisDiscoverer {
    pub url: String,
    pub namespace: String,
    pub ttl_secs: u64,
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    #[cfg(feature = "redis")]
    client: tokio::sync::RwLock<Option<redis::aio::ConnectionManager>>,
    #[cfg(not(feature = "redis"))]
    _ph: std::marker::PhantomData<()>,
}

impl RedisDiscoverer {
    pub fn new(url: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            namespace: namespace.into(),
            ttl_secs: 30,
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            #[cfg(feature = "redis")]
            client: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "redis"))]
            _ph: std::marker::PhantomData,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "redis")]
        {
            let c = redis::Client::open(self.url.as_str())
                .map_err(|e| MoleculerError::Transport(e.to_string()))?;
            let mgr = redis::aio::ConnectionManager::new(c).await
                .map_err(|e| MoleculerError::Transport(e.to_string()))?;
            *self.client.write().await = Some(mgr);
            log::info!("[RedisDiscoverer] connected to {}", self.url);
        }
        Ok(())
    }

    fn info_key(&self, node_id: &str) -> String {
        format!("MOL-DISC:{}:INFO:{}", self.namespace, node_id)
    }

    fn hb_key(&self, node_id: &str) -> String {
        format!("MOL-DISC:{}:HB:{}", self.namespace, node_id)
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl Discoverer for RedisDiscoverer {
    fn name(&self) -> &str { "RedisDiscoverer" }

    async fn register_local_node(&self, node: &NodeInfo) -> Result<()> {
        use redis::AsyncCommands;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Transport("Redis not connected".into()))?;
        let info = serde_json::to_string(node)?;
        conn.set_ex::<_, _, ()>(self.info_key(&node.id), &info, self.ttl_secs as usize).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        log::debug!("[RedisDiscoverer] registered node: {}", node.id);
        Ok(())
    }

    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        use redis::AsyncCommands;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Transport("Redis not connected".into()))?;
        let pattern = format!("MOL-DISC:{}:INFO:*", self.namespace);
        let keys: Vec<String> = conn.keys(&pattern).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        let mut nodes = Vec::new();
        for key in keys {
            let raw: Option<String> = conn.get(&key).await
                .map_err(|e| MoleculerError::Transport(e.to_string()))?;
            if let Some(s) = raw {
                if let Ok(node) = serde_json::from_str::<NodeInfo>(&s) {
                    nodes.push(node);
                }
            }
        }
        Ok(nodes)
    }

    async fn send_heartbeat(&self, node: &NodeInfo) -> Result<()> {
        use redis::AsyncCommands;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Transport("Redis not connected".into()))?;
        let ts = chrono::Utc::now().timestamp_millis().to_string();
        conn.set_ex::<_, _, ()>(self.hb_key(&node.id), &ts, self.ttl_secs as usize).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        // Refresh info TTL too
        let _: redis::RedisResult<()> = conn.expire(self.info_key(&node.id), self.ttl_secs as usize).await;
        Ok(())
    }

    async fn discard_node(&self, node_id: &str) -> Result<()> {
        use redis::AsyncCommands;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Transport("Redis not connected".into()))?;
        let _: () = conn.del(vec![self.info_key(node_id), self.hb_key(node_id)]).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        Ok(())
    }
}

#[cfg(not(feature = "redis"))]
#[async_trait]
impl Discoverer for RedisDiscoverer {
    fn name(&self) -> &str { "RedisDiscoverer(disabled)" }
    async fn register_local_node(&self, _: &NodeInfo) -> Result<()> {
        Err(MoleculerError::Transport("redis feature not enabled".into()))
    }
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        Err(MoleculerError::Transport("redis feature not enabled".into()))
    }
    async fn send_heartbeat(&self, _: &NodeInfo) -> Result<()> {
        Err(MoleculerError::Transport("redis feature not enabled".into()))
    }
    async fn discard_node(&self, _: &str) -> Result<()> {
        Err(MoleculerError::Transport("redis feature not enabled".into()))
    }
}

// ─── etcd3 discoverer ────────────────────────────────────────────────────────

/// Uses etcd v3 for node discovery via leased keys.
///
/// Mirrors `moleculer/src/registry/discoverers/etcd3.js`.
///
/// Keys layout:
/// - `/{prefix}/{namespace}/nodes/{nodeId}` — node info (JSON, lease TTL)
pub struct Etcd3Discoverer {
    pub endpoints: Vec<String>,
    pub namespace: String,
    pub prefix: String,
    pub ttl_secs: u64,
    /// In-memory cache of known nodes.
    local_cache: Arc<dashmap::DashMap<String, NodeInfo>>,
}

impl Etcd3Discoverer {
    pub fn new(endpoints: Vec<String>, namespace: impl Into<String>) -> Self {
        Self {
            endpoints,
            namespace: namespace.into(),
            prefix: "/MOL".into(),
            ttl_secs: 30,
            local_cache: Arc::new(dashmap::DashMap::new()),
        }
    }

    fn node_key(&self, node_id: &str) -> String {
        format!("{}/{}/nodes/{}", self.prefix, self.namespace, node_id)
    }
}

/// NOTE: Full etcd3 support requires the `etcd-client` crate.
/// This provides the API contract and in-memory fallback.
/// To enable real etcd3: add `etcd-client = "0.13"` to Cargo.toml and
/// implement using `etcd_client::Client`.
#[async_trait]
impl Discoverer for Etcd3Discoverer {
    fn name(&self) -> &str { "Etcd3Discoverer" }

    async fn register_local_node(&self, node: &NodeInfo) -> Result<()> {
        self.local_cache.insert(node.id.clone(), node.clone());
        log::info!("[Etcd3Discoverer] registered node {} (in-memory fallback, no etcd-client crate)",
            node.id);
        Ok(())
    }

    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        Ok(self.local_cache.iter().map(|e| e.value().clone()).collect())
    }

    async fn send_heartbeat(&self, node: &NodeInfo) -> Result<()> {
        if let Some(mut n) = self.local_cache.get_mut(&node.id) {
            n.last_heartbeat_time = chrono::Utc::now();
        }
        Ok(())
    }

    async fn discard_node(&self, node_id: &str) -> Result<()> {
        self.local_cache.remove(node_id);
        Ok(())
    }
}
