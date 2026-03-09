# Introduction

**moleculer-rs** is a full-featured Rust reimplementation of [Moleculer.js](https://moleculer.services) — the progressive microservices framework. Built on Tokio's async runtime with fearless concurrency, zero-cost abstractions, and no garbage collector.

> This is **not** a thin wrapper around the JS version. Every component is a native Rust reimplementation designed for correctness and performance.

## What is Moleculer?

Moleculer is a fast, modern microservices framework. Services expose **actions** (like RPC) and **events** (pub/sub). A **ServiceBroker** routes calls through a configurable middleware pipeline, handles load balancing across service instances, and provides built-in fault tolerance.

moleculer-rs ports every core concept to Rust:

| Concept | moleculer.js | moleculer-rs |
|---------|-------------|-------------|
| Service hub | `ServiceBroker` | `ServiceBroker` (`src/broker.rs`) |
| Service definition | `ServiceSchema` object | `ServiceSchema` builder |
| Middleware | `broker.use()` | `broker.add_middleware()` |
| Fault tolerance | Circuit breaker, retry, bulkhead | All three, full state machines |
| Durable messaging | `@moleculer/channels` | `src/channels/` (in-memory adapter) |
| Observability | Metrics + tracing | Prometheus + SpanStore |
| Dashboard | lab.moleculer.services | Laboratory agent on `:3210` |

## Why Rust?

Node.js inherits GC pauses, a single-threaded event loop, and high per-process memory. Rust eliminates all three:

- **Zero GC** — ownership and borrowing replace garbage collection. No pauses, predictable latency.
- **True parallelism** — Tokio tasks run across all CPU cores. No single-threaded event loop constraint.
- **Memory safety** — data races and use-after-free are compile-time errors, not runtime crashes.
- **Single binary** — deploy a statically-linked binary; no Node.js runtime, no `node_modules`.

## Repository layout

```
mj/
├── src/                     # Rust core — moleculer-rs
│   ├── broker.rs            # ServiceBroker
│   ├── service.rs           # ServiceSchema, ActionDef, Mixins
│   ├── context.rs           # Context + metadata propagation
│   ├── registry/            # Service registry + load balancing
│   ├── middleware/          # circuit_breaker, retry, bulkhead, timeout, cacher
│   ├── channels/            # Durable queues (InMemoryAdapter + Adapter trait)
│   ├── metrics/             # Counter, Gauge, Histogram + Prometheus export
│   ├── tracing/             # Distributed SpanStore
│   └── laboratory/          # HTTP agent (:3210) + topology + logs
│
├── ecosystem/               # JS ecosystem (bundled for reference)
│   ├── moleculer-channels/  # @moleculer/channels — Redis, AMQP, NATS, Kafka
│   ├── moleculer-database/  # @moleculer/database — NeDB, MongoDB, Knex
│   └── moleculer-workflows/ # @moleculer/workflows — Temporal-style workflows
│
├── docs/                    # GitHub Pages site (this site)
└── examples/                # Rust usage examples
```

## Feature status

| Feature | Status | File |
|---------|--------|------|
| ServiceBroker | ✅ Done | `src/broker.rs` |
| ServiceSchema + Mixins | ✅ Done | `src/service.rs` |
| Context & metadata | ✅ Done | `src/context.rs` |
| Service Registry | ✅ Done | `src/registry/` |
| Load Balancing (Round-Robin, Random, Shard) | ✅ Done | `src/registry/strategy.rs` |
| Circuit Breaker | ✅ Done | `src/middleware/circuit_breaker.rs` |
| Retry + exponential backoff | ✅ Done | `src/middleware/retry.rs` |
| Bulkhead | ✅ Done | `src/middleware/bulkhead.rs` |
| LRU Cacher | ✅ Done | `src/cache/` |
| Timeout | ✅ Done | `src/middleware/timeout_mw.rs` |
| Channels (durable) | ✅ Done | `src/channels/` |
| Metrics + Prometheus | ✅ Done | `src/metrics/` |
| Distributed Tracing | ✅ Done | `src/tracing/` |
| Laboratory Agent | ✅ Done | `src/laboratory/` |
| Topology Graph | ✅ Done | `src/laboratory/` |
| Redis Streams adapter | 🔲 Planned | — |
| NATS transporter | 🔲 Planned | — |
| Parameter validation | 🔲 Planned | — |
