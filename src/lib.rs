//! # moleculer-rs
//!
//! Full-featured Rust port of [Moleculer.js](https://moleculer.services) —
//! a progressive microservices framework.
//!
//! ## What is implemented
//!
//! | Feature                  | Status | Notes                                    |
//! |--------------------------|--------|------------------------------------------|
//! | ServiceBroker            | ✅     | call / emit / broadcast / ping           |
//! | ServiceSchema            | ✅     | actions, events, hooks, mixins           |
//! | Context                  | ✅     | params, meta, tracing, call level        |
//! | Service Registry         | ✅     | local catalog, endpoint resolution       |
//! | Load Balancing           | ✅     | Round-Robin, Random, Shard               |
//! | Circuit Breaker          | ✅     | Closed/HalfOpen/Open + metrics           |
//! | Retry Policy             | ✅     | Exponential backoff, retryable flag      |
//! | Bulkhead                 | ✅     | Semaphore + overflow queue per action    |
//! | Cacher                   | ✅     | Memory LRU + key generation              |
//! | Middleware pipeline      | ✅     | localAction, localEvent, full hooks      |
//! | Action hooks             | ✅     | before / after / error                  |
//! | Mixins                   | ✅     | Schema composition / merge               |
//! | Transporter              | ✅     | Trait + in-process LocalTransporter      |
//! | Metrics                  | ✅     | Counter, Gauge, Histogram + reporters    |
//! | Distributed Tracing      | ✅     | SpanStore, parent/child spans            |
//! | **Channels**             | ✅     | `@moleculer/channels` equivalent         |
//! | Channels: In-memory      | ✅     | Tokio MPSC-backed                        |
//! | Channels: Dead Letter Q  | ✅     | configurable DLQ per channel             |
//! | Channels: Consumer Group | ✅     | group-based competing consumers          |
//! | Channels: ACK/NACK       | ✅     | explicit acknowledgement                 |
//! | Laboratory Agent         | ✅     | HTTP API + topology graph                |
//! | Lab: Metrics reporter    | ✅     |                                          |
//! | Lab: Trace exporter      | ✅     |                                          |
//! | Lab: Event logger        | ✅     |                                          |
//! | Lab: Topology tracking   | ✅     | service call graph (who calls whom)      |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use moleculer::prelude::*;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() {
//!     let broker = ServiceBroker::new(BrokerConfig::default());
//!     broker.add_service(
//!         ServiceSchema::new("math")
//!             .action(ActionDef::new("add", |ctx| async move {
//!                 let a = ctx.params["a"].as_f64().unwrap_or(0.0);
//!                 let b = ctx.params["b"].as_f64().unwrap_or(0.0);
//!                 Ok(json!({ "result": a + b }))
//!             }))
//!     ).await;
//!     broker.start().await.unwrap();
//! }
//! ```

pub mod broker;
pub mod cache;
pub mod constants;
pub mod health;
pub mod serializers;
pub mod validators;
pub mod channels;
pub mod config;
pub mod context;
pub mod error;
pub mod middleware;
pub mod registry;
pub mod service;
pub mod transporter;
pub mod metrics;
pub mod tracing;

#[cfg(feature = "laboratory")]
pub mod laboratory;

/// Convenience re-exports for common use.
pub mod prelude {
    pub use crate::broker::ServiceBroker;
    pub use crate::config::BrokerConfig;
    pub use crate::context::Context;
    pub use crate::error::{MoleculerError, Result};
    pub use crate::service::{ActionDef, EventDef, ServiceSchema};
    pub use crate::channels::{ChannelDef, ChannelMessage};
    pub use crate::middleware::Middleware;
}

pub use prelude::*;
