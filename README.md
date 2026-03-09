# moleculer-rs 🦀

> Full-featured Rust port of [Moleculer.js](https://moleculer.services) — a progressive microservices framework.

[![Rust](https://img.shields.io/badge/rust-1.75+-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## What is Moleculer?

Moleculer is a fast, modern microservices framework. This project is a complete Rust reimplementation — with fearless concurrency, zero-cost abstractions, and no garbage collector.

## Architecture

```
┌───────────────────────────────────────────────────────────────────────────┐
│                            ServiceBroker                                  │
│                                                                           │
│  ┌────────────────┐   ┌──────────────────────────────────────────────┐   │
│  │    Registry    │   │         Middleware Pipeline                   │   │
│  │  • services    │   │  Logging → Metrics → Tracing → Timeout →     │   │
│  │  • actions     │   │  Retry → CircuitBreaker → Bulkhead → Cacher  │   │
│  │  • events      │   └──────────────────────────────────────────────┘   │
│  │  • nodes       │                                                       │
│  │  • topology    │   ┌──────────────┐  ┌───────────┐  ┌──────────────┐ │
│  └────────────────┘   │   Channels   │  │  Metrics  │  │   Tracing    │ │
│                        │  • InMemory  │  │  Counter  │  │  SpanStore   │ │
│  ┌────────────────┐   │  • DLQ       │  │  Gauge    │  │  Parent/child│ │
│  │   LRU Cacher   │   │  • Consumer  │  │  Histogram│  │  Exporters   │ │
│  │  • Memory LRU  │   │    Groups    │  │  Prometheus│ └──────────────┘ │
│  │  • Key gen     │   │  • ACK/NACK  │  └───────────┘                   │
│  └────────────────┘   └──────────────┘                                   │
│                                                                           │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                    Laboratory Agent (:3210)                        │  │
│  │  /health /info /services /topology /metrics /traces /logs          │  │
│  │  /channels /cache /metrics/prometheus /nodes /actions              │  │
│  └────────────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────────────┘
```

## Features

| Feature | Status | Notes |
|---------|--------|-------|
| ServiceBroker | ✅ | call / emit / broadcast / ping |
| ServiceSchema | ✅ | actions, events, hooks |
| Mixins | ✅ | Schema composition + merge |
| Context | ✅ | params, meta, headers, tracing, level |
| Service Registry | ✅ | Local catalog, endpoint resolution |
| Load Balancing | ✅ | Round-Robin, Random, Shard |
| **Circuit Breaker** | ✅ | Full CLOSED/HALF_OPEN/OPEN state machine |
| **Retry Policy** | ✅ | Exponential backoff, retryable errors |
| **Bulkhead** | ✅ | Semaphore + overflow queue per action |
| **Cacher (LRU)** | ✅ | Memory LRU, key generation, TTL |
| Timeout | ✅ | Per-request timeout middleware |
| Middleware Pipeline | ✅ | Full ordered pipeline |
| Transporter | ✅ | Trait + in-process LocalTransporter |
| Metrics | ✅ | Counter, Gauge, Histogram + Prometheus export |
| Distributed Tracing | ✅ | Spans, parent/child, by-trace query |
| **Channels (durable queues)** | ✅ | `@moleculer/channels` equivalent |
| Channels: In-memory | ✅ | Tokio MPSC-backed |
| Channels: Consumer Groups | ✅ | Competing consumers within a group |
| Channels: ACK/NACK | ✅ | Explicit acknowledgement |
| Channels: Retry + backoff | ✅ | Exponential backoff on NACK |
| Channels: Dead Letter Queue | ✅ | Configurable DLQ per channel |
| Laboratory Agent | ✅ | HTTP JSON API on :3210 |
| Lab: Topology graph | ✅ | Service call graph (who calls whom) |
| Lab: Metrics reporter | ✅ | Real-time metrics snapshot |
| Lab: Trace exporter | ✅ | Recent distributed spans |
| Lab: Event logger | ✅ | In-memory ring-buffer log collector |
| Lab: Prometheus metrics | ✅ | `/metrics/prometheus` endpoint |

---

## What was learned from reverse-engineering Moleculer

### What the original has that we now implement

**1. Circuit Breaker** — not just config, a real state machine:
- `CLOSED` → counts failures in a rolling window
- `OPEN` when `failures/total >= threshold` and `count >= minRequestCount`
- `HALF_OPEN` after `halfOpenTime` ms — lets ONE probe request through
- `HALF_OPEN_WAIT` — blocks all others while probe is in flight
- `CLOSED` again if probe succeeds; re-`OPEN` if it fails

**2. Retry** — exponential backoff with `factor`:
```
delay = min(delay * factor^(attempt-1), maxDelay)
```

**3. Bulkhead** — semaphore per action, with overflow queue up to `maxQueueSize`

**4. Cacher** — transparent `get/set` around actions with `cache: true` + key templates

**5. `@moleculer/channels`** — the critical missing module:
- Messages stored durably until ACK'd (unlike `emit` which is fire-and-forget)
- Each channel has consumer groups — within a group, messages are load-balanced
- `maxInFlight` limits concurrent processing per consumer
- NACK triggers exponential-backoff retry
- After `maxRetries` NACKs → Dead Letter Queue

**6. Laboratory topology graph** (the network diagram in the screenshot):
- Each service is a node
- Each `broker.call()` creates an edge with `req/min` and `latency`
- The registry tracks call stats per `(caller → action)` pair

**7. Mixins** — schema composition: a mixin's actions/events merge into the service,
but own definitions take priority

---

## Quick Start

```rust
use moleculer::prelude::*;
use serde_json::json;

#[tokio::main]
async fn main() {
    let broker = ServiceBroker::new(
        BrokerConfig::default()
            .with_metrics()
            .with_tracing()
            .with_circuit_breaker()
            .with_retry()
            .with_channels()
    );

    broker.install_default_middlewares().await;

    broker.add_service(
        ServiceSchema::new("math")
            .action(ActionDef::new("add", |ctx| async move {
                let a = ctx.params["a"].as_f64().unwrap_or(0.0);
                let b = ctx.params["b"].as_f64().unwrap_or(0.0);
                Ok(json!({ "result": a + b }))
            }).cache(vec!["a", "b"]))
    ).await;

    broker.start().await.unwrap();
    let res = broker.call("math.add", json!({ "a": 2, "b": 3 })).await.unwrap();
    println!("{}", res); // {"result":5.0}
}
```

## Channels (durable queues)

```rust
use moleculer::channels::{ChannelDef, ChannelMessage};

// Subscribe
broker.subscribe_channel(
    ChannelDef::new("orders.created", |msg: ChannelMessage| async move {
        println!("Order: {}", msg.payload);
        msg.ack().await  // or msg.nack() to retry
    })
    .group("order-processor")
    .max_in_flight(5)
    .max_retries(3)
    .dead_letter("orders.DLQ"),
).await.unwrap();

// Publish
broker.send_to_channel("orders.created", json!({ "id": "ORD-001" })).await.unwrap();
```

## Circuit Breaker

```rust
let config = BrokerConfig::default().with_circuit_breaker();
// After 50% failure rate over 20+ requests:
// → circuit OPENS → subsequent calls immediately fail with CircuitBreakerOpen
// → after 10s → HALF_OPEN → probe request allowed through
// → success → CLOSED again
```

## Laboratory

```rust
use moleculer::laboratory::{AgentService, AgentConfig};
use std::sync::Arc;

let agent = Arc::new(AgentService::with_config(
    Arc::clone(&broker),
    AgentConfig { port: 3210, token: None, name: "my-app".into() },
));
agent.spawn(); // background thread

// Now open https://lab.moleculer.services → http://localhost:3210
```

### API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check |
| `GET /info` | Node, broker, feature flags |
| `GET /services` | Services with actions/events |
| `GET /nodes` | Cluster nodes |
| `GET /actions` | All action endpoints |
| `GET /topology` | Service call graph (nodes + edges with latency) |
| `GET /metrics` | Metrics snapshot (JSON) |
| `GET /metrics/prometheus` | Prometheus text format |
| `GET /traces` | Recent spans (last 500) |
| `GET /traces/{trace_id}` | All spans for a trace |
| `GET /logs` | Recent log entries |
| `GET /channels` | Channel stats + DLQ info |
| `GET /cache` | Cache stats |

## Roadmap

- [ ] Redis Streams channel adapter
- [ ] AMQP channel adapter  
- [ ] NATS JetStream channel adapter
- [ ] Kafka channel adapter
- [ ] NATS / Redis transporter
- [ ] Parameter validation (JSON Schema)
- [ ] Action fallback handler
- [ ] `$node` internal service
- [ ] Hot reload
- [ ] WebSocket push from Laboratory (live updates)

## License

MIT © 2025 moleculer-rs contributors
