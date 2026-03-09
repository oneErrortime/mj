//! Distributed lock — in-process and Redis-backed implementations.
//!
//! Mirrors `moleculer/src/lock.js`.
//!
//! In-process: tokio Mutex per key.
//! Redis: `SET key NX EX` (lightweight Redlock).

use crate::error::{MoleculerError, Result};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

// ─── Trait ───────────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait Lock: Send + Sync {
    /// Acquire the named lock. Returns a guard that releases on drop.
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<Box<dyn LockGuard>>;
    /// Try to acquire; returns None if already locked.
    async fn try_acquire(&self, key: &str, ttl: Duration) -> Result<Option<Box<dyn LockGuard>>>;
}

pub trait LockGuard: Send + Sync {
    fn key(&self) -> &str;
    fn token(&self) -> &str;
}

// ─── Local (in-process) lock ─────────────────────────────────────────────────

pub struct LocalLockGuard {
    key: String,
    token: String,
    _guard: OwnedMutexGuard<()>,
}

impl LockGuard for LocalLockGuard {
    fn key(&self) -> &str { &self.key }
    fn token(&self) -> &str { &self.token }
}

pub struct LocalLock {
    mutexes: DashMap<String, Arc<Mutex<()>>>,
}

impl LocalLock {
    pub fn new() -> Self { Self { mutexes: DashMap::new() } }

    fn get_or_create(&self, key: &str) -> Arc<Mutex<()>> {
        self.mutexes.entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }
}

#[async_trait::async_trait]
impl Lock for LocalLock {
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<Box<dyn LockGuard>> {
        let mutex = self.get_or_create(key);
        let guard = mutex.lock_owned().await;
        let token = Uuid::new_v4().to_string();
        let key_owned = key.to_string();
        // Auto-release after TTL
        let mutex2 = Arc::clone(&mutex);
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            drop(mutex2.lock_owned().await);
        });
        Ok(Box::new(LocalLockGuard { key: key_owned, token, _guard: guard }))
    }

    async fn try_acquire(&self, key: &str, ttl: Duration) -> Result<Option<Box<dyn LockGuard>>> {
        let mutex = self.get_or_create(key);
        match mutex.clone().try_lock_owned() {
            Ok(guard) => {
                let token = Uuid::new_v4().to_string();
                Ok(Some(Box::new(LocalLockGuard { key: key.to_string(), token, _guard: guard })))
            }
            Err(_) => Ok(None),
        }
    }
}

// ─── Redis lock ───────────────────────────────────────────────────────────────

pub struct RedisLockGuard {
    key: String,
    token: String,
    url: String,
}

impl LockGuard for RedisLockGuard {
    fn key(&self) -> &str { &self.key }
    fn token(&self) -> &str { &self.token }
}

impl Drop for RedisLockGuard {
    fn drop(&mut self) {
        // Fire-and-forget release — in production use async Drop wrapper
        let key = format!("MOL-LOCK:{}", self.key);
        let token = self.token.clone();
        let url = self.url.clone();
        tokio::spawn(async move {
            #[cfg(feature = "redis")]
            {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut conn) = client.get_async_connection().await {
                        // Lua script: only delete if token matches
                        let _: redis::RedisResult<()> = redis::Script::new(
                            r#"if redis.call('get', KEYS[1]) == ARGV[1] then
                                 return redis.call('del', KEYS[1])
                               else return 0 end"#
                        ).key(&key).arg(&token).invoke_async(&mut conn).await;
                    }
                }
            }
        });
    }
}

pub struct RedisLock {
    url: String,
    prefix: String,
    retry_count: u32,
    retry_delay_ms: u64,
}

impl RedisLock {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            prefix: "MOL-LOCK".into(),
            retry_count: 3,
            retry_delay_ms: 100,
        }
    }

    fn key(&self, name: &str) -> String {
        format!("{}:{}", self.prefix, name)
    }
}

#[async_trait::async_trait]
impl Lock for RedisLock {
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<Box<dyn LockGuard>> {
        for attempt in 0..=self.retry_count {
            if let Some(guard) = self.try_acquire(key, ttl).await? {
                return Ok(guard);
            }
            if attempt < self.retry_count {
                tokio::time::sleep(Duration::from_millis(self.retry_delay_ms)).await;
            }
        }
        Err(MoleculerError::Internal(format!("Could not acquire lock '{}' after {} retries", key, self.retry_count)))
    }

    async fn try_acquire(&self, key: &str, ttl: Duration) -> Result<Option<Box<dyn LockGuard>>> {
        #[cfg(feature = "redis")]
        {
            use redis::AsyncCommands;
            let client = redis::Client::open(self.url.as_str())
                .map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let mut conn = client.get_async_connection().await
                .map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let token = Uuid::new_v4().to_string();
            let full_key = self.key(key);
            let result: Option<String> = conn.set_options(
                &full_key,
                &token,
                redis::SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(redis::SetExpiry::EX(ttl.as_secs() as usize)),
            ).await.map_err(|e| MoleculerError::Internal(e.to_string()))?;

            if result.is_some() {
                Ok(Some(Box::new(RedisLockGuard {
                    key: key.to_string(),
                    token,
                    url: self.url.clone(),
                })))
            } else {
                Ok(None)
            }
        }
        #[cfg(not(feature = "redis"))]
        {
            Err(MoleculerError::Internal("redis feature not enabled for RedisLock".into()))
        }
    }
}
