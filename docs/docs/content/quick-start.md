# Quick Start

Get a complete moleculer-rs node running in under two minutes.

## Prerequisites

Rust 1.75+ and Cargo. Install via rustup if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Clone & run

```bash
git clone https://github.com/oneErrortime/mj
cd mj
cargo run
```

This starts a broker with the math service, channels demo, circuit breaker, metrics, tracing, and the Laboratory agent on `:3210`.

## Cargo.toml

```toml
[dependencies]
moleculer  = "0.1"
tokio      = { version = "1", features = ["full"] }
serde_json = "1"
```

## Minimal example

A complete broker with one service, one action, and the Laboratory agent:

```rust
use moleculer::prelude::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 1. Create the broker with all features enabled
    let broker = ServiceBroker::new(
        BrokerConfig::default()
            .with_metrics()
            .with_tracing()
            .with_circuit_breaker()
            .with_retry(RetryConfig::default())
            .with_channels(),
    );

    // 2. Install the default middleware pipeline
    broker.install_default_middlewares().await;

    // 3. Register a service
    broker.add_service(
        ServiceSchema::new("math")
            .action(ActionDef::new("add", |ctx| async move {
                let a = ctx.params["a"].as_f64().unwrap_or(0.0);
                let b = ctx.params["b"].as_f64().unwrap_or(0.0);
                Ok(json!({ "result": a + b }))
            }).cache(vec!["a", "b"]))
    ).await;

    // 4. Start the Laboratory agent on :3210
    let agent = Arc::new(AgentService::with_config(
        Arc::clone(&broker),
        AgentConfig { port: 3210, token: None, name: "demo".into() },
    ));
    agent.spawn();

    // 5. Start the broker
    broker.start().await.unwrap();

    // 6. Call an action
    let res = broker.call("math.add", json!({ "a": 2, "b": 3 })).await.unwrap();
    println!("{}", res);  // {"result": 5.0}
}
```

## What's running

After `cargo run`, the process exposes:

| Endpoint | Description |
|----------|-------------|
| `http://localhost:3210/health` | Health check → `{"ok":true}` |
| `http://localhost:3210/services` | All registered services and actions |
| `http://localhost:3210/topology` | Service call graph (nodes + edges) |
| `http://localhost:3210/metrics/prometheus` | Prometheus text format metrics |
| `http://localhost:3210/traces` | Recent distributed spans (last 500) |
| `http://localhost:3210/channels` | Channel stats + DLQ info |

## Connect the Lab UI

Open the [Laboratory →](/lab/) in your browser, set the endpoint to `http://localhost:3210`, and explore real-time topology, metrics, and traces.

> **Tip:** If you're running the Lab UI from GitHub Pages, you'll need to run moleculer-rs locally with CORS enabled or use a CORS proxy, since GitHub Pages is HTTPS and `localhost:3210` is HTTP.
