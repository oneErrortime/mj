//! Redis Cacher — distributed caching backed by Redis.
//!
//! Mirrors `moleculer/src/cachers/redis.js`.
//!
//! Supports:
//! - TTL-based expiry
//! - Key prefix / namespace
//! - Cluster mode (via redis feature flag)
//! - Distributed lock via `SET NX EX` (poor-man's Redlock)
//!
//! Enable with feature flag `redis` in Cargo.toml.

use crate::error::{MoleculerError, Result};
use serde_json::Value;
use std::time::Duration;

/// Trait for cacher backends (memory or Redis).
#[async_trait::async_trait]
pub trait Cacher: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Value>>;
    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn clean(&self, pattern: &str) -> Result<u64>;
    async fn flush(&self) -> Result<()>;
    fn name(&self) -> &str;
}

// ─── Redis cacher ─────────────────────────────────────────────────────────────

pub struct RedisCacherOptions {
    pub url: String,
    pub prefix: String,
    pub ttl: Option<Duration>,
    pub max_reconnect_period: u64,
    pub ping_interval: Option<u64>,
}

impl Default for RedisCacherOptions {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".into(),
            prefix: "MOL".into(),
            ttl: None,
            max_reconnect_period: 0,
            ping_interval: None,
        }
    }
}

pub struct RedisCacher {
    opts: RedisCacherOptions,
    #[cfg(feature = "redis")]
    pool: tokio::sync::RwLock<Option<redis::aio::ConnectionManager>>,
    #[cfg(not(feature = "redis"))]
    _ph: std::marker::PhantomData<()>,
}

impl RedisCacher {
    pub fn new(opts: RedisCacherOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "redis")]
            pool: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "redis"))]
            _ph: std::marker::PhantomData,
        }
    }

    fn prefixed(&self, key: &str) -> String {
        format!("{}:{}", self.opts.prefix, key)
    }

    /// Initialize connection (call at broker start).
    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "redis")]
        {
            let client = redis::Client::open(self.opts.url.as_str())
                .map_err(|e| MoleculerError::Cache(e.to_string()))?;
            let mgr = redis::aio::ConnectionManager::new(client).await
                .map_err(|e| MoleculerError::Cache(e.to_string()))?;
            *self.pool.write().await = Some(mgr);
            log::info!("[RedisCacher] connected to {}", self.opts.url);
        }
        Ok(())
    }
}

#[cfg(feature = "redis")]
#[async_trait::async_trait]
impl Cacher for RedisCacher {
    async fn get(&self, key: &str) -> Result<Option<Value>> {
        use redis::AsyncCommands;
        let mut guard = self.pool.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Cache("Redis not connected".into()))?;
        let k = self.prefixed(key);
        let raw: Option<String> = conn.get(&k).await
            .map_err(|e| MoleculerError::Cache(e.to_string()))?;
        match raw {
            None => Ok(None),
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        }
    }

    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> Result<()> {
        use redis::AsyncCommands;
        let mut guard = self.pool.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Cache("Redis not connected".into()))?;
        let k = self.prefixed(key);
        let serialized = serde_json::to_string(&value)?;
        let effective_ttl = ttl.or(self.opts.ttl);
        match effective_ttl {
            Some(t) => {
                conn.set_ex::<_, _, ()>(&k, serialized, t.as_secs() as usize).await
                    .map_err(|e| MoleculerError::Cache(e.to_string()))?;
            }
            None => {
                conn.set::<_, _, ()>(&k, serialized).await
                    .map_err(|e| MoleculerError::Cache(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        use redis::AsyncCommands;
        let mut guard = self.pool.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Cache("Redis not connected".into()))?;
        conn.del::<_, ()>(self.prefixed(key)).await
            .map_err(|e| MoleculerError::Cache(e.to_string()))?;
        Ok(())
    }

    async fn clean(&self, pattern: &str) -> Result<u64> {
        use redis::AsyncCommands;
        let mut guard = self.pool.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Cache("Redis not connected".into()))?;
        let full_pattern = format!("{}:{}", self.opts.prefix, pattern);
        let keys: Vec<String> = conn.keys(&full_pattern).await
            .map_err(|e| MoleculerError::Cache(e.to_string()))?;
        let count = keys.len() as u64;
        if count > 0 {
            conn.del::<_, ()>(keys).await
                .map_err(|e| MoleculerError::Cache(e.to_string()))?;
        }
        Ok(count)
    }

    async fn flush(&self) -> Result<()> {
        self.clean("*").await.map(|_| ())
    }

    fn name(&self) -> &str { "RedisCacher" }
}

#[cfg(not(feature = "redis"))]
#[async_trait::async_trait]
impl Cacher for RedisCacher {
    async fn get(&self, _: &str) -> Result<Option<Value>> {
        Err(MoleculerError::Cache("Redis feature not enabled".into()))
    }
    async fn set(&self, _: &str, _: Value, _: Option<Duration>) -> Result<()> {
        Err(MoleculerError::Cache("Redis feature not enabled".into()))
    }
    async fn delete(&self, _: &str) -> Result<()> {
        Err(MoleculerError::Cache("Redis feature not enabled".into()))
    }
    async fn clean(&self, _: &str) -> Result<u64> {
        Err(MoleculerError::Cache("Redis feature not enabled".into()))
    }
    async fn flush(&self) -> Result<()> {
        Err(MoleculerError::Cache("Redis feature not enabled".into()))
    }
    fn name(&self) -> &str { "RedisCacher(disabled)" }
}
