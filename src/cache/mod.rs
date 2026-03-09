//! Cacher — in-memory LRU cache with optional TTL.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct CacheEntry {
    pub value: Value,
    pub expires_at: Option<DateTime<Utc>>,
}
impl CacheEntry {
    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| Utc::now() >= exp)
    }
}

pub struct MemoryLruCacher {
    inner: Mutex<LruCache<String, CacheEntry>>,
    default_ttl: Option<u64>,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
}

impl MemoryLruCacher {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1000).unwrap())
            )),
            default_ttl: None,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self { self.default_ttl = Some(ttl_secs); self }

    pub fn make_key(action: &str, params: &Value, keys: &[String]) -> String {
        let parts: Vec<String> = keys.iter()
            .map(|k| format!("{}:{}", k, params.get(k).map(|v| v.to_string()).unwrap_or_default()))
            .collect();
        format!("{}|{}", action, parts.join("|"))
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        let mut cache = self.inner.lock().unwrap();
        match cache.get(key) {
            Some(e) if !e.is_expired() => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(e.value.clone())
            },
            _ => {
                cache.pop(key);
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn set(&self, key: String, value: Value, ttl: Option<u64>) {
        let ttl = ttl.or(self.default_ttl);
        let expires_at = ttl.map(|s| Utc::now() + chrono::Duration::seconds(s as i64));
        self.inner.lock().unwrap().put(key, CacheEntry { value, expires_at });
    }

    pub fn delete(&self, key: &str) { self.inner.lock().unwrap().pop(key); }
    pub fn clear(&self) { self.inner.lock().unwrap().clear(); }
    pub fn len(&self) -> usize { self.inner.lock().unwrap().len() }

    /// Snapshot of cache keys (up to `limit`) for the Lab UI.
    pub fn keys_snapshot(&self, limit: usize) -> Vec<String> {
        let cache = self.inner.lock().unwrap();
        cache.iter()
            .filter(|(_, e)| !e.is_expired())
            .take(limit)
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub fn clean_prefix(&self, prefix: &str) {
        let mut cache = self.inner.lock().unwrap();
        let keys: Vec<_> = cache.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, _)| k.clone()).collect();
        for k in keys { cache.pop(&k); }
    }
}

pub mod redis_cacher;
pub use redis_cacher::{RedisCacher, RedisCacherOptions, Cacher};
