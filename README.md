# moleculer-rs 🦀

> Rust port of the [Moleculer](https://moleculer.services) progressive microservices framework.

[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## What is Moleculer?

Moleculer is a fast, modern, and powerful microservices framework.  
This project is a Rust reimplementation of its core concepts — with type safety, zero-cost abstractions, and fearless concurrency.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         ServiceBroker                            │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────────┐   │
│  │  Registry    │  │  Transporter │  │     Middlewares      │   │
│  │  - services  │  │  - local     │  │  - LoggingMiddleware │   │
│  │  - actions   │  │  - (NATS WIP)│  │  - MetricsMiddleware │   │
│  │  - events    │  └──────────────┘  └─────────────────────┘   │
│  │  - nodes     │                                                │
│  └──────────────┘  ┌──────────────┐  ┌─────────────────────┐   │
│                    │   Metrics    │  │      Tracing         │   │
│                    │  (in-memory) │  │   (SpanStore)        │   │
│                    └──────────────┘  └─────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

## Features

- ✅ **ServiceBroker** — central message broker
- ✅ **ServiceSchema** — define services with actions and event listeners
- ✅ **Context** — rich context passed to every handler
- ✅ **Service Registry** — local service catalog with action resolution
- ✅ **Load Balancing** — Round-Robin, Random strategies
- ✅ **Middleware** — logging and metrics middlewares included
- ✅ **Events** — balanced `emit()` and `broadcast()`
- ✅ **Metrics** — in-memory counter/gauge registry
- ✅ **Tracing** — distributed span store
- ✅ **Circuit Breaker** config (enforcement WIP)
- ✅ **Retry** config (enforcement WIP)
- 🔬 **Laboratory Agent** — HTTP API on port 3210

## Laboratory Module

The Laboratory is a developer observability tool — the Rust equivalent of  
[`@moleculer/lab`](https://moleculer.services/laboratory/introduction.html).

### Components

| Component | Description |
|-----------|-------------|
| `AgentService` | HTTP server on port 3210, serves JSON API |
| `MetricReporter` | Collects metrics from the broker |
| `TraceExporter` | Exposes recent tracing spans |
| `EventLogger` | In-memory ring-buffer log collector |

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/info` | Node & broker info |
| GET | `/services` | Registered services |
| GET | `/metrics` | Metrics snapshot |
| GET | `/traces` | Recent spans |
| GET | `/logs` | Recent log entries |

### Usage

```rust
use moleculer::{BrokerConfig, ServiceBroker};
use moleculer::laboratory::{AgentService, AgentConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config = BrokerConfig::default().with_metrics().with_tracing();
    let broker = ServiceBroker::new(config);
    broker.start().await.unwrap();

    let agent = Arc::new(AgentService::new(Arc::clone(&broker)));
    agent.spawn(); // runs in background thread on port 3210

    // Now open https://lab.moleculer.services → connect to http://localhost:3210
}
```

## Quick start

```rust
use moleculer::{BrokerConfig, ServiceBroker, ServiceSchema, service::ActionDef};
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let broker = ServiceBroker::new(BrokerConfig::default());

    broker.add_service(
        ServiceSchema::new("math")
            .action(ActionDef::new("add", |ctx| async move {
                let a = ctx.params["a"].as_f64().unwrap_or(0.0);
                let b = ctx.params["b"].as_f64().unwrap_or(0.0);
                Ok(json!({ "result": a + b }))
            }))
    ).await;

    broker.start().await.unwrap();

    let res = broker.call("math.add", json!({ "a": 2, "b": 3 })).await.unwrap();
    println!("{}", res); // {"result":5.0}

    broker.stop().await.unwrap();
}
```

## Roadmap

- [ ] Circuit Breaker enforcement
- [ ] Retry logic
- [ ] Bulkhead enforcement
- [ ] NATS transporter
- [ ] Redis caching layer
- [ ] Parameter validation
- [ ] Versioned services routing
- [ ] Service mixins
- [ ] Laboratory: live WebSocket streaming

## License

MIT © 2025 moleculer-rs contributors
