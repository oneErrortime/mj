# moleculer-rs-client

> Lightweight Node.js HTTP client for the [moleculer-rs](https://github.com/oneErrortime/mj) Rust broker.

Zero runtime dependencies — uses the built-in `fetch` available since **Node 18**.

---

## Installation

```bash
npm install moleculer-rs-client
```

---

## Quick start

```js
const { MoleculerRsClient } = require("moleculer-rs-client");

const broker = new MoleculerRsClient({
  endpoint: "http://localhost:3210",   // default
  // token: "secret",                 // if broker started with --token
  // timeout: 10_000,                 // ms, default 10 s
});

// Call an action
const result = await broker.call("math.add", { a: 5, b: 3 });
console.log(result); // { result: 8 }

// Publish to a channel (durable, ACK-based)
await broker.sendToChannel("orders.created", { id: "ORD-001", total: 99.99 });

// Observe the broker
const info     = await broker.info();
const services = await broker.services();
const metrics  = await broker.metrics();
const traces   = await broker.traces();
```

---

## API

| Method | HTTP | Description |
|--------|------|-------------|
| `health()` | `GET /health` | Broker liveness check |
| `info()` | `GET /info` | Node metadata and feature flags |
| `services()` | `GET /services` | Registered services |
| `actions()` | `GET /actions` | Registered actions |
| `topology()` | `GET /topology` | Service call-graph |
| `metrics()` | `GET /metrics` | Metrics snapshot |
| `traces()` | `GET /traces` | Recent distributed spans |
| `logs()` | `GET /logs` | In-memory ring-buffer logs |
| `metricsPrometheus()` | `GET /metrics/prometheus` | Prometheus text format |
| `circuitBreakers()` | `GET /circuit-breakers` | CB states per action |
| `cache()` | `GET /cache` | LRU cache entries |
| `channels()` | `GET /channels` | Channels, consumer groups, DLQ |
| `call(action, params?)` | `POST /action` | Invoke a service action |
| `sendToChannel(ch, payload)` | `POST /channels/send` | Publish a durable message |
| `retryDlq(id)` | `DELETE /channels/dlq/:id` | Retry / discard a DLQ entry |
| `emit(event, payload?)` | n/a | Fire-and-forget event (if supported) |

All methods return `Promise<any>` and throw `MoleculerRsError` on non-2xx responses.

---

## MoleculerRsError

```js
const { MoleculerRsError } = require("moleculer-rs-client");

try {
  await broker.call("missing.action");
} catch (err) {
  if (err instanceof MoleculerRsError) {
    console.error(err.message); // "Action not found"
    console.error(err.status);  // 404
    console.error(err.body);    // raw response body
  }
}
```

---

## Running the tests

No external dependencies required:

```bash
node --test test/
```
