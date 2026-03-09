//! Full moleculer-rs demo — broker, services, channels, database, workflows.
//!
//! Run:  cargo run --example full_demo --features database
//!
//! Shows:
//! - ServiceBroker with metrics + tracing
//! - Custom actions with before/after hooks
//! - Database CRUD via DatabaseMixin
//! - Channels (in-memory durable queue)
//! - Workflow (multi-step pipeline)
//! - $node internal service queries
//! - Graceful shutdown

use moleculer::prelude::*;
use moleculer::middleware::{ActionHookMiddleware, ValidatorMiddleware};
use serde_json::json;

#[cfg(feature = "database")]
use moleculer::database::{DatabaseMixin, NeDbAdapter, DatabaseOptions};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    // ── Broker ────────────────────────────────────────────────────────────────
    let broker = ServiceBroker::new(
        BrokerConfig::default()
            .node_id("demo-node-1")
            .namespace("production")
            .with_metrics()
            .with_tracing()
            .with_circuit_breaker()
            .with_retry()
            .with_channels()
    );

    // ── Middlewares ───────────────────────────────────────────────────────────
    broker.install_default_middlewares().await;

    broker.add_middleware(std::sync::Arc::new(
        ActionHookMiddleware::new()
            .before_all(|ctx| async move {
                log::info!("[Hook] → {}", ctx.action.as_deref().unwrap_or("?"));
                Ok(ctx)
            })
            .after_all(|ctx, result| async move {
                log::info!("[Hook] ← {} ok", ctx.action.as_deref().unwrap_or("?"));
                Ok(result)
            })
    )).await;

    // ── Math service ──────────────────────────────────────────────────────────
    broker.add_service(
        ServiceSchema::new("math")
            .action(ActionDef::new("add", |ctx| async move {
                let a = ctx.params["a"].as_f64().unwrap_or(0.0);
                let b = ctx.params["b"].as_f64().unwrap_or(0.0);
                Ok(json!({ "result": a + b }))
            }).cache(vec!["a", "b"]))
            .action(ActionDef::new("multiply", |ctx| async move {
                let a = ctx.params["a"].as_f64().unwrap_or(0.0);
                let b = ctx.params["b"].as_f64().unwrap_or(0.0);
                Ok(json!({ "result": a * b }))
            }))
    ).await;

    // ── Greeter service ───────────────────────────────────────────────────────
    broker.add_service(
        ServiceSchema::new("greeter")
            .action(ActionDef::new("hello", |ctx| async move {
                let name = ctx.params["name"].as_str().unwrap_or("World");
                Ok(json!({ "message": format!("Hello, {}!", name) }))
            }))
            .on_event(EventDef::new("user.created", |ctx| async move {
                log::info!("New user: {}", ctx.params["name"].as_str().unwrap_or("?"));
                Ok(())
            }))
    ).await;

    // ── Users service with DatabaseMixin ──────────────────────────────────────
    #[cfg(feature = "database")]
    {
        let users = DatabaseMixin::with_options(NeDbAdapter::new(), DatabaseOptions {
                soft_delete: false,
                cache_enabled: false,
                default_page_size: 10,
                max_page_size: 100,
                id_field: "id".into(),
                rest: false,
            })
            .apply(ServiceSchema::new("users"));
        broker.add_service(users).await;
    }

    // ── Channels ──────────────────────────────────────────────────────────────
    broker.subscribe_channel(
        ChannelDef::new("orders.created", |msg: ChannelMessage| async move {
            log::info!("[Channel] order received: {}", msg.payload["id"].as_str().unwrap_or("?"));
            msg.ack().await
        })
        .group("order-processor")
        .max_in_flight(5)
        .max_retries(3),
    ).await?;

    // ── Start ─────────────────────────────────────────────────────────────────
    broker.start().await?;

    // ── Calls ─────────────────────────────────────────────────────────────────
    let sum = broker.call("math.add", json!({ "a": 10, "b": 32 })).await?;
    println!("math.add: {}", sum["result"]);

    let sum2 = broker.call("math.add", json!({ "a": 10, "b": 32 })).await?; // cached
    println!("math.add (cached): {}", sum2["result"]);

    let hi = broker.call("greeter.hello", json!({ "name": "moleculer-rs" })).await?;
    println!("greeter.hello: {}", hi["message"]);

    // ── Emit event ────────────────────────────────────────────────────────────
    broker.emit("user.created", json!({ "name": "Alice", "email": "alice@example.com" })).await?;

    // ── Channel publish ───────────────────────────────────────────────────────
    broker.send_to_channel("orders.created", json!({ "id": "ORD-001", "total": 99.99 })).await?;

    // ── Database CRUD ─────────────────────────────────────────────────────────
    #[cfg(feature = "database")]
    {
        let alice = broker.call("users.create", json!({
            "name": "Alice", "email": "alice@example.com", "role": "admin"
        })).await?;
        println!("created user: id={}", alice["id"]);

        let list = broker.call("users.list", json!({ "page": 1, "pageSize": 10 })).await?;
        println!("users.list: {} total", list["total"]);

        let found = broker.call("users.find", json!({ "query": { "role": "admin" } })).await?;
        println!("users.find (admin): {} results", found.as_array().map(|a| a.len()).unwrap_or(0));

        if let Some(id) = alice["id"].as_str() {
            let updated = broker.call("users.update", json!({ "id": id, "role": "superadmin" })).await?;
            println!("updated user role: {}", updated["role"]);

            broker.call("users.remove", json!({ "id": id })).await?;
            println!("user removed");
        }
    }

    // ── Workflow ──────────────────────────────────────────────────────────────
    let wf = Workflow::builder("checkout")
        .step(Step::call("math.add")
            .params(|ctx| ctx.params.clone()))
        .step(Step::call("math.multiply")
            .depends_on("math_add")
            .params(|ctx| {
                let prev = ctx.outputs.get("math_add")
                    .and_then(|v| v["result"].as_f64())
                    .unwrap_or(0.0);
                json!({ "a": prev, "b": 2.0 })
            })
            .output_as("doubled"))
        .build();

    let wf_result = wf.run(&broker, json!({ "a": 5.0, "b": 3.0 })).await?;
    println!("workflow 'checkout': doubled={}", 
        wf_result.outputs.get("doubled").and_then(|v| v["result"].as_f64()).unwrap_or(0.0));

    // ── $node queries ─────────────────────────────────────────────────────────
    let services = broker.call("$node.services", json!({ "withActions": true })).await?;
    println!("$node.services: {} services registered", 
        services.as_array().map(|a| a.len()).unwrap_or(0));

    let health = broker.call("$node.health", json!({})).await?;
    println!("$node.health: cpu_cores={}", health["cpu"]["cores"]);

    // ── Ping ──────────────────────────────────────────────────────────────────
    let latency = broker.ping().await;
    println!("ping latency: {}μs", latency);

    // ── Shutdown ──────────────────────────────────────────────────────────────
    broker.stop().await?;
    println!("✓ Done.");
    Ok(())
}
