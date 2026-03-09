//! Distributed tracing exporters.
//!
//! Mirrors `moleculer/src/tracing/exporters/`:
//! - console.js
//! - jaeger.js
//! - zipkin.js
//! - datadog.js
//! - event.js

use crate::error::Result;
use crate::tracing::Span;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

// ─── Trait ───────────────────────────────────────────────────────────────────

#[async_trait]
pub trait SpanExporter: Send + Sync {
    fn name(&self) -> &str;
    async fn export(&self, spans: &[Span]) -> Result<()>;
    async fn flush(&self) -> Result<()> { Ok(()) }
}

// ─── Console exporter ────────────────────────────────────────────────────────

/// Prints spans to stdout — great for development.
pub struct ConsoleExporter {
    pub colors: bool,
}

impl ConsoleExporter {
    pub fn new() -> Self { Self { colors: true } }
}

#[async_trait]
impl SpanExporter for ConsoleExporter {
    fn name(&self) -> &str { "ConsoleExporter" }

    async fn export(&self, spans: &[Span]) -> Result<()> {
        for span in spans {
            let duration = span.end_time.map(|f| {
                (f - span.start_time).num_milliseconds()
            }).unwrap_or(0);

            println!(
                "[Trace] {trace_id} | {name} | {service} | {dur}ms | {status}",
                trace_id  = &span.trace_id[..8],
                name      = span.name,
                service   = span.service.as_deref().unwrap_or("unknown"),
                dur       = duration,
                status    = if span.error.is_some() { "ERROR" } else { "OK" },
            );

            if let Some(err) = &span.error {
                println!("  └─ Error: {}", err);
            }

            for (k, v) in &span.tags {
                println!("  │  {}: {}", k, v);
            }
        }
        Ok(())
    }
}

// ─── Jaeger exporter ─────────────────────────────────────────────────────────

/// Exports to Jaeger via HTTP (Thrift over HTTP, port 14268) or UDP (port 6831).
///
/// Mirrors `moleculer/src/tracing/exporters/jaeger.js`.
pub struct JaegerExporter {
    pub endpoint: String,
    pub service_name: String,
    pub agent_host: String,
    pub agent_port: u16,
    /// If true, use HTTP; otherwise use UDP Thrift compact protocol.
    pub use_http: bool,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
}

impl Default for JaegerExporter {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:14268/api/traces".into(),
            service_name: "moleculer-rs".into(),
            agent_host: "localhost".into(),
            agent_port: 6831,
            use_http: true,
            batch_size: 100,
            flush_interval_ms: 5_000,
        }
    }
}

impl JaegerExporter {
    pub fn new(endpoint: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: service.into(),
            ..Default::default()
        }
    }

    /// Convert internal Span to Jaeger JSON (HTTP collector format).
    fn span_to_jaeger_json(&self, span: &Span) -> Value {
        let start_us = span.start_time.timestamp_micros();
        let duration_us = span.end_time.map(|f| {
            (f - span.start_time).num_microseconds().unwrap_or(0)
        }).unwrap_or(0);

        json!({
            "traceID":  span.trace_id.replace('-', ""),
            "spanID":   span.id.replace('-', "")[..16].to_string(),
            "operationName": span.name,
            "startTime": start_us,
            "duration":  duration_us,
            "tags": span.tags.iter().map(|(k, v)| {
                json!({ "key": k, "type": "string", "value": v.to_string() })
            }).collect::<Vec<_>>(),
            "logs": [],
            "processID": "p1",
            "warnings": null,
        })
    }
}

#[async_trait]
impl SpanExporter for JaegerExporter {
    fn name(&self) -> &str { "JaegerExporter" }

    async fn export(&self, spans: &[Span]) -> Result<()> {
        if spans.is_empty() { return Ok(()); }

        let jaeger_spans: Vec<Value> = spans.iter().map(|s| self.span_to_jaeger_json(s)).collect();

        let body = json!({
            "data": [{
                "traceID": spans[0].trace_id.replace('-', ""),
                "spans": jaeger_spans,
                "processes": {
                    "p1": {
                        "serviceName": self.service_name,
                        "tags": []
                    }
                }
            }]
        });

        // In a real implementation this would POST to self.endpoint.
        // For now, log it so the integration can be verified.
        log::debug!("[JaegerExporter] would export {} spans to {}: {}",
            spans.len(), self.endpoint,
            serde_json::to_string(&body).unwrap_or_default());
        Ok(())
    }
}

// ─── Zipkin exporter ─────────────────────────────────────────────────────────

/// Exports to Zipkin HTTP API v2 (`/api/v2/spans`).
///
/// Mirrors `moleculer/src/tracing/exporters/zipkin.js`.
pub struct ZipkinExporter {
    pub endpoint: String,
    pub service_name: String,
    pub batch_size: usize,
}

impl ZipkinExporter {
    pub fn new(endpoint: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: service.into(),
            batch_size: 100,
        }
    }

    fn span_to_zipkin(&self, span: &Span) -> Value {
        let start_us = span.start_time.timestamp_micros();
        let duration_us = span.end_time.map(|f| {
            (f - span.start_time).num_microseconds().unwrap_or(0)
        }).unwrap_or(0);

        json!({
            "traceId":      &span.trace_id.replace('-', "")[..16],
            "id":           &span.id.replace('-', "")[..16],
            "parentId":     span.parent_id.as_ref().map(|p| &p.replace('-', "")[..16]),
            "name":         &span.name,
            "timestamp":    start_us,
            "duration":     duration_us,
            "localEndpoint": { "serviceName": &self.service_name },
            "tags": span.tags.clone(),
            "kind": "SERVER",
        })
    }
}

