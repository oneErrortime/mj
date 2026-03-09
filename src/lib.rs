//! # moleculer-rs
//!
//! Rust port of the [Moleculer](https://moleculer.services) progressive
//! microservices framework for Node.js.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────┐
//!  │                   ServiceBroker                  │
//!  │  ┌───────────┐  ┌──────────┐  ┌───────────────┐ │
//!  │  │ Registry  │  │ Transit  │  │  Middlewares  │ │
//!  │  └───────────┘  └──────────┘  └───────────────┘ │
//!  │  ┌───────────┐  ┌──────────┐  ┌───────────────┐ │
//!  │  │  Cacher   │  │ Tracer   │  │    Metrics    │ │
//!  │  └───────────┘  └──────────┘  └───────────────┘ │
//!  └──────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use moleculer::{ServiceBroker, BrokerConfig, ServiceSchema, ActionDef};
//! use moleculer::context::Context;
//! use serde_json::Value;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = BrokerConfig::default();
//!     let broker = ServiceBroker::new(config);
//!
//!     let greeter = ServiceSchema::new("greeter")
//!         .action("hello", |ctx: Context| async move {
//!             Ok(serde_json::json!({ "message": "Hello, Moleculer-RS!" }))
//!         });
//!
//!     broker.add_service(greeter).await;
//!     broker.start().await.unwrap();
//! }
//! ```

pub mod broker;
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

// Re-exports for ergonomic top-level use
pub use broker::ServiceBroker;
pub use config::BrokerConfig;
pub use context::Context;
pub use error::{MoleculerError, Result};
pub use service::{ActionDef, EventDef, ServiceSchema};
