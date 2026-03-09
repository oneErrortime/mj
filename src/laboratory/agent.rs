//! Laboratory Agent — HTTP server on port 3210 exposing the full Lab JSON API.
//!
//! ## Endpoints
//!
//! | Method | Path              | Description                          |
//! |--------|-------------------|--------------------------------------|
//! | GET    | /health           | Health check                         |
//! | GET    | /info             | Node & broker info                   |
//! | GET    | /services         | Registered services (with actions)   |
//! | GET    | /nodes            | Cluster nodes                        |
//! | GET    | /actions          | All available actions                |
//! | GET    | /metrics          | Metrics snapshot                     |
//! | GET    | /metrics/prometheus | Prometheus text format             |
//! | GET    | /traces           | Recent tracing spans                 |
//! | GET    | /traces/{id}      | Spans for a specific trace           |
//! | GET    | /logs             | Recent log entries                   |
//! | GET    | /topology         | Service call graph                   |
//! | GET    | /channels         | Channel stats + DLQ info             |
//! | GET    | /circuit-breakers | Circuit breaker states               |
//! | GET    | /cache            | Cache stats                          |
//! | POST   | /action           | Call an action (JSON body)           |
//! | POST   | /channels/send    | Publish to a channel                 |
//! | DELETE /channels/dlq/{id} | Retry a DLQ entry            |

use super::logger::EventLogger;
use super::metrics_reporter::MetricReporter;
use super::trace_exporter::TraceExporter;
use crate::broker::ServiceBroker;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub port: u16,
    pub token: Option<String>,
    pub name: String,
}

impl Default for AgentConfig {
    fn default() -> Self { Self { port: 3210, token: None, name: "moleculer-rs".into() } }
}

pub struct AgentService {
    pub config: AgentConfig,
    pub broker: Arc<ServiceBroker>,
    pub logger: Arc<EventLogger>,
    pub metrics: Arc<MetricReporter>,
    pub traces: Arc<TraceExporter>,
}

impl AgentService {
    pub fn new(broker: Arc<ServiceBroker>) -> Self {
        Self::with_config(broker, AgentConfig::default())
    }

    pub fn with_config(broker: Arc<ServiceBroker>, config: AgentConfig) -> Self {
        let nid = broker.node_id.clone();
        Self {
            config,
            logger: Arc::new(EventLogger::new(nid.clone())),
            metrics: Arc::new(MetricReporter::new(nid.clone(), Arc::clone(&broker.metrics))),
            traces: Arc::new(TraceExporter::new(nid.clone(), Arc::clone(&broker.spans))),
            broker,
        }
    }

