# Middleware

Every `broker.call()` passes through an ordered middleware chain. moleculer-rs implements all five fault-tolerance patterns from Moleculer.js.

**Source:** `src/middleware/`

## Pipeline order

Middlewares wrap each other. The call flows inward to the handler; the response flows back out through the same stack:

```
broker.call()
  │
  ▼
LoggingMiddleware         — request/response logging
  ▼
MetricsMiddleware         — increment counters, record latency histograms
  ▼
TracingMiddleware         — open/close distributed span
  ▼
TimeoutMiddleware         — enforce per-request timeout
  ▼
RetryMiddleware           — catch errors, apply exponential backoff, retry
  ▼
CircuitBreakerMiddleware  — track failures, short-circuit when open
  ▼
BulkheadMiddleware        — semaphore per action, overflow queue
  ▼
CacherMiddleware          — check LRU cache; skip handler on hit
  ▼
Action Handler            — your fn(ctx) async → Result<Value>
```

## Circuit Breaker

**Source:** `src/middleware/circuit_breaker.rs`

The circuit breaker monitors failure rates per action endpoint and short-circuits further calls when a failure threshold is breached. It implements the full Moleculer.js state machine:

```
CLOSED ──(failures >= threshold)──► OPEN
  ▲                                  │
  │  (probe succeeds)                │ (half_open_time elapsed)
  │                                  ▼
  └──────────────────────────── HALF_OPEN
       (HALF_OPEN_WAIT: first req
        allowed through; all others
        blocked until probe resolves)
```

**State descriptions:**

- `CLOSED` — Normal operation. Failures tracked in a rolling window. When `failures/total >= threshold` AND `count >= min_request_count` → transitions to `OPEN`.
- `OPEN` — All calls immediately fail with `CircuitBreakerOpen` error. After `half_open_time` ms → transitions to `HALF_OPEN`.
- `HALF_OPEN` — One probe request is allowed through (others queue in `HALF_OPEN_WAIT`). Probe success → `CLOSED`. Probe failure → `OPEN` again.

```rust
CircuitBreakerConfig {
    enabled:           true,
    threshold:         0.5,    // 50% failure rate triggers open
    min_request_count: 20,     // need ≥20 requests before tripping
    window_time:       60_000, // rolling window ms
    half_open_time:    10_000, // ms before probe attempt
}
```

## Retry

**Source:** `src/middleware/retry.rs`

Catches errors and re-invokes the downstream pipeline with exponential backoff. The delay formula mirrors Moleculer.js exactly:

```
delay = min(initial_delay × factor^(attempt−1), max_delay)
```

```rust
RetryConfig {
    retries:   3,     // max retry attempts
    delay:     100,   // initial delay ms
    factor:    2.0,   // multiplier per attempt
    max_delay: 2000,  // cap in ms
}

// Delays for 3 retries with factor=2, initial=100ms:
//   attempt 1: 100ms
//   attempt 2: 200ms
//   attempt 3: 400ms (would be 800ms but capped at 2000ms)
```

> **Note:** Retry sits *outside* the circuit breaker in the pipeline. If a circuit is OPEN, the retry middleware will see `CircuitBreakerOpen` errors and will NOT re-attempt — the circuit short-circuits before retry logic kicks in.

## Bulkhead

**Source:** `src/middleware/bulkhead.rs`

Limits the number of concurrent in-flight calls per action using a Tokio `Semaphore`. Additional requests queue up to `max_queue_size`; requests beyond that are rejected immediately with `BulkheadFull`.

```rust
BulkheadConfig {
    enabled:        true,
    concurrency:    10,    // max concurrent calls
    max_queue_size: 100,   // overflow queue before rejection
}
```

## Timeout

**Source:** `src/middleware/timeout_mw.rs`

Wraps downstream execution with `tokio::time::timeout`. A per-call override can be passed via `CallOptions::timeout`.

```rust
TimeoutConfig {
    enabled: true,
    timeout: 5000,   // ms; per-call can override via CallOptions
}

// Override per-call:
broker.call_with_options("svc.action", params, CallOptions {
    timeout: Some(1000),  // 1s for this call only
    ..Default::default()
}).await?;
```

## Cacher (LRU)

**Source:** `src/cache/`

For actions marked `.cache(keys)`, the cacher computes a cache key from the specified param fields. On a hit, the handler is skipped entirely.

```rust
// Mark an action as cacheable
ActionDef::new("find", handler)
    .cache(vec!["userId", "page"])
// Generated key: "greeter.find:userId=u123:page=2"

// Manual invalidation
broker.cache_delete("greeter.find:userId=u123:*").await;
```

| Config | Default | Description |
|--------|---------|-------------|
| `max_size` | 1000 | LRU capacity (number of entries) |
| `ttl` | 0 | Time-to-live in ms; 0 = no expiry |

## Custom middleware

Implement the `Middleware` trait to insert custom logic anywhere in the pipeline:

```rust
use moleculer::middleware::{Middleware, NextHandler};
use async_trait::async_trait;

pub struct AuditMiddleware;

#[async_trait]
impl Middleware for AuditMiddleware {
    async fn call(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        println!("[audit] → {}", ctx.action_name);
        let result = next.call(ctx).await;
        println!("[audit] ← ok={}", result.is_ok());
        result
    }
}

broker.add_middleware(AuditMiddleware);
```

Custom middlewares are prepended to the pipeline (they execute first, outermost).
