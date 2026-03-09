//! moleculer-rs — full demo (broker + channels + laboratory).

use moleculer::prelude::*;
use moleculer::channels::{ChannelDef, ChannelMessage};
use moleculer::laboratory::{AgentService, AgentConfig};
use moleculer::middleware::{LoggingMiddleware, MetricsMiddleware};
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env().filter_level(log::LevelFilter::Debug).init();

    // ── 1. Configure ──────────────────────────────────────────────────────────
    let config = BrokerConfig::new()
        .namespace("demo")
        .node_id("node-main")
        .request_timeout(10_000)
        .with_metrics()
        .with_tracing()
        .with_circuit_breaker()
        .with_retry()
        .with_cacher()
        .with_channels()
        .with_laboratory_metrics()
        .with_laboratory_tracing();

    let broker = ServiceBroker::new(config);
    broker.install_default_middlewares().await;

    // ── 2. Services ───────────────────────────────────────────────────────────

    // Mixin: shared logging
    let log_mixin = ServiceSchema::new("__log_mixin")
        .on_started(|| async { log::info!("(mixin started)"); Ok(()) });

    // math service (uses cache, retry, mixin)
    let math = ServiceSchema::new("math")
        .mixin(log_mixin.clone())
        .action(ActionDef::new("add", |ctx| async move {
            let a = ctx.params["a"].as_f64().unwrap_or(0.0);
            let b = ctx.params["b"].as_f64().unwrap_or(0.0);
            Ok(json!({ "result": a + b }))
        }).cache(vec!["a", "b"]))
        .action(ActionDef::new("sub", |ctx| async move {
            let a = ctx.params["a"].as_f64().unwrap_or(0.0);
            let b = ctx.params["b"].as_f64().unwrap_or(0.0);
            Ok(json!({ "result": a - b }))
        }))
        .action(ActionDef::new("fib", |ctx| async move {
            let n = ctx.params["n"].as_u64().unwrap_or(0);
            fn fib(n: u64) -> u64 { if n <= 1 { n } else { fib(n-1) + fib(n-2) } }
            Ok(json!({ "result": fib(n.min(30)) }))
        }).cache(vec!["n"]));

    // greeter service with event listener
    let greeter = ServiceSchema::new("greeter")
        .action(ActionDef::new("hello", |ctx| async move {
            let name = ctx.params["name"].as_str().unwrap_or("World").to_string();
            Ok(json!({ "message": format!("Hello, {}! — moleculer-rs 🦀", name) }))
        }))
        .event(EventDef::new("user.created", |ctx| async move {
            log::info!("[greeter] user.created: {:?}", ctx.params);
            Ok(())
        }))
        .on_started(|| async { log::info!("[greeter] started"); Ok(()) });

    // versioned posts service
    let posts_v2 = ServiceSchema::new("posts")
        .version("2")
        .action(ActionDef::new("list", |ctx| async move {
            Ok(json!([
                { "id": 1, "title": "Hello Moleculer-RS", "author": "alice" },
                { "id": 2, "title": "Rust microservices are blazing fast", "author": "bob" },
            ]))
        }))
        .action(ActionDef::new("get", |ctx| async move {
            let id = ctx.params["id"].as_u64().unwrap_or(0);
            if id == 0 { return Err(MoleculerError::Validation("id required".into())); }
            Ok(json!({ "id": id, "title": format!("Post #{}", id) }))
        }));

    broker.add_service(math).await;
    broker.add_service(greeter).await;
    broker.add_service(posts_v2).await;

    // ── 3. Channels ───────────────────────────────────────────────────────────
    broker.subscribe_channel(
        ChannelDef::new("orders.created", |msg: ChannelMessage| async move {
            log::info!("[orders] Processing order: {}", msg.payload);
            // Simulate processing
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            msg.ack().await
        })
        .group("order-processor")
        .max_in_flight(5)
        .max_retries(3)
        .dead_letter("orders.DLQ"),
    ).await.unwrap();

    broker.subscribe_channel(
        ChannelDef::new("notifications.email", |msg: ChannelMessage| async move {
            log::info!("[notifications] Sending email for: {}", msg.payload["to"].as_str().unwrap_or("?"));
            msg.ack().await
        })
        .group("email-sender")
        .max_in_flight(10),
    ).await.unwrap();

    // ── 4. Start ──────────────────────────────────────────────────────────────
    broker.start().await.unwrap();

    // ── 5. Smoke tests ────────────────────────────────────────────────────────
    let r = broker.call("math.add", json!({ "a": 5, "b": 3 })).await.unwrap();
    println!("math.add(5+3)      = {}", r["result"]);

    let r = broker.call("math.add", json!({ "a": 5, "b": 3 })).await.unwrap(); // cache hit
    println!("math.add(5+3) [↑$] = {}", r["result"]);

    let r = broker.call("greeter.hello", json!({ "name": "Moleculer" })).await.unwrap();
    println!("greeter.hello      = {}", r["message"]);

    let r = broker.call("v2.posts.list", json!({})).await.unwrap();
    println!("v2.posts.list      = {} posts", r.as_array().map_or(0, |a| a.len()));

    broker.emit("user.created", json!({ "id": 42, "name": "Alice" })).await.unwrap();

    // Publish to channels
    broker.send_to_channel("orders.created",   json!({ "id": "ORD-001", "total": 99.99 })).await.unwrap();
    broker.send_to_channel("orders.created",   json!({ "id": "ORD-002", "total": 14.50 })).await.unwrap();
    broker.send_to_channel("notifications.email", json!({ "to": "alice@example.com", "subject": "Order confirmed" })).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // let channels process

    println!("\nCache entries: {}", broker.cacher.as_ref().map_or(0, |c| c.len()));
    let topo = broker.registry.topology_snapshot();
    println!("Topology edges: {}", topo.len());

    // ── 6. Laboratory Agent ───────────────────────────────────────────────────
    let agent = Arc::new(AgentService::with_config(
        Arc::clone(&broker),
        AgentConfig { port: 3210, token: None, name: "moleculer-rs demo".into() },
    ));

    agent.logger.log("info",  "Broker started",              Some("broker".into()));
    agent.logger.log("info",  "math, greeter, v2.posts ready", Some("registry".into()));
    agent.logger.log("info",  "Channels: orders, notifications", Some("channels".into()));

    println!("\n🔬 Laboratory Agent → http://localhost:3210");
    println!("   Endpoints: /health /info /services /topology /metrics /traces /logs /channels /cache");
    println!("   Connect lab frontend: https://lab.moleculer.services → http://localhost:3210\n");

    agent.spawn();

    tokio::signal::ctrl_c().await.unwrap();
    println!("\nShutting down...");
    broker.stop().await.unwrap();
}
