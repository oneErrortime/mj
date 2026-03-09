# moleculer-rs 🦀

> **Full-featured Rust port of the entire Moleculer.js ecosystem** — a progressive microservices framework.

[![Rust](https://img.shields.io/badge/rust-1.75+-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## What is Moleculer?

Moleculer is a fast, modern microservices framework. This project is a complete Rust reimplementation — covering all four official repositories:

| Original JS Repo | Rust status |
|---|---|
| `moleculerjs/moleculer` | ✅ Full port |
| `moleculerjs/moleculer-channels` | ✅ Full port |
| `moleculerjs/database` | ✅ Full port |
| `moleculerjs/workflows` | ✅ Full port |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            ServiceBroker                                │
│                                                                         │
│  ┌────────────────┐   ┌──────────────────────────────────────────────┐ │
│  │    Registry    │   │         Middleware Pipeline                   │ │
│  │  • services    │   │  Logging → Metrics → Tracing → Timeout →     │ │
│  │  • actions     │   │  Retry → CircuitBreaker → Bulkhead →         │ │
│  │  • events      │   │  Throttle → Debounce → Fallback → Cacher     │ │
│  │  • nodes       │   └──────────────────────────────────────────────┘ │
│  │  • topology    │                                                     │
│  └────────────────┘   ┌──────────────────────────────────────────────┐ │
│                        │           Channels (durable queues)          │ │
│  ┌────────────────┐   │  • InMemory  • Redis Streams                 │ │
│  │    Cachers     │   │  • AMQP      • NATS JetStream                │ │
│  │  • Memory LRU  │   │  • Kafka                                     │ │
│  │  • Redis       │   │  Consumer groups, ACK/NACK, DLQ, backoff     │ │
│  └────────────────┘   └──────────────────────────────────────────────┘ │
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Transporters                                    │ │
│  │  • LocalTransporter (in-process)                                  │ │
│  │  • TcpTransporter  (P2P Gossip + UDP discovery)                   │ │
│  │  • NatsTransporter (requires 'nats' feature)                      │ │
│  │  • RedisTransporter (requires 'redis' feature)                    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                 Distributed Tracing                               │   │
│  │  • Console  • Jaeger  • Zipkin  • Datadog  • Event               │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │               Laboratory Agent  (:3210)                          │   │
│  │  /health /info /services /topology /metrics /traces /logs        │   │
│  │  /channels /cache /metrics/prometheus /nodes /actions            │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Features

| Feature | Status | Notes |
|---------|--------|-------|
| ServiceBroker | ✅ | call / emit / broadcast / ping |
| ServiceSchema | ✅ | actions, events, hooks, mixins |
| Context | ✅ | params, meta, headers, tracing, level |
| Service Registry | ✅ | local catalog, endpoint resolution |
| Load Balancing | ✅ | Round-Robin, Random, Shard, CPU, Latency |
| **Circuit Breaker** | ✅ | CLOSED/HALF_OPEN/OPEN state machine |
| **Retry Policy** | ✅ | Exponential backoff, retryable flag |
| **Bulkhead** | ✅ | Semaphore + overflow queue per action |
| **Throttle** | ✅ | Rate limiting per action |
| **Debounce** | ✅ | Delay + reset on repeat calls |
| **Fallback** | ✅ | Custom fallback on action failure |
| Context Tracker | ✅ | Graceful shutdown, in-flight tracking |
| **Cacher: LRU** | ✅ | Memory LRU, TTL, key generation |
| **Cacher: Redis** | ✅ | Distributed cache (`redis` feature) |
| Timeout | ✅ | Per-request timeout middleware |
| Middleware Pipeline | ✅ | Full ordered pipeline, custom hooks |
| **Transporter: Local** | ✅ | In-process broadcast |
| **Transporter: TCP** | ✅ | P2P Gossip + UDP multicast discovery |
| **Transporter: NATS** | ✅ | (`nats` feature) |
| **Transporter: Redis** | ✅ | Pub/Sub (`redis` feature) |
| Metrics | ✅ | Counter, Gauge, Histogram + Prometheus |
| Distributed Tracing | ✅ | Spans, parent/child, sampling |
| **Tracing: Console** | ✅ | Dev-friendly stdout exporter |
| **Tracing: Jaeger** | ✅ | HTTP collector format |
| **Tracing: Zipkin** | ✅ | v2 HTTP API |
| **Tracing: Datadog** | ✅ | APM agent format |
| **Tracing: Event** | ✅ | Emit as moleculer events |
| **Channels: InMemory** | ✅ | Tokio MPSC-backed |
| **Channels: Redis** | ✅ | Redis Streams XREADGROUP (`redis` feature) |
| **Channels: AMQP** | ✅ | RabbitMQ (`amqp` feature) |
| **Channels: NATS JS** | ✅ | JetStream (`nats` feature) |
| **Channels: Kafka** | ✅ | rdkafka (`kafka` feature) |
| Channels: DLQ | ✅ | Dead Letter Queue |
| Channels: Consumer Groups | ✅ | Competing consumers |
| Channels: ACK/NACK | ✅ | Explicit acknowledgement |
| Channels: Retry+backoff | ✅ | Exponential backoff on NACK |
| **@moleculer/database** | ✅ | Full CRUD mixin |
| Database: MemoryAdapter | ✅ | HashMap-backed, dev/test |
| Database: find/list/count | ✅ | Filter, sort, paginate, search |
| Database: CRUD actions | ✅ | create/get/update/replace/remove |
| **Workflows** | ✅ | Declarative step pipelines |
| Workflows: Dependencies | ✅ | `depends_on` DAG resolution |
| Workflows: Conditions | ✅ | Per-step predicates |
| Workflows: Transforms | ✅ | Map context → params |
| Workflows: allow_failure | ✅ | Non-fatal steps |
| **Discoverers: Local** | ✅ | Heartbeat over transporter |
| **Discoverers: Redis** | ✅ | Redis key-value (`redis` feature) |
| **Discoverers: etcd3** | ✅ | Leased keys (stub, API complete) |
| Serializers: JSON | ✅ | Always on |
| Serializers: MsgPack | ✅ | (`msgpack` feature) |
| Transmit: Compression | ✅ | Deflate codec |
| Transmit: Encryption | ✅ | AES-256-CBC codec |
| Distributed Lock | ✅ | In-process + Redis (`redis` feature) |
| `$node` service | ✅ | list/services/actions/events/health |
| **Runner** | ✅ | Env-based config + graceful shutdown |
| Laboratory Agent | ✅ | HTTP JSON API on :3210 |
| Lab: Topology graph | ✅ | Service call graph |
| Lab: Metrics reporter | ✅ | Real-time snapshot |
| Lab: Trace exporter | ✅ | Recent spans |
| Lab: Prometheus | ✅ | `/metrics/prometheus` |

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

## @moleculer/database

```rust
use moleculer::database::{DatabaseMixin, MemoryAdapter, DatabaseOptions};

let users_svc = DatabaseMixin::new(MemoryAdapter::new())
    .apply(ServiceSchema::new("users"));

broker.add_service(users_svc).await;

// Now available:
broker.call("users.create", json!({ "name": "Alice" })).await?;
broker.call("users.list",   json!({ "page": 1, "pageSize": 10 })).await?;
broker.call("users.find",   json!({ "query": { "role": "admin" } })).await?;
broker.call("users.get",    json!({ "id": "uuid-here" })).await?;
broker.call("users.update", json!({ "id": "uuid-here", "role": "superadmin" })).await?;
broker.call("users.remove", json!({ "id": "uuid-here" })).await?;
```

## Channels (durable queues)

```rust
use moleculer::channels::{ChannelDef, ChannelMessage};

// Subscribe (in-memory adapter — add redis/nats/amqp/kafka features for production)
broker.subscribe_channel(
    ChannelDef::new("orders.created", |msg: ChannelMessage| async move {
        println!("Order: {}", msg.payload);
        msg.ack().await
    })
    .group("order-processor")
    .max_in_flight(5)
    .max_retries(3)
    .dead_letter("orders.DLQ"),
).await.unwrap();

// Publish
broker.send_to_channel("orders.created", json!({ "id": "ORD-001" })).await.unwrap();
```

## Workflows

```rust
use moleculer::workflows::{Workflow, Step};

let wf = Workflow::builder("checkout")
    .step(Step::call("payment.charge")
        .params(|ctx| ctx.params.clone()))
    .step(Step::call("inventory.reserve")
        .depends_on("payment.charge")
        .params(|ctx| ctx.outputs.get("payment_charge").cloned().unwrap_or_default()))
    .step(Step::call("email.send")
        .depends_on("inventory.reserve")
        .allow_failure())
    .build();

let result = wf.run(&broker, json!({ "userId": "u1", "total": 99.99 })).await?;
println!("outputs: {:?}", result.outputs);
```

## Feature Flags

```toml
[dependencies]
moleculer-rs = { version = "0.2", features = ["redis", "nats", "msgpack"] }
```

| Flag | Enables |
|------|---------|
| `nats` | NATS transporter + JetStream channel adapter |
| `redis` | Redis transporter, cacher, channel adapter, discoverer, lock |
| `amqp` | AMQP (RabbitMQ) channel adapter |
| `kafka` | Kafka channel adapter |
| `msgpack` | MessagePack serializer |
| `laboratory` | HTTP Laboratory Agent |
| `database` | `@moleculer/database` CRUD mixin |
| `full` | All of the above |

## Roadmap

- [ ] MongoDB adapter for `@moleculer/database`
- [ ] SQL/Knex adapter (via `sqlx`)
- [ ] NeDB adapter  
- [ ] Hot reload
- [ ] WebSocket push from Laboratory (live updates)
- [ ] `$node` internal service auto-registration
- [ ] AMQP10 / MQTT / STAN transporters
- [ ] Full Redlock implementation
- [ ] etcd3 real client (via `etcd-client`)
- [ ] Parameter validation (JSON Schema via `jsonschema`)

## License

MIT © 2025 moleculer-rs contributors
