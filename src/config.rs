//! Broker configuration — mirrors Moleculer's BrokerOptions.

use serde::{Deserialize, Serialize};

/// Log level enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Default for LogLevel {
    fn default() -> Self { LogLevel::Info }
}

/// Load-balancing strategy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    #[default]
    RoundRobin,
    Random,
    CpuUsage,
    Latency,
}

/// Circuit-breaker options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    /// Number of failures before opening the circuit.
    pub threshold: u32,
    /// Minimum requests before circuit breaker kicks in.
    pub min_request_count: u32,
    /// Half-open check interval in ms.
    pub half_open_time: u64,
    /// Window size in seconds for counting failures.
    pub window_time: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 5,
            min_request_count: 20,
            half_open_time: 10_000,
            window_time: 60,
        }
    }
}

/// Retry policy options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub enabled: bool,
    pub retries: u32,
    /// Delay in ms between retries.
    pub delay: u64,
    pub max_delay: u64,
    pub factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retries: 5,
            delay: 100,
            max_delay: 1000,
            factor: 2.0,
        }
    }
}

/// Bulkhead options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkheadConfig {
    pub enabled: bool,
    /// Max concurrent requests.
    pub concurrency: usize,
    /// Max queue size.
    pub max_queue_size: usize,
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self { enabled: false, concurrency: 10, max_queue_size: 100 }
    }
}

/// Metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    /// Reporter type: "console", "prometheus", "laboratory".
    pub reporter: Option<String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self { enabled: false, reporter: None }
    }
}

/// Tracing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    /// Exporter type: "console", "jaeger", "zipkin", "laboratory".
    pub exporter: Option<String>,
    /// Sampling rate 0.0–1.0.
    pub sampling_rate: f64,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self { enabled: false, exporter: None, sampling_rate: 1.0 }
    }
}

/// Top-level broker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    /// Namespace to segment nodes.
    pub namespace: String,
    /// Unique node ID. Defaults to `hostname-PID`.
    pub node_id: Option<String>,
    pub log_level: LogLevel,
    /// Request timeout in ms (0 = disabled).
    pub request_timeout: u64,
    /// Max concurrent requests.
    pub max_call_level: u32,
    /// Heartbeat interval in ms.
    pub heartbeat_interval: u64,
    /// Heartbeat timeout in ms.
    pub heartbeat_timeout: u64,
    /// Prefer local services over remote.
    pub prefer_local: bool,
    pub strategy: Strategy,
    pub circuit_breaker: CircuitBreakerConfig,
    pub retry: RetryConfig,
    pub bulkhead: BulkheadConfig,
    pub metrics: MetricsConfig,
    pub tracing: TracingConfig,
    /// Max outbound queue size.
    pub max_queue_size: usize,
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
            max_queue_size: 50_000,
        }
    }
}

impl BrokerConfig {
    pub fn new() -> Self { Self::default() }

    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = ns.into(); self
    }
    pub fn node_id(mut self, id: impl Into<String>) -> Self {
        self.node_id = Some(id.into()); self
    }
    pub fn log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level; self
    }
    pub fn request_timeout(mut self, ms: u64) -> Self {
        self.request_timeout = ms; self
    }
    pub fn prefer_local(mut self, v: bool) -> Self {
        self.prefer_local = v; self
    }
    pub fn with_metrics(mut self) -> Self {
        self.metrics.enabled = true; self
    }
    pub fn with_tracing(mut self) -> Self {
        self.tracing.enabled = true; self
    }
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker.enabled = true; self
    }
    pub fn with_retry(mut self) -> Self {
        self.retry.enabled = true; self
    }
}
