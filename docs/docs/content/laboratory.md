# Laboratory

The Laboratory Agent exposes a complete HTTP JSON API on port `:3210` for real-time observation of your moleculer-rs instance — topology, metrics, traces, channels, logs, and more.

**Source:** `src/laboratory/`

## Setup

```rust
use moleculer::laboratory::{AgentService, AgentConfig};
use std::sync::Arc;

let agent = Arc::new(AgentService::with_config(
    Arc::clone(&broker),
    AgentConfig {
        port:  3210,
        token: None,          // optional Bearer auth token
        name:  "my-app".into(),
    },
));
agent.spawn();  // runs in a background Tokio task
```

The agent starts a minimal HTTP server (no external framework dependency) that serves all endpoints below.

## API Reference

### Health & Info

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check → `{"ok": true, "uptime_ms": 12345}` |
| `GET` | `/info` | Node ID, instance ID, broker config, enabled features |

```bash
curl http://localhost:3210/health
# {"ok":true,"uptime_ms":45231}

curl http://localhost:3210/info
# {"node_id":"myhost-12345","instance_id":"...","features":{"metrics":true,"tracing":true,...}}
```

### Services & Registry

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/services` | All registered services with their actions and events |
| `GET` | `/nodes` | Cluster nodes (local + discovered remote) |
| `GET` | `/actions` | Flat list of all action endpoints |

```bash
curl http://localhost:3210/services
# [{"name":"math","actions":["math.add"],"events":[]}, ...]
```

### Topology

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/topology` | Service call graph — nodes and edges with latency/throughput |

The topology graph tracks every `broker.call()` made between services. Each edge records:
- `calls_per_min` — request rate
- `avg_latency_ms` — average call latency
- `error_rate` — fraction of failed calls

```json
{
  "nodes": [
    { "id": "api", "type": "service" },
    { "id": "math", "type": "service" }
  ],
  "edges": [
    {
      "from": "api",
      "to": "math.add",
      "calls_per_min": 42.3,
      "avg_latency_ms": 1.8,
      "error_rate": 0.001
    }
  ]
}
```

The Lab UI renders this as an interactive force-directed graph.

### Metrics

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/metrics` | All metrics snapshot as JSON |
| `GET` | `/metrics/prometheus` | Prometheus text format (`text/plain`) |

### Tracing

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/traces` | Recent spans (last 500, newest first) |
| `GET` | `/traces/:trace_id` | All spans belonging to one trace |

### Logs

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/logs` | Recent log entries (ring buffer, last 1000) |

Log entries include level, timestamp, message, and optional structured fields from the action context.

### Channels

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/channels` | All channel stats — message counts, DLQ sizes, in-flight |
| `DELETE` | `/channels/dlq/:id` | Delete a specific DLQ entry |
| `POST` | `/channels/dlq/replay` | Replay a DLQ message back into its channel |

```bash
# Inspect channels
curl http://localhost:3210/channels

# Replay a failed message
curl -X POST http://localhost:3210/channels/dlq/replay \
  -H "Content-Type: application/json" \
  -d '{"id": "msg-uuid-here"}'
```

### Cache

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/cache` | Cache stats: hit rate, size, oldest/newest entry |

### Actions (call endpoint)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/action` | Call any action directly via HTTP |

```bash
curl -X POST http://localhost:3210/action \
  -H "Content-Type: application/json" \
  -d '{"action": "math.add", "params": {"a": 2, "b": 3}}'
# {"result": 5.0}
```

### Channels publish endpoint

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/channels/send` | Publish a message to a channel |

```bash
curl -X POST http://localhost:3210/channels/send \
  -H "Content-Type: application/json" \
  -d '{"channel": "orders.created", "payload": {"id": "ORD-001"}}'
```

## Authentication

If `token` is set in `AgentConfig`, all requests must include `Authorization: Bearer <token>`. Without a token, all endpoints are open (suitable for local development).

```rust
AgentConfig {
    port:  3210,
    token: Some("my-secret-token".to_string()),
    name:  "prod-node".into(),
}
```

```bash
curl -H "Authorization: Bearer my-secret-token" http://localhost:3210/health
```

## Lab UI

The Lab UI is hosted on GitHub Pages at `/lab/`. Point it to `http://localhost:3210` to connect to your local instance.

It provides:
- **Topology view** — interactive force-directed service call graph
- **Metrics dashboard** — live counters, gauges, histograms
- **Traces explorer** — drill into distributed traces and spans
- **Channels panel** — channel stats, DLQ inspector, message replay
- **Logs stream** — tail recent log entries
- **Action caller** — call any action directly from the browser
