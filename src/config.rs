//! Broker configuration — full Moleculer option parity.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel { Trace, Debug, Info, Warn, Error, Fatal }
impl Default for LogLevel { fn default() -> Self { LogLevel::Info } }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strategy { #[default] RoundRobin, Random, Shard, CpuUsage, Latency }

/// Circuit-breaker state machine config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    /// Failure rate threshold [0.0, 1.0] to trip open.
    pub threshold: f64,
    pub min_request_count: u32,
    /// How long (ms) to wait before half-opening.
    pub half_open_time: u64,
    /// Rolling window in seconds.
    pub window_time: u64,
}
impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self { enabled: false, threshold: 0.5, min_request_count: 20, half_open_time: 10_000, window_time: 60 }
    }
}

/// Retry policy with exponential back-off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub enabled: bool,
    pub retries: u32,
    pub delay: u64,
    pub max_delay: u64,
    pub factor: f64,
}
impl Default for RetryConfig {
    fn default() -> Self {
        Self { enabled: false, retries: 5, delay: 100, max_delay: 1000, factor: 2.0 }
    }
}

/// Bulkhead — semaphore + overflow queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkheadConfig {
    pub enabled: bool,
    pub concurrency: usize,
    pub max_queue_size: usize,
}
impl Default for BulkheadConfig {
    fn default() -> Self { Self { enabled: false, concurrency: 10, max_queue_size: 100 } }
}

/// Metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    /// Reporter type(s): "console", "prometheus", "laboratory".
    pub reporters: Vec<String>,
    /// Collection interval in seconds.
    pub collect_interval: u64,
}
impl Default for MetricsConfig {
    fn default() -> Self { Self { enabled: false, reporters: Vec::new(), collect_interval: 5 } }
}

/// Distributed tracing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    /// Exporter(s): "console", "jaeger", "zipkin", "laboratory".
    pub exporters: Vec<String>,
    /// Sampling rate [0.0, 1.0].
    pub sampling_rate: f64,
}
impl Default for TracingConfig {
    fn default() -> Self { Self { enabled: false, exporters: Vec::new(), sampling_rate: 1.0 } }
}

/// Cacher configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacherConfig {
    pub enabled: bool,
    /// Cacher type: "memory", "memory-lru", "redis".
    pub kind: String,
    /// LRU capacity.
    pub max_size: usize,
    /// Default TTL in seconds (0 = no expiry).
    pub ttl: u64,
}
impl Default for CacherConfig {
    fn default() -> Self { Self { enabled: false, kind: "memory-lru".into(), max_size: 1000, ttl: 0 } }
}

/// Channels (durable queues) global configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelsConfig {
    pub enabled: bool,
    /// Adapter type: "memory", "redis", "amqp", "kafka", "nats".
    pub adapter: String,
    pub max_retries: u32,
    pub max_in_flight: usize,
    pub dead_lettering: DeadLetteringConfig,
}
impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            adapter: "memory".into(),
            max_retries: 3,
            max_in_flight: 1,
            dead_lettering: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeadLetteringConfig {
    pub enabled: bool,
    pub queue_name: String,
}
impl DeadLetteringConfig {
    pub fn new(queue: impl Into<String>) -> Self {
        Self { enabled: true, queue_name: queue.into() }
    }
}

/// Transit / network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitConfig {
    pub max_queue_size: usize,
    pub max_chunk_size: usize,
    pub disable_reconnect: bool,
}
impl Default for TransitConfig {
    fn default() -> Self { Self { max_queue_size: 50_000, max_chunk_size: 256 * 1024, disable_reconnect: false } }
}

/// Top-level broker configuration — mirrors Moleculer's defaultOptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub namespace: String,
    pub node_id: Option<String>,
    pub log_level: LogLevel,
    /// Default request timeout in ms (0 = disabled).
    pub request_timeout: u64,
    /// Max call level to prevent infinite recursion.
    pub max_call_level: u32,
    pub heartbeat_interval: u64,
    pub heartbeat_timeout: u64,
    /// Prefer local service instances over remote.
    pub prefer_local: bool,
    pub strategy: Strategy,
    pub circuit_breaker: CircuitBreakerConfig,
    pub retry: RetryConfig,
    pub bulkhead: BulkheadConfig,
    pub metrics: MetricsConfig,
    pub tracing: TracingConfig,
    pub cacher: CacherConfig,
    pub channels: ChannelsConfig,
    pub transit: TransitConfig,
    /// Context params deep cloning on every call.
    pub context_params_cloning: bool,
    /// Register internal $node service.
    pub internal_services: bool,
    /// Enable parameter validation middleware.
    pub validator: bool,
    pub metadata: serde_json::Value,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            node_id: None,
            log_level: LogLevel::Info,
            request_timeout: 5000,
            max_call_level: 100,
            heartbeat_interval: 5000,
            heartbeat_timeout: 15000,
            prefer_local: true,
            strategy: Strategy::RoundRobin,
            circuit_breaker: Default::default(),
            retry: Default::default(),
            bulkhead: Default::default(),
            metrics: Default::default(),
            tracing: Default::default(),
            cacher: Default::default(),
            channels: Default::default(),
            transit: Default::default(),
            context_params_cloning: false,
            internal_services: true,
            validator: true,
            metadata: serde_json::Value::Object(Default::default()),
        }
    }
}

impl BrokerConfig {
    pub fn new() -> Self { Self::default() }
    pub fn namespace(mut self, ns: impl Into<String>) -> Self { self.namespace = ns.into(); self }
    pub fn node_id(mut self, id: impl Into<String>) -> Self { self.node_id = Some(id.into()); self }
    pub fn request_timeout(mut self, ms: u64) -> Self { self.request_timeout = ms; self }
    pub fn prefer_local(mut self, v: bool) -> Self { self.prefer_local = v; self }
    pub fn with_metrics(mut self) -> Self { self.metrics.enabled = true; self }
    pub fn with_tracing(mut self) -> Self { self.tracing.enabled = true; self }
    pub fn with_circuit_breaker(mut self) -> Self { self.circuit_breaker.enabled = true; self }
    pub fn with_retry(mut self) -> Self { self.retry.enabled = true; self }
    pub fn with_bulkhead(mut self) -> Self { self.bulkhead.enabled = true; self }
    pub fn with_cacher(mut self) -> Self { self.cacher.enabled = true; self }
    pub fn with_channels(mut self) -> Self { self.channels.enabled = true; self }
    pub fn with_laboratory_metrics(mut self) -> Self {
        self.metrics.enabled = true;
        self.metrics.reporters.push("laboratory".into());
        self
    }
    pub fn with_laboratory_tracing(mut self) -> Self {
        self.tracing.enabled = true;
        self.tracing.exporters.push("laboratory".into());
        self
    }
}
