# Metrics & Tracing

Built-in observability for moleculer-rs — Prometheus-compatible metrics and distributed tracing with no external dependencies.

**Source:** `src/metrics/` · `src/tracing/`

## Metrics

### Enable

```rust
let broker = ServiceBroker::new(
    BrokerConfig::default()
        .with_metrics()
);
```

### Metric types

moleculer-rs implements the standard Moleculer.js metric types:

| Type | Description | Methods |
|------|-------------|---------|
| `Counter` | Monotonically increasing value | `.increment(n)` |
| `Gauge` | Value that goes up and down | `.set(v)`, `.increment(n)`, `.decrement(n)` |
| `Histogram` | Records distribution of values (latency, size) | `.observe(v)` |

### Built-in metrics

The middleware pipeline automatically records these metrics on every call:

| Metric | Type | Description |
|--------|------|-------------|
| `moleculer.request.total` | Counter | Total request count by action |
| `moleculer.request.error.total` | Counter | Error count by action + error type |
| `moleculer.request.time` | Histogram | Request latency in ms |
| `moleculer.circuit_breaker.state` | Gauge | CB state per action (0=closed, 1=open) |
| `moleculer.channel.messages.total` | Counter | Channel messages published |
| `moleculer.channel.errors.total` | Counter | Channel NACK errors |
| `moleculer.cache.hit.total` | Counter | Cache hits |
| `moleculer.cache.miss.total` | Counter | Cache misses |

### Custom metrics

```rust
let metrics = broker.metrics.clone();

// Counter
let counter = metrics.counter("my_service.requests_processed", "Total processed");
counter.increment(1);

// Gauge
let gauge = metrics.gauge("my_service.queue_depth", "Current queue depth");
gauge.set(42.0);

// Histogram
let hist = metrics.histogram(
    "my_service.processing_time_ms",
    "Processing latency",
    vec![1.0, 5.0, 10.0, 50.0, 100.0, 500.0],  // bucket boundaries
);
hist.observe(37.5);
```

### Prometheus export

Metrics are available in Prometheus text format at `/metrics/prometheus`:

```bash
curl http://localhost:3210/metrics/prometheus
```

Example output:

```
# HELP moleculer_request_total Total request count
# TYPE moleculer_request_total counter
moleculer_request_total{action="math.add"} 1523
moleculer_request_total{action="greeter.hello"} 841

# HELP moleculer_request_time_ms Request latency
# TYPE moleculer_request_time_ms histogram
moleculer_request_time_ms_bucket{action="math.add",le="1"} 1200
moleculer_request_time_ms_bucket{action="math.add",le="5"} 1480
moleculer_request_time_ms_bucket{action="math.add",le="+Inf"} 1523
moleculer_request_time_ms_sum{action="math.add"} 2841.5
moleculer_request_time_ms_count{action="math.add"} 1523
```

### JSON snapshot

```bash
curl http://localhost:3210/metrics
```

Returns a JSON object with all metrics and their current values — useful for dashboards that prefer JSON over Prometheus format.

---

## Distributed Tracing

### Enable

```rust
let broker = ServiceBroker::new(
    BrokerConfig::default()
        .with_tracing()
);
```

### How it works

The `TracingMiddleware` automatically wraps every action call in a span. When `broker.call()` is called from within an existing action context, the new span is linked as a child — creating a distributed trace tree.

```
Request ID: req-abc123
│
├─ span: math.add         (2ms)     ← root span, level=1
│  ├─ span: cache.get     (0.1ms)   ← child, level=2
│  └─ span: store.lookup  (1.5ms)   ← child, level=2
│     └─ span: db.query   (1.2ms)   ← grandchild, level=3
```

### Span fields

| Field | Description |
|-------|-------------|
| `id` | Unique span UUID |
| `trace_id` | Shared across all spans in the same request |
| `parent_id` | Parent span ID (null for root) |
| `action_name` | Fully-qualified action name |
| `node_id` | Node where this span was executed |
| `started_at` | Unix timestamp ms |
| `duration_ms` | Duration in milliseconds |
| `error` | Error message if the action failed |
| `tags` | Key-value metadata (from context meta) |

### Query traces

```bash
# All recent spans (last 500)
curl http://localhost:3210/traces

# All spans for a specific trace
curl http://localhost:3210/traces/req-abc123
```

### Custom spans

```rust
use moleculer::tracing::Span;

// Create a manual child span inside an action handler
async fn my_handler(ctx: Context) -> Result<Value> {
    let span = ctx.start_span("external-api-call");

    let result = call_external_api().await?;

    span.finish();
    Ok(result)
}
```

### Export

The `SpanStore` retains the last 500 spans in memory (ring buffer). The Laboratory agent exposes them via `/traces`. Future adapters: Jaeger (OTLP), Zipkin.

---

## Laboratory API

See the [Laboratory](/docs?name=Laboratory) page for the full HTTP API reference, including how to connect the Lab UI to your running instance.
