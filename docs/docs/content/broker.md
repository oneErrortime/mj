# ServiceBroker

The central hub of every moleculer-rs node. Wires together the Registry, Middleware Pipeline, Cacher, Channels, Metrics, and Tracing into a single `Arc<ServiceBroker>`.

**Source:** `src/broker.rs`

## Construction

Create a broker with `ServiceBroker::new(config)`. The config builder uses a fluent API:

```rust
let broker = ServiceBroker::new(
    BrokerConfig::default()
        .node_id("my-node")               // optional; auto-generated if omitted
        .with_metrics()                   // Counter/Gauge/Histogram
        .with_tracing()                   // distributed SpanStore
        .with_cacher(CacherConfig {
            enabled:  true,
            max_size: 1000,               // LRU capacity
            ttl:      60_000,             // ms; 0 = no TTL
        })
        .with_circuit_breaker()           // default thresholds
        .with_retry(RetryConfig {
            retries:   3,
            delay:     100,               // ms initial
            factor:    2.0,               // exponential factor
            max_delay: 2000,              // ms cap
        })
        .with_channels()                  // in-memory durable channel adapter
        .strategy(BalancingStrategy::RoundRobin),
);
```

The broker is always wrapped in `Arc<ServiceBroker>` — it is cheaply cloneable and safe to pass across tasks.

## Services & Actions

### ServiceSchema

A service is a named unit with actions and events. Schemas are registered with `broker.add_service()`:

```rust
broker.add_service(
    ServiceSchema::new("greeter")
        // Simple action
        .action(ActionDef::new("hello", |ctx| async move {
            let name = ctx.params["name"].as_str().unwrap_or("World");
            Ok(json!({ "msg": format!("Hello, {}!", name) }))
        }))
        // Cached action — key derived from params ["a", "b"]
        .action(ActionDef::new("add", |ctx| async move {
            let (a, b) = (
                ctx.params["a"].as_f64().unwrap_or(0.0),
                ctx.params["b"].as_f64().unwrap_or(0.0),
            );
            Ok(json!({ "result": a + b }))
        }).cache(vec!["a", "b"]))
).await;
```

### Mixins

Schemas compose via mixins. Mixin actions/events merge into the parent service; own definitions win conflicts. Mirrors [Moleculer.js mixins](https://moleculer.services/docs/0.14/services.html#Mixins).

```rust
let base = ServiceSchema::new("_base")
    .action(ActionDef::new("ping", |_| async { Ok(json!({"pong": true})) }));

let service = ServiceSchema::new("my-service")
    .mixin(base)          // inherits ping action
    .action(/* own actions override mixin */);
```

## Calling actions

### broker.call()

Executes an action and waits for the result. Passes through the full middleware pipeline.

```rust
// Simple call
let result = broker.call("math.add", json!({"a": 1, "b": 2})).await?;

// Call with options (timeout, retries, meta)
let result = broker.call_with_options(
    "math.add",
    json!({"a": 1, "b": 2}),
    CallOptions {
        timeout: Some(5000),    // ms; overrides global
        retries: Some(2),
        meta:    json!({"user_id": "u123"}),
    },
).await?;
```

### emit & broadcast

`emit` is fire-and-forget to one subscriber (load-balanced within event group). `broadcast` sends to all subscribers.

```rust
// fire-and-forget event (load-balanced within group)
broker.emit("order.created", json!({"id": "ORD-1"})).await;

// broadcast to every subscriber on every node
broker.broadcast("config.updated", json!({})).await;
```

> **emit vs channels:** `emit`/`broadcast` are **not durable**. If the consumer is offline, the event is lost. Use [Channels](/docs?name=Channels) for guaranteed delivery.

## Context

Every action handler receives a `Context` carrying request-scoped data through the entire call chain. Parent trace IDs propagate automatically for distributed tracing.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Unique context UUID |
| `action_name` | `String` | Fully-qualified action name |
| `params` | `serde_json::Value` | Input parameters |
| `meta` | `serde_json::Value` | Request metadata (propagated across calls) |
| `headers` | `HashMap<String, String>` | Transport headers |
| `level` | `u32` | Call depth (root = 1) |
| `request_id` | `String` | Correlation ID across the full call chain |
| `parent_id` | `Option<String>` | Parent context ID for tracing |

## Load balancing

The `ServiceRegistry` tracks all service endpoints. When a call resolves multiple endpoints, a strategy picks which one to use:

| Strategy | Description |
|----------|-------------|
| `RoundRobin` | Cycles through endpoints in order. **Default.** |
| `Random` | Picks a random endpoint per call. |
| `Shard { shard_key }` | Consistent hashing on a param field. Same key → same instance. |

```rust
BrokerConfig::default()
    .strategy(BalancingStrategy::Shard {
        shard_key: "userId".into(),
    })
```

## BrokerConfig reference

| Method | Default | Description |
|--------|---------|-------------|
| `.node_id(s)` | auto | Override node identifier |
| `.with_metrics()` | disabled | Enable Counter/Gauge/Histogram registry |
| `.with_tracing()` | disabled | Enable distributed SpanStore |
| `.with_cacher(cfg)` | disabled | Enable LRU cacher with capacity + TTL |
| `.with_circuit_breaker()` | disabled | Enable circuit breaker with defaults |
| `.with_circuit_breaker_config(cfg)` | — | Enable circuit breaker with custom config |
| `.with_retry(cfg)` | disabled | Enable retry with exponential backoff |
| `.with_channels()` | disabled | Enable in-memory channel adapter |
| `.strategy(s)` | `RoundRobin` | Load-balancing strategy |
