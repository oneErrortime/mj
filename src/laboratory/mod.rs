//! Moleculer Laboratory — developer tool for Moleculer-RS.
//!
//! # What is it?
//!
//! The Laboratory is a developer observability tool that mirrors
//! [`@moleculer/lab`](https://moleculer.services/laboratory/introduction.html).
//!
//! It consists of:
//! * **AgentService** — runs an HTTP server on port 3210 and exposes a JSON API
//!   that the frontend (lab.moleculer.services) can connect to.
//! * **MetricReporter** — reads from the broker's MetricsRegistry and serves them.
//! * **TraceExporter** — reads from the broker's SpanStore and serves them.
//! * **EventLogger** — captures log records and streams them.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use moleculer::{ServiceBroker, BrokerConfig};
//! use moleculer::laboratory::AgentService;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = BrokerConfig::default().with_metrics().with_tracing();
//!     let broker = ServiceBroker::new(config);
//!     broker.start().await.unwrap();
//!
//!     let agent = AgentService::new(Arc::clone(&broker));
//!     agent.start().await.unwrap();
//!
//!     // open https://lab.moleculer.services and point it at http://localhost:3210
//! }
//! ```

pub mod agent;
pub mod logger;
pub mod metrics_reporter;
pub mod trace_exporter;

pub use agent::AgentService;
pub use logger::EventLogger;
pub use metrics_reporter::MetricReporter;
pub use trace_exporter::TraceExporter;
pub use agent::AgentConfig;