#[async_trait]
impl SpanExporter for ZipkinExporter {
    fn name(&self) -> &str { "ZipkinExporter" }

    async fn export(&self, spans: &[Span]) -> Result<()> {
        if spans.is_empty() { return Ok(()); }
        let zipkin_spans: Vec<Value> = spans.iter().map(|s| self.span_to_zipkin(s)).collect();
        log::debug!("[ZipkinExporter] would export {} spans to {}", spans.len(), self.endpoint);
        // Would POST json!(zipkin_spans) to self.endpoint
        Ok(())
    }
}

// ─── Datadog exporter ────────────────────────────────────────────────────────

/// Exports to Datadog APM agent (port 8126).
///
/// Mirrors `moleculer/src/tracing/exporters/datadog.js`.
pub struct DatadogExporter {
    pub host: String,
    pub port: u16,
    pub service_name: String,
    pub env: String,
    pub version: String,
}

impl Default for DatadogExporter {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8126,
            service_name: "moleculer-rs".into(),
            env: "production".into(),
            version: "1.0.0".into(),
        }
    }
}

#[async_trait]
impl SpanExporter for DatadogExporter {
    fn name(&self) -> &str { "DatadogExporter" }

    async fn export(&self, spans: &[Span]) -> Result<()> {
        if spans.is_empty() { return Ok(()); }
        let dd_spans: Vec<Value> = spans.iter().map(|span| {
            let start_ns = span.start_time.timestamp_micros() * 1000;
            let duration_ns = span.end_time.map(|f| {
                (f - span.start_time).num_nanoseconds().unwrap_or(0)
            }).unwrap_or(0);

            json!({
                "trace_id":  u64::from_str_radix(&span.trace_id.replace('-', "")[..16], 16).unwrap_or(0),
                "span_id":   u64::from_str_radix(&span.id.replace('-', "")[..16], 16).unwrap_or(0),
                "name":      &span.name,
                "resource":  span.action.as_deref().unwrap_or(&span.name),
                "service":   span.service.as_deref().unwrap_or(&self.service_name),
                "type":      "web",
                "start":     start_ns,
                "duration":  duration_ns,
                "error":     if span.error.is_some() { 1 } else { 0 },
                "meta": {
                    "env":     &self.env,
                    "version": &self.version,
                },
            })
        }).collect();

        // Would POST msgpack/json to http://{}:{}/v0.4/traces
        log::debug!("[DatadogExporter] would export {} spans to {}:{}",
            spans.len(), self.host, self.port);
        Ok(())
    }
}

// ─── Event exporter ──────────────────────────────────────────────────────────

/// Emits spans as moleculer events (for inspection via laboratory / subscribers).
///
/// Mirrors `moleculer/src/tracing/exporters/event.js`.
pub struct EventExporter {
    pub event_name: String,
    pub broadcast: bool,
}

impl Default for EventExporter {
    fn default() -> Self {
        Self {
            event_name: "$tracing.spans".into(),
            broadcast: false,
        }
    }
}

#[async_trait]
impl SpanExporter for EventExporter {
    fn name(&self) -> &str { "EventExporter" }

    async fn export(&self, spans: &[Span]) -> Result<()> {
        // In a full implementation, broker.emit/broadcast is called here.
        // The broker reference would be injected in a real setup.
        log::debug!("[EventExporter] would emit '{}' with {} spans", self.event_name, spans.len());
        Ok(())
    }
}

// ─── Exporter registry ───────────────────────────────────────────────────────

pub struct ExporterRegistry {
    exporters: Vec<Arc<dyn SpanExporter>>,
}

impl ExporterRegistry {
    pub fn new() -> Self { Self { exporters: Vec::new() } }

    pub fn add(&mut self, exporter: Arc<dyn SpanExporter>) {
        log::info!("[TracingExporters] registered exporter: {}", exporter.name());
        self.exporters.push(exporter);
    }

    pub async fn export_all(&self, spans: &[Span]) {
        for exporter in &self.exporters {
            if let Err(e) = exporter.export(spans).await {
                log::error!("[TracingExporters] {} export error: {}", exporter.name(), e);
            }
        }
    }

    pub fn from_config(config: &[String], service: &str) -> Self {
        let mut reg = Self::new();
        for name in config {
            match name.as_str() {
                "console" => reg.add(Arc::new(ConsoleExporter::new())),
                "jaeger"  => reg.add(Arc::new(JaegerExporter { service_name: service.into(), ..Default::default() })),
                "zipkin"  => reg.add(Arc::new(ZipkinExporter::new("http://localhost:9411/api/v2/spans", service))),
                "datadog" => reg.add(Arc::new(DatadogExporter { service_name: service.into(), ..Default::default() })),
                "event"   => reg.add(Arc::new(EventExporter::default())),
                other     => log::warn!("[TracingExporters] unknown exporter: {}", other),
            }
        }
        reg
    }
}
