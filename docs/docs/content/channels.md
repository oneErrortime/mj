# Channels

Durable, reliable messaging — the Rust equivalent of `@moleculer/channels`. Unlike `emit`, channels persist messages until they are explicitly ACK'd.

**Source:** `src/channels/`  
**JS Reference:** `ecosystem/moleculer-channels/`

## Concept

Moleculer's built-in `emit`/`broadcast` are **fire-and-forget**. If a consumer is offline, the event is lost. Channels solve this:

- **Durable storage** — messages persist until the consumer calls `ack()`
- **Consumer groups** — multiple consumers share load; each message goes to exactly one consumer within a group
- **Auto-retry** — `nack()` triggers exponential-backoff retry up to `max_retries`
- **Dead-Letter Queue** — failed messages after max retries route to a configurable DLQ channel

## Architecture

```
Producer
  broker.send_to_channel("orders.created", payload)
            │
            ▼
  ChannelAdapter  (InMemory | Redis* | AMQP* | Kafka*)
            │
            ├──── group "order-processor" ───── consumer A  ← ack()
            │                                   consumer B
            │
            └──── group "audit-log" ────────── consumer C  ← ack()

On nack():
  retry_count < max_retries  →  re-queue with exponential backoff
  retry_count >= max_retries →  move to Dead-Letter Queue
```

`*` = planned, adapter trait is ready

## Message headers

Mirrors `@moleculer/channels` header constants:

| Header | Description |
|--------|-------------|
| `x-redelivered-count` | How many times this message was retried |
| `x-group` | Consumer group name |
| `x-original-channel` | Channel where the error occurred (DLQ entries) |
| `x-original-group` | Group that could not process it |
| `x-error-message` | Error message on DLQ entry |
| `x-error-type` | Error type string |

## Subscribe

```rust
broker.subscribe_channel(
    ChannelDef::new("orders.created", |msg: ChannelMessage| async move {
        println!("Order: {}", msg.payload);

        // Process the message...
        if success {
            msg.ack().await   // remove from queue
        } else {
            msg.nack().await  // retry with backoff
        }
    })
    .group("order-processor")   // consumer group name
    .max_in_flight(5)           // max concurrent messages
    .max_retries(3)             // before DLQ
    .dead_letter("orders.DLQ"), // DLQ channel name
).await.unwrap();
```

## Publish

```rust
// Publish a message
broker.send_to_channel(
    "orders.created",
    json!({ "id": "ORD-001", "amount": 99.99 }),
).await.unwrap();

// With options
broker.send_to_channel_with_options(
    "orders.created",
    json!({ "id": "ORD-001" }),
    SendOptions {
        headers: HashMap::from([
            ("x-source".to_string(), "api-gateway".to_string()),
        ]),
    },
).await.unwrap();
```

## ChannelDef options

| Option | Default | Description |
|--------|---------|-------------|
| `.group(name)` | service name | Consumer group |
| `.max_in_flight(n)` | 1 | Max concurrent in-progress messages per consumer |
| `.max_retries(n)` | 3 | Max NACK retries before DLQ |
| `.dead_letter(channel)` | none | DLQ channel name |
| `.retry_delay_ms(n)` | 1000 | Initial retry delay in ms |
| `.retry_factor(f)` | 2.0 | Exponential backoff factor |

## Dead-Letter Queue

Messages that exhaust all retries are moved to the DLQ. You can inspect and replay them via the Laboratory API:

```bash
# List DLQ entries
curl http://localhost:3210/channels

# Replay a DLQ message
curl -X POST http://localhost:3210/channels/dlq/replay \
  -H "Content-Type: application/json" \
  -d '{"id": "<message-id>"}'

# Delete a DLQ entry
curl -X DELETE http://localhost:3210/channels/dlq/<message-id>
```

## Difference from emit/broadcast

| | emit/broadcast | Channels |
|---|---|---|
| Delivery guarantee | None (fire-and-forget) | At-least-once |
| Message persistence | No | Yes (until ACK) |
| Consumer offline | Message lost | Message waits |
| Retry on failure | No | Yes (configurable) |
| Dead-letter | No | Yes |
| Consumer groups | Yes (event groups) | Yes |

## Adapters

The `Adapter` trait is fully defined. Currently implemented:

| Adapter | Status | Description |
|---------|--------|-------------|
| `InMemoryAdapter` | ✅ Done | Tokio MPSC queues, single-process |
| `RedisAdapter` | 🔲 Planned | Redis Streams (`XADD`/`XREADGROUP`/`XCLAIM`) |
| `AmqpAdapter` | 🔲 Planned | RabbitMQ AMQP 0-9-1 |
| `KafkaAdapter` | 🔲 Planned | Apache Kafka |
| `NatsAdapter` | 🔲 Planned | NATS JetStream |

## JS ecosystem reference

The full `@moleculer/channels` JS implementation is included at `ecosystem/moleculer-channels/`. It supports all adapters in production and serves as the reference for the Rust port.

See [`ecosystem/moleculer-channels/README.md`](https://github.com/oneErrortime/mj/blob/main/ecosystem/moleculer-channels/README.md) for the JS usage guide.
