//! Metrics reporters — push metrics to external systems.
//!
//! Mirrors `moleculer/src/metrics/reporters/`:
//! - console.js   → ConsoleReporter
//! - prometheus.js → PrometheusReporter  (built-in via prometheus_text())
//! - event.js     → EventReporter
//! - csv.js       → CsvReporter
//! - statsd.js    → StatsdReporter
//! - datadog.js   → DatadogReporter

use crate::error::Result;
use crate::metrics::{MetricEntry, MetricType, MetricsRegistry};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

// ─── Trait ───────────────────────────────────────────────────────────────────

#[async_trait]
pub trait MetricsReporter: Send + Sync {
    fn name(&self) -> &str;
    async fn report(&self, metrics: &[MetricEntry]) -> Result<()>;
    fn collect_interval(&self) -> Duration { Duration::from_secs(5) }
}

// ─── Console reporter ────────────────────────────────────────────────────────

pub struct ConsoleReporter {
    pub interval: Duration,
    pub filter: Option<String>,
    pub colors: bool,
}

impl Default for ConsoleReporter {
    fn default() -> Self {
        Self { interval: Duration::from_secs(5), filter: None, colors: true }
    }
}

#[async_trait]
impl MetricsReporter for ConsoleReporter {
    fn name(&self) -> &str { "ConsoleReporter" }
    fn collect_interval(&self) -> Duration { self.interval }

    async fn report(&self, metrics: &[MetricEntry]) -> Result<()> {
        println!("\n┌──────────────────────────────────────────────────────────────┐");
        println!("│                    METRICS SNAPSHOT                          │");
        println!("└──────────────────────────────────────────────────────────────┘");
        for m in metrics {
            if let Some(f) = &self.filter {
                if !m.name.contains(f.as_str()) { continue; }
            }
            let labels = if m.labels.is_empty() { String::new() }
            else {
                let l = m.labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(" ");
                format!(" [{}]", l)
            };
            let unit = m.unit.as_deref().unwrap_or("");
            println!("  {:50} {:>12.3} {}{}", m.name, m.value, unit, labels);
        }
        println!();
        Ok(())
    }
}

// ─── CSV reporter ─────────────────────────────────────────────────────────────

pub struct CsvReporter {
    pub path: String,
    pub interval: Duration,
    pub separator: char,
}

impl Default for CsvReporter {
    fn default() -> Self {
        Self { path: "./metrics.csv".into(), interval: Duration::from_secs(10), separator: ',' }
    }
}

#[async_trait]
impl MetricsReporter for CsvReporter {
    fn name(&self) -> &str { "CsvReporter" }
    fn collect_interval(&self) -> Duration { self.interval }

    async fn report(&self, metrics: &[MetricEntry]) -> Result<()> {
        use std::io::Write;
        let is_new = !std::path::Path::new(&self.path).exists();
        let mut file = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(&self.path)
            .map_err(crate::error::MoleculerError::Io)?;
        if is_new {
            writeln!(file, "timestamp{}name{}type{}value{}labels",
                self.separator, self.separator, self.separator, self.separator)
                .map_err(crate::error::MoleculerError::Io)?;
        }
        let ts = chrono::Utc::now().to_rfc3339();
        for m in metrics {
            let labels = m.labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("|");
            let type_str = match m.metric_type {
                MetricType::Counter   => "counter",
                MetricType::Gauge     => "gauge",
                MetricType::Histogram => "histogram",
                MetricType::Info      => "info",
            };
            writeln!(file, "{}{}{}{}{}{}{}{}{}", ts, self.separator, m.name, self.separator,
                type_str, self.separator, m.value, self.separator, labels)
                .map_err(crate::error::MoleculerError::Io)?;
        }
        Ok(())
    }
}

// ─── StatsD reporter ─────────────────────────────────────────────────────────

/// Sends metrics via UDP to a StatsD-compatible server.
///
/// Mirrors `moleculer/src/metrics/reporters/statsd.js`.
pub struct StatsdReporter {
    pub host: String,
    pub port: u16,
    pub prefix: String,
    pub interval: Duration,
    pub max_udp_packet_size: usize,
}

impl Default for StatsdReporter {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8125,
            prefix: "moleculer".into(),
            interval: Duration::from_secs(10),
            max_udp_packet_size: 1432,
        }
    }
}

#[async_trait]
impl MetricsReporter for StatsdReporter {
    fn name(&self) -> &str { "StatsdReporter" }
    fn collect_interval(&self) -> Duration { self.interval }

