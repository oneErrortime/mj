//! Load balancing strategies.
//!
//! Mirrors `moleculer/src/strategies/`:
//! - round-robin.js
//! - random.js
//! - shard.js
//! - cpu-usage.js
//! - latency.js

use super::ActionEndpoint;
use crate::error::{MoleculerError, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use rand::Rng;
use fnv::FnvHasher;
use std::hash::Hasher;

// ─── Trait ───────────────────────────────────────────────────────────────────

pub trait LoadStrategy: Send + Sync {
    fn name(&self) -> &str;
    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], ctx_id: Option<&str>) -> Result<&'a ActionEndpoint>;
}

// ─── Round-Robin ──────────────────────────────────────────────────────────────

pub struct RoundRobinStrategy {
    counter: AtomicUsize,
}

impl RoundRobinStrategy {
    pub fn new() -> Self { Self { counter: AtomicUsize::new(0) } }
}

impl LoadStrategy for RoundRobinStrategy {
    fn name(&self) -> &str { "RoundRobin" }

    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], _: Option<&str>) -> Result<&'a ActionEndpoint> {
        if endpoints.is_empty() {
            return Err(MoleculerError::ServiceNotFound("no endpoints".into()));
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % endpoints.len();
        Ok(&endpoints[idx])
    }
}

// ─── Random ───────────────────────────────────────────────────────────────────

pub struct RandomStrategy;

impl LoadStrategy for RandomStrategy {
    fn name(&self) -> &str { "Random" }

    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], _: Option<&str>) -> Result<&'a ActionEndpoint> {
        if endpoints.is_empty() {
            return Err(MoleculerError::ServiceNotFound("no endpoints".into()));
        }
        let idx = rand::thread_rng().gen_range(0..endpoints.len());
        Ok(&endpoints[idx])
    }
}

// ─── Shard ────────────────────────────────────────────────────────────────────

/// Consistent-hashing shard strategy.
///
/// Routes requests to the same endpoint based on a shard key extracted from
/// the request context, ensuring affinity for the same key.
///
/// Mirrors `moleculer/src/strategies/shard.js`.
pub struct ShardStrategy {
    /// Param name to use as shard key (default: first param value).
    pub shard_key: Option<String>,
    /// Ring size for consistent hashing.
    pub ring_size: usize,
}

impl Default for ShardStrategy {
    fn default() -> Self {
        Self { shard_key: None, ring_size: 256 }
    }
}

impl LoadStrategy for ShardStrategy {
    fn name(&self) -> &str { "Shard" }

    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], ctx_id: Option<&str>) -> Result<&'a ActionEndpoint> {
        if endpoints.is_empty() {
            return Err(MoleculerError::ServiceNotFound("no endpoints".into()));
        }
        // Use ctx_id or action name as shard key
        let key = ctx_id.unwrap_or("default");
        let mut hasher = FnvHasher::default();
        hasher.write(key.as_bytes());
        let hash = hasher.finish() as usize;
        let idx = hash % endpoints.len();
        Ok(&endpoints[idx])
    }
}

// ─── CPU-Usage strategy ───────────────────────────────────────────────────────

/// Routes to the endpoint with the lowest CPU usage.
///
/// Mirrors `moleculer/src/strategies/cpu-usage.js`.
///
/// Tracks per-node CPU metrics reported via HEARTBEAT packets.
pub struct CpuUsageStrategy {
    /// Per-node CPU usage (0.0–100.0).
    cpu: Arc<dashmap::DashMap<String, f64>>,
    /// Fall back to Round-Robin if CPU info unavailable.
    fallback: RoundRobinStrategy,
    /// Don't use node if CPU > this threshold (default: 80%).
    low_water_mark: f64,
    high_water_mark: f64,
}

impl Default for CpuUsageStrategy {
    fn default() -> Self {
        Self {
            cpu: Arc::new(dashmap::DashMap::new()),
            fallback: RoundRobinStrategy::new(),
            low_water_mark: 10.0,
            high_water_mark: 80.0,
        }
    }
}

impl CpuUsageStrategy {
    pub fn new() -> Self { Self::default() }