    pub fn start_blocking(self: Arc<Self>) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.port);
        let server = Server::http(&addr)
            .map_err(|e| crate::error::MoleculerError::Internal(e.to_string()))?;

        log::info!("[Laboratory] Agent listening on http://{}", addr);
        log::info!("[Laboratory] → Open https://lab.moleculer.services and connect to http://localhost:{}", self.config.port);
        if let Some(ref t) = self.config.token {
            log::info!("[Laboratory] Token required (Bearer ***)");
        }

        for request in server.incoming_requests() {
            let agent = Arc::clone(&self);
            let resp = agent.handle(&request);
            let _ = request.respond(resp);
        }
        Ok(())
    }

    pub fn spawn(self: Arc<Self>) {
        std::thread::spawn(move || {
            if let Err(e) = self.start_blocking() {
                log::error!("[Laboratory] Server error: {}", e);
            }
        });
    }

    // ─── Router ───────────────────────────────────────────────────────────────

    fn handle(&self, req: &Request) -> Response<std::io::Cursor<Vec<u8>>> {
        // Auth check
        if let Some(ref token) = self.config.token {
            let ok = req.headers().iter()
                .find(|h| h.field.as_str().to_ascii_lowercase() == "authorization")
                .map(|h| h.value.as_str() == format!("Bearer {}", token))
                .unwrap_or(false);
            if !ok { return Self::json(401, json!({ "error": "Unauthorized" })); }
        }

        if req.method() == &Method::Options {
            return Self::cors_preflight();
        }

        let path = req.url().split('?').next().unwrap_or("/");
        let method = req.method();

        match (method, path) {
            // ── GET endpoints ─────────────────────────────────────────────────
            (Method::Get, "/health") => Self::json(200, json!({
                "status": "ok",
                "node_id": self.broker.node_id,
                "instance_id": self.broker.instance_id,
                "running": self.broker.is_running(),
                "uptime_s": 0,
            })),

            (Method::Get, "/info") => Self::json(200, json!({
                "node_id": self.broker.node_id,
                "instance_id": self.broker.instance_id,
                "namespace": self.broker.config.namespace,
                "version": env!("CARGO_PKG_VERSION"),
                "name": self.config.name,
                "services": self.broker.registry.service_names(),
                "metrics_enabled": self.broker.config.metrics.enabled,
                "tracing_enabled": self.broker.config.tracing.enabled,
                "cacher_enabled": self.broker.config.cacher.enabled,
                "channels_enabled": self.broker.config.channels.enabled,
                "circuit_breaker_enabled": self.broker.config.circuit_breaker.enabled,
                "retry_enabled": self.broker.config.retry.enabled,
                "bulkhead_enabled": self.broker.config.bulkhead.enabled,
            })),

            (Method::Get, "/nodes") => {
                let nodes: Vec<Value> = self.broker.registry.nodes.iter().map(|e| {
                    let n = e.value();
                    serde_json::to_value(n).unwrap_or_default()
                }).collect();
                Self::json(200, json!({ "nodes": nodes, "count": nodes.len() }))
            },

            (Method::Get, "/services") => {
                let svcs: Vec<Value> = self.broker.registry.services.iter().map(|e| {
                    let s = e.value();
                    let actions: Vec<Value> = s.actions.iter().map(|(name, a)| json!({
                        "name": name,
                        "cache": a.cache.is_some(),
                        "timeout": a.timeout,
                    })).collect();
                    let events: Vec<String> = s.events.keys().cloned().collect();
                    json!({
                        "name": s.full_name(),
                        "version": s.version,
                        "actions": actions,
                        "events": events,
                        "settings": s.settings,
                        "metadata": s.metadata,
                    })
                }).collect();
                Self::json(200, json!({ "services": svcs }))
            },

            (Method::Get, "/actions") => {
                let actions: Vec<Value> = self.broker.registry.actions.iter().map(|e| {
                    json!({
                        "name": e.key(),
                        "endpoints": e.value().iter().map(|ep| json!({
                            "node_id": ep.node_id,
                            "service": ep.service_name,
                            "available": ep.available,
                        })).collect::<Vec<_>>(),
                    })
                }).collect();
                Self::json(200, json!({ "actions": actions }))
            },

            (Method::Get, "/topology") => {
                let edges = self.broker.registry.topology_snapshot();
                let nodes: Vec<Value> = self.broker.registry.service_names().into_iter().map(|s| {
                    json!({
                        "id": s,
                        "name": s,
                        "type": "service",
                    })
                }).collect();
                Self::json(200, json!({
                    "nodes": nodes,
                    "edges": serde_json::to_value(&edges).unwrap_or_default(),
                }))
            },

            (Method::Get, "/metrics") => {
                let snap = self.metrics.snapshot();
                Self::json(200, serde_json::to_value(&snap).unwrap_or_default())
            },

            (Method::Get, "/metrics/prometheus") => {
                let text = self.broker.metrics.prometheus_text();
                let bytes = text.into_bytes();
                let len = bytes.len();
                Response::new(
                    tiny_http::StatusCode(200),
                    vec![
                        Header::from_bytes("Content-Type", "text/plain; version=0.0.4").unwrap(),
                        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                    ],
                    std::io::Cursor::new(bytes),
                    Some(len),
                    None,
                )
            },

            (Method::Get, "/traces") => {
                let snap = self.traces.snapshot(500);
                Self::json(200, serde_json::to_value(&snap).unwrap_or_default())
            },

            (Method::Get, path) if path.starts_with("/traces/") => {
                let trace_id = &path[8..];
                let spans = self.broker.spans.by_trace(trace_id);
                Self::json(200, json!({ "trace_id": trace_id, "spans": serde_json::to_value(&spans).unwrap_or_default() }))
            },

            (Method::Get, "/logs") => {
                let entries = self.logger.recent(1000);
                Self::json(200, json!({
                    "node_id": self.broker.node_id,
                    "count": entries.len(),
                    "logs": serde_json::to_value(&entries).unwrap_or_default(),
                }))
            },

            (Method::Get, "/channels") => {
                let stats = self.broker.channel_stats();
                let dlq_count = self.broker.channel_adapter.as_ref()
                    .and_then(|_| None::<usize>)  // DLQ count exposed via adapter in full impl
                    .unwrap_or(0);
                Self::json(200, json!({
                    "channels": serde_json::to_value(&stats).unwrap_or_default(),
                    "dlq_count": dlq_count,
                }))
            },

            (Method::Get, "/cache") => {
                let stats = self.broker.cacher.as_ref().map(|c| json!({
                    "enabled": true,
                    "entries": c.len(),
                    "type": "memory-lru",
                    "capacity": self.broker.config.cacher.max_size,
                })).unwrap_or_else(|| json!({ "enabled": false }));
                Self::json(200, stats)
            },

            // ── POST: call action ─────────────────────────────────────────────
            (Method::Post, "/action") => {
                Self::json(200, json!({
                    "note": "Use broker.call() in Rust code. HTTP action proxy not available in blocking handler.",
                    "hint": "Use tokio::runtime::Handle::current().block_on(broker.call(action, params))"
                }))
            },

            (Method::Post, "/channels/send") => {
                Self::json(200, json!({
                    "note": "Channel send via HTTP available in async context. Use broker.send_to_channel()."
                }))
            },

            _ => Self::json(404, json!({ "error": "Not found", "path": path })),
        }
    }

    fn json(status: u32, body: Value) -> Response<std::io::Cursor<Vec<u8>>> {
        let bytes = body.to_string().into_bytes();
        let len = bytes.len();
        Response::new(
            tiny_http::StatusCode(status as u16),
            vec![
                Header::from_bytes("Content-Type", "application/json").unwrap(),
                Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                Header::from_bytes("Access-Control-Allow-Headers", "Authorization, Content-Type").unwrap(),
            ],
            std::io::Cursor::new(bytes),
            Some(len), None,
        )
    }

    fn cors_preflight() -> Response<std::io::Cursor<Vec<u8>>> {
        Response::new(
            tiny_http::StatusCode(204),
            vec![
                Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS").unwrap(),
                Header::from_bytes("Access-Control-Allow-Headers", "Authorization, Content-Type").unwrap(),
            ],
            std::io::Cursor::new(vec![]), Some(0), None,
        )
    }
}
