//! # moleculer-rs  🦀
//!
//! Full-featured Rust port of [Moleculer.js](https://moleculer.services) —
//! a progressive microservices framework.
//!
//! Includes ports of:
//! - `moleculer`          — core framework
//! - `@moleculer/channels` — durable message queues
//! - `@moleculer/database` — CRUD service mixin
//! - `moleculerjs/workflows` — declarative step pipelines
//!
//! ## Feature flags
//!
//! | Flag       | Enables                                              |
//! |------------|------------------------------------------------------|
//! | `nats`     | NATS transporter + JetStream channel adapter         |
//! | `redis`    | Redis transporter, cacher, channel adapter           |
//! | `amqp`     | AMQP (RabbitMQ) channel adapter                      |
//! | `kafka`    | Kafka channel adapter                                |
//! | `msgpack`  | MessagePack serializer                               |
//! | `jaeger`   | Jaeger tracing exporter                              |
//! | `zipkin`   | Zipkin tracing exporter                              |
//! | `full`     | All of the above                                     |
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        ServiceBroker                                │
//! │                                                                     │
//! │  ┌──────────────┐   ┌──────────────────────────────────────────┐   │
//! │  │   Registry   │   │          Middleware Pipeline              │   │
//! │  │  • services  │   │  Log → Metrics → Trace → Timeout →       │   │
//! │  │  • actions   │   │  Retry → CircuitBreaker → Bulkhead →     │   │
//! │  │  • events    │   │  Throttle → Fallback → Cacher            │   │
//! │  │  • nodes     │   └──────────────────────────────────────────┘   │
//! │  │  • topology  │                                                   │
//! │  └──────────────┘   ┌──────────────┐  ┌───────────┐               │
//! │                      │   Channels   │  │  Metrics  │               │
//! │  ┌──────────────┐   │  • InMemory  │  │  Counter  │               │
//! │  │  LRU Cacher  │   │  • Redis     │  │  Gauge    │               │
//! │  │  Redis Cache │   │  • AMQP      │  │  Histogr. │               │
//! │  └──────────────┘   │  • NATS JS   │  └───────────┘               │
//! │                      │  • Kafka     │                               │
//! │  ┌─────────────┐    └──────────────┘                               │
//! │  │ Transporters│    ┌─────────────────────────────────────────┐    │
//! │  │  • Local    │    │           Tracing                       │    │
//! │  │  • TCP/UDP  │    │  • Console  • Jaeger  • Zipkin          │    │
//! │  │  • NATS     │    │  • Datadog  • Event                     │    │
//! │  │  • Redis    │    └─────────────────────────────────────────┘    │
//! │  └─────────────┘                                                   │
//! │  ┌──────────────────────────────────────────────────────────────┐  │
//! │  │                 Laboratory Agent (:3210)                     │  │
//! │  │  /health /info /services /topology /metrics /traces /logs   │  │
//! │  └──────────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

pub mod broker;
pub mod cache;
pub mod constants;
pub mod logger;
pub mod health;
pub mod serializers;
pub mod transit;
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
pub mod lock;
pub mod runner;
pub mod internals;

#[cfg(feature = "laboratory")]
pub mod laboratory;

#[cfg(feature = "database")]
pub mod database;

pub mod workflows;

/// Convenience re-exports.
pub mod prelude {
    pub use crate::broker::ServiceBroker;
    pub use crate::config::BrokerConfig;
    pub use crate::context::Context;
    pub use crate::error::{MoleculerError, Result};
    pub use crate::service::{ActionDef, EventDef, ServiceSchema};
    pub use crate::channels::{ChannelDef, ChannelMessage};
    pub use crate::middleware::Middleware;
    pub use crate::runner::Runner;

    #[cfg(feature = "database")]
    pub use crate::database::{DatabaseMixin, MemoryAdapter, DatabaseOptions, database_service};

    pub use crate::workflows::{Workflow, Step, WorkflowService};
    pub use crate::streaming::{ActionStream, StreamSender, stream_channel};
    pub use crate::metrics::{MetricsRegistry, MetricEntry};

    #[cfg(feature = "database")]
    pub use crate::database::{NeDbAdapter, MongoAdapter, SqlAdapter};
}

pub use prelude::*;

pub mod streaming;

pub mod hotreload;