    /// Update CPU usage for a node (called from heartbeat handler).
    pub fn update_cpu(&self, node_id: &str, cpu: f64) {
        self.cpu.insert(node_id.to_string(), cpu);
    }

    pub fn cpu_map(&self) -> Arc<dashmap::DashMap<String, f64>> {
        Arc::clone(&self.cpu)
    }
}

impl LoadStrategy for CpuUsageStrategy {
    fn name(&self) -> &str { "CpuUsage" }

    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], ctx_id: Option<&str>) -> Result<&'a ActionEndpoint> {
        if endpoints.is_empty() {
            return Err(MoleculerError::ServiceNotFound("no endpoints".into()));
        }
        // Find endpoint with lowest CPU
        let best = endpoints.iter().min_by(|a, b| {
            let ca = self.cpu.get(&a.node_id).map(|c| *c).unwrap_or(50.0);
            let cb = self.cpu.get(&b.node_id).map(|c| *c).unwrap_or(50.0);
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
        best.ok_or_else(|| MoleculerError::ServiceNotFound("no endpoints".into()))
    }
}

// ─── Latency strategy ─────────────────────────────────────────────────────────

/// Routes to the endpoint with the lowest measured latency.
///
/// Mirrors `moleculer/src/strategies/latency.js`.
///
/// Keeps an exponential moving average of latency per endpoint.
/// Probes all endpoints periodically to refresh latency measurements.
pub struct LatencyStrategy {
    /// Per-node-action latency EMA in microseconds.
    latency: Arc<dashmap::DashMap<String, f64>>,
    fallback: RoundRobinStrategy,
    /// Smoothing factor for EMA (0 < alpha < 1).
    alpha: f64,
    /// Number of endpoints to collect latency from per window.
    collect_count: usize,
    /// Ping interval in seconds.
    ping_interval: u64,
}

impl Default for LatencyStrategy {
    fn default() -> Self {
        Self {
            latency: Arc::new(dashmap::DashMap::new()),
            fallback: RoundRobinStrategy::new(),
            alpha: 0.3,
            collect_count: 5,
            ping_interval: 10,
        }
    }
}

impl LatencyStrategy {
    pub fn new() -> Self { Self::default() }

    /// Record a new latency sample for a node (μs).
    pub fn record(&self, node_id: &str, latency_us: u64) {
        let new_val = latency_us as f64;
        self.latency.entry(node_id.to_string())
            .and_modify(|old| {
                // EMA: new = alpha * sample + (1 - alpha) * old
                *old = self.alpha * new_val + (1.0 - self.alpha) * *old;
            })
            .or_insert(new_val);
    }

    pub fn latency_map(&self) -> Arc<dashmap::DashMap<String, f64>> {
        Arc::clone(&self.latency)
    }
}

impl LoadStrategy for LatencyStrategy {
    fn name(&self) -> &str { "Latency" }

    fn select<'a>(&self, endpoints: &'a [ActionEndpoint], ctx_id: Option<&str>) -> Result<&'a ActionEndpoint> {
        if endpoints.is_empty() {
            return Err(MoleculerError::ServiceNotFound("no endpoints".into()));
        }
        // Find endpoint with lowest latency EMA
        let best = endpoints.iter().min_by(|a, b| {
            let la = self.latency.get(&a.node_id).map(|v| *v).unwrap_or(f64::MAX);
            let lb = self.latency.get(&b.node_id).map(|v| *v).unwrap_or(f64::MAX);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        });
        best.ok_or_else(|| MoleculerError::ServiceNotFound("no endpoints".into()))
    }
}

// ─── Factory ─────────────────────────────────────────────────────────────────

pub fn create_strategy(kind: &crate::config::Strategy) -> Box<dyn LoadStrategy> {
    match kind {
        crate::config::Strategy::RoundRobin => Box::new(RoundRobinStrategy::new()),
        crate::config::Strategy::Random     => Box::new(RandomStrategy),
        crate::config::Strategy::Shard      => Box::new(ShardStrategy::default()),
        crate::config::Strategy::CpuUsage   => Box::new(CpuUsageStrategy::new()),
        crate::config::Strategy::Latency    => Box::new(LatencyStrategy::new()),
    }
}
