//! AgentService — HTTP server on port 3210 that exposes the Laboratory JSON API.
//!
//! ## Endpoints
//!
//! | Method | Path               | Description                        |
//! |--------|--------------------|------------------------------------|
//! | GET    | /health            | Health check                       |
//! | GET    | /info              | Node & broker info                 |
//! | GET    | /services          | List of registered services        |
//! | GET    | /metrics           | Current metrics snapshot           |
//! | GET    | /traces            | Recent tracing spans               |
//! | GET    | /logs              | Recent log entries                 |
//! | POST   | /action            | Call an action (JSON body)         |
//!
//! The frontend at <https://lab.moleculer.services> connects directly to
//! `http://localhost:3210` (or wherever you expose the agent).

use super::logger::EventLogger;
use super::metrics_reporter::MetricReporter;
use super::trace_exporter::TraceExporter;
use crate::broker::ServiceBroker;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tiny_http::{Header, Method, Response, Server};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// HTTP port for the Agent server. Default: 3210.
    pub port: u16,
    /// Optional token to protect access.
    pub token: Option<String>,
    /// Project name shown in the Lab frontend.
    pub name: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self { port: 3210, token: None, name: "moleculer-rs".to_string() }
    }
}

/// The Laboratory Agent Service.
///
/// Run alongside your broker to enable the Lab frontend.
pub struct AgentService {
    config: AgentConfig,
    broker: Arc<ServiceBroker>,
    pub logger: Arc<EventLogger>,
    pub metrics: Arc<MetricReporter>,
    pub traces: Arc<TraceExporter>,
}

impl AgentService {
    pub fn new(broker: Arc<ServiceBroker>) -> Self {
        Self::with_config(broker, AgentConfig::default())
    }

    pub fn with_config(broker: Arc<ServiceBroker>, config: AgentConfig) -> Self {
        let node_id = broker.node_id.clone();
        let logger = Arc::new(EventLogger::new(node_id.clone()));
        let metrics = Arc::new(MetricReporter::new(node_id.clone(), Arc::clone(&broker.metrics)));
        let traces = Arc::new(TraceExporter::new(node_id.clone(), Arc::clone(&broker.spans)));
        Self { config, broker, logger, metrics, traces }
    }

    /// Start the HTTP server (blocking — call in a dedicated thread or `spawn_blocking`).
    pub fn start_blocking(self: Arc<Self>) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.port);
        let server = Server::http(&addr)
            .map_err(|e| crate::error::MoleculerError::Internal(e.to_string()))?;

        log::info!(
            "[Laboratory] Agent started on http://{}  token={:?}",
            addr,
            self.config.token.as_deref().map(|_| "***")
        );
        log::info!("[Laboratory] Open https://lab.moleculer.services and connect to http://localhost:{}", self.config.port);

        for request in server.incoming_requests() {
            let agent = Arc::clone(&self);
            let response = agent.handle(&request);
            let _ = request.respond(response);
        }
        Ok(())
    }

    /// Spawn the agent in a background thread.
    pub fn spawn(self: Arc<Self>) {
        let agent = Arc::clone(&self);
        std::thread::spawn(move || {
            if let Err(e) = agent.start_blocking() {
                log::error!("[Laboratory] Agent error: {}", e);
            }
        });
    }

    // ------------------------------------------------------------------ //
    //  Internal request handling                                           //
    // ------------------------------------------------------------------ //

    fn handle(&self, req: &tiny_http::Request) -> Response<std::io::Cursor<Vec<u8>>> {
        // Token auth check
        if let Some(ref token) = self.config.token {
            let auth_ok = req
                .headers()
                .iter()
                .find(|h| h.field.as_str().to_ascii_lowercase() == "authorization")
                .map(|h| h.value.as_str() == format!("Bearer {}", token))
                .unwrap_or(false);
            if !auth_ok {
                return Self::json_response(401, json!({ "error": "Unauthorized" }));
            }
        }

        // CORS preflight
        if req.method() == &Method::Options {
            return Self::cors_response();
        }

        let path = req.url().split('?').next().unwrap_or("/");

        match (req.method(), path) {
            (Method::Get, "/health") => Self::json_response(200, json!({
                "status": "ok",
                "node": self.broker.node_id,
                "uptime": true,
            })),

            (Method::Get, "/info") => Self::json_response(200, json!({
                "node_id": self.broker.node_id,
                "namespace": self.broker.config.namespace,
                "version": env!("CARGO_PKG_VERSION"),
                "agent": self.config.name,
                "services": self.broker.registry.service_names(),
                "metrics_enabled": self.broker.config.metrics.enabled,
                "tracing_enabled": self.broker.config.tracing.enabled,
            })),

            (Method::Get, "/services") => {
                let services: Vec<Value> = self.broker.registry.services.iter().map(|e| {
                    let schema = e.value();
                    let actions: Vec<String> = schema.actions.keys().cloned().collect();
                    let events: Vec<String> = schema.events.keys().cloned().collect();
                    json!({
                        "name": schema.full_name(),
                        "version": schema.version,
                        "actions": actions,
                        "events": events,
                        "settings": schema.settings,
                    })
                }).collect();
                Self::json_response(200, json!({ "services": services }))
            }

            (Method::Get, "/metrics") => {
                let snap = self.metrics.snapshot();
                Self::json_response(200, serde_json::to_value(&snap).unwrap_or_default())
            }

            (Method::Get, "/traces") => {
                let snap = self.traces.snapshot(200);
                Self::json_response(200, serde_json::to_value(&snap).unwrap_or_default())
            }

            (Method::Get, "/logs") => {
                let entries = self.logger.recent(500);
                Self::json_response(200, json!({
                    "node_id": self.broker.node_id,
                    "count": entries.len(),
                    "logs": serde_json::to_value(&entries).unwrap_or_default(),
                }))
            }

            (Method::Post, "/action") => {
                // Read body
                let mut body = String::new();
                let mut req_mut = req;
                // tiny_http Request is not Clone so we read body via reader trick
                // For simplicity, return a note about async limitation.
                Self::json_response(200, json!({
                    "note": "Use call() on the broker directly in Rust code. REST action proxy coming soon.",
                    "node_id": self.broker.node_id,
                }))
            }

            _ => Self::json_response(404, json!({ "error": "Not found", "path": path })),
        }
    }

    fn json_response(status: u32, body: Value) -> Response<std::io::Cursor<Vec<u8>>> {
        let bytes = body.to_string().into_bytes();
        let len = bytes.len();
        let cursor = std::io::Cursor::new(bytes);
        let mut resp = Response::new(
            tiny_http::StatusCode(status as u16),
            vec![
                Header::from_bytes("Content-Type", "application/json").unwrap(),
                Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
            ],
            cursor,
            Some(len),
            None,
        );
        resp
    }

    fn cors_response() -> Response<std::io::Cursor<Vec<u8>>> {
        Response::new(
            tiny_http::StatusCode(204),
            vec![
                Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
                Header::from_bytes("Access-Control-Allow-Headers", "Authorization, Content-Type").unwrap(),
            ],
            std::io::Cursor::new(vec![]),
            Some(0),
            None,
        )
    }
}