    async fn report(&self, metrics: &[MetricEntry]) -> Result<()> {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(crate::error::MoleculerError::Io)?;
        let target = format!("{}:{}", self.host, self.port);
        let mut batch = String::new();

        for m in metrics {
            let key = format!("{}.{}", self.prefix, m.name.replace('.', "_").replace('-', "_"));
            let line = match m.metric_type {
                MetricType::Counter   => format!("{}:{}|c\n", key, m.value as i64),
                MetricType::Gauge     => format!("{}:{}|g\n", key, m.value),
                MetricType::Histogram => format!("{}:{}|ms\n", key, m.value),
                MetricType::Info      => continue,
            };
            if batch.len() + line.len() > self.max_udp_packet_size {
                let _ = socket.send_to(batch.as_bytes(), &target).await;
                batch.clear();
            }
            batch.push_str(&line);
        }
        if !batch.is_empty() {
            let _ = socket.send_to(batch.as_bytes(), &target).await;
        }
        Ok(())
    }
}

// ─── Datadog reporter ─────────────────────────────────────────────────────────

/// Sends metrics to Datadog DogStatsD (port 8125, extended StatsD format).
pub struct DatadogMetricsReporter {
    pub host: String,
    pub port: u16,
    pub prefix: String,
    pub default_tags: Vec<String>,
    pub interval: Duration,
}

impl Default for DatadogMetricsReporter {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8125,
            prefix: "moleculer".into(),
            default_tags: Vec::new(),
            interval: Duration::from_secs(10),
        }
    }
}

#[async_trait]
impl MetricsReporter for DatadogMetricsReporter {
    fn name(&self) -> &str { "DatadogReporter" }
    fn collect_interval(&self) -> Duration { self.interval }

    async fn report(&self, metrics: &[MetricEntry]) -> Result<()> {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(crate::error::MoleculerError::Io)?;
        let target = format!("{}:{}", self.host, self.port);

        for m in metrics {
            let key = format!("{}.{}", self.prefix, m.name.replace('.', "_"));
            // DogStatsD tags: |#tag1:val1,tag2:val2
            let mut tags = self.default_tags.clone();
            for (k, v) in &m.labels { tags.push(format!("{}:{}", k, v)); }
            let tag_str = if tags.is_empty() { String::new() }
            else { format!("|#{}", tags.join(",")) };

            let line = match m.metric_type {
                MetricType::Counter   => format!("{}:{}|c{}\n", key, m.value as i64, tag_str),
                MetricType::Gauge     => format!("{}:{}|g{}\n", key, m.value, tag_str),
                MetricType::Histogram => format!("{}:{}|h{}\n", key, m.value, tag_str),
                MetricType::Info      => continue,
            };
            let _ = socket.send_to(line.as_bytes(), &target).await;
        }
        Ok(())
    }
}

// ─── Event reporter ──────────────────────────────────────────────────────────

/// Emits metrics as a moleculer event (`$metrics.snapshot`).
pub struct EventReporter {
    pub event_name: String,
    pub interval: Duration,
    pub broadcast: bool,
}

impl Default for EventReporter {
    fn default() -> Self {
        Self { event_name: "$metrics.snapshot".into(), interval: Duration::from_secs(5), broadcast: false }
    }
}

#[async_trait]
impl MetricsReporter for EventReporter {
    fn name(&self) -> &str { "EventReporter" }
    fn collect_interval(&self) -> Duration { self.interval }

    async fn report(&self, metrics: &[MetricEntry]) -> Result<()> {
        // In a full implementation, broker.emit/broadcast is called here
        log::debug!("[EventReporter] would emit '{}' with {} metrics",
            self.event_name, metrics.len());
        Ok(())
    }
}

// ─── Reporter registry ────────────────────────────────────────────────────────

pub struct ReporterRegistry {
    reporters: Vec<Arc<dyn MetricsReporter>>,
}

impl ReporterRegistry {
    pub fn new() -> Self { Self { reporters: Vec::new() } }

    pub fn add(&mut self, r: Arc<dyn MetricsReporter>) {
        log::info!("[MetricsReporters] registered: {}", r.name());
        self.reporters.push(r);
    }

    pub async fn report_all(&self, registry: &MetricsRegistry) {
        let snapshot = registry.snapshot();
        for r in &self.reporters {
            if let Err(e) = r.report(&snapshot).await {
                log::error!("[MetricsReporters] {} error: {}", r.name(), e);
            }
        }
    }

    /// Build from config string list.
    pub fn from_config(names: &[String]) -> Self {
        let mut reg = Self::new();
        for name in names {
            match name.as_str() {
                "console"    => reg.add(Arc::new(ConsoleReporter::default())),
                "csv"        => reg.add(Arc::new(CsvReporter::default())),
                "statsd"     => reg.add(Arc::new(StatsdReporter::default())),
                "datadog"    => reg.add(Arc::new(DatadogMetricsReporter::default())),
                "event"      => reg.add(Arc::new(EventReporter::default())),
                "prometheus" => { /* built-in via prometheus_text() */ }
                "laboratory" => { /* handled by lab agent */ }
                other        => log::warn!("[MetricsReporters] unknown reporter: {}", other),
            }
        }
        reg
    }

    /// Spawn background collection loop.
    pub fn spawn_collection(self: Arc<Self>, registry: Arc<MetricsRegistry>, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.report_all(&registry).await;
            }
        });
    }
}
