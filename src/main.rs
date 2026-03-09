//! moleculer-rs demo — shows ServiceBroker + Laboratory Agent in action.

use moleculer::{
    BrokerConfig, ServiceBroker, ServiceSchema,
    service::ActionDef,
    service::EventDef,
    middleware::{LoggingMiddleware, MetricsMiddleware},
    config::LogLevel,
};
use moleculer::laboratory::{AgentService, AgentConfig};
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // -------------------------------------------------------------------
    // 1. Configure the broker
    // -------------------------------------------------------------------
    let config = BrokerConfig::new()
        .namespace("demo")
        .node_id("node-main")
        .log_level(LogLevel::Debug)
        .request_timeout(10_000)
        .with_metrics()
        .with_tracing()
        .with_circuit_breaker();

    let broker = ServiceBroker::new(config);

    // -------------------------------------------------------------------
    // 2. Add middlewares
    // -------------------------------------------------------------------
    broker.add_middleware(Arc::new(LoggingMiddleware)).await;
    broker.add_middleware(Arc::new(MetricsMiddleware)).await;

    // -------------------------------------------------------------------
    // 3. Register services
    // -------------------------------------------------------------------

    // math service — mirrors the classic Moleculer demo
    let math_svc = ServiceSchema::new("math")
        .action(ActionDef::new("add", |ctx| async move {
            let a = ctx.params["a"].as_f64().unwrap_or(0.0);
            let b = ctx.params["b"].as_f64().unwrap_or(0.0);
            Ok(json!({ "result": a + b }))
        }))
        .action(ActionDef::new("sub", |ctx| async move {
            let a = ctx.params["a"].as_f64().unwrap_or(0.0);
            let b = ctx.params["b"].as_f64().unwrap_or(0.0);
            Ok(json!({ "result": a - b }))
        }))
        .action(ActionDef::new("mul", |ctx| async move {
            let a = ctx.params["a"].as_f64().unwrap_or(0.0);
            let b = ctx.params["b"].as_f64().unwrap_or(0.0);
            Ok(json!({ "result": a * b }))
        }))
        .on_started(|| async {
            log::info!("math service started ✓");
            Ok(())
        })
        .on_stopped(|| async {
            log::info!("math service stopped");
            Ok(())
        });

    // greeter service
    let greeter_svc = ServiceSchema::new("greeter")
        .action(ActionDef::new("hello", |ctx| async move {
            let name = ctx.params["name"]
                .as_str()
                .unwrap_or("World")
                .to_string();
            Ok(json!({ "message": format!("Hello, {}! — moleculer-rs 🦀", name) }))
        }))
        .event(EventDef::new("user.created", |ctx| async move {
            log::info!("greeter received user.created: {:?}", ctx.params);
            Ok(())
        }));

    broker.add_service(math_svc).await;
    broker.add_service(greeter_svc).await;

    // -------------------------------------------------------------------
    // 4. Start broker
    // -------------------------------------------------------------------
    broker.start().await.unwrap();

    // -------------------------------------------------------------------
    // 5. Smoke-test some calls
    // -------------------------------------------------------------------
    let result = broker.call("math.add", json!({ "a": 5, "b": 3 })).await.unwrap();
    println!("math.add(5, 3) => {}", result);

    let result = broker.call("greeter.hello", json!({ "name": "Moleculer" })).await.unwrap();
    println!("greeter.hello => {}", result);

    broker.emit("user.created", json!({ "id": 42, "name": "Alice" })).await.unwrap();

    // -------------------------------------------------------------------
    // 6. Start the Laboratory Agent
    // -------------------------------------------------------------------
    let agent = Arc::new(AgentService::with_config(
        Arc::clone(&broker),
        AgentConfig {
            port: 3210,
            token: None,
            name: "moleculer-rs demo".to_string(),
        },
    ));

    // Log a few entries so the Lab /logs page has something to show
    agent.logger.log("info",  "Broker started",           Some("broker".into()));
    agent.logger.log("debug", "math.add called",           Some("math".into()));
    agent.logger.log("info",  "greeter.hello called",      Some("greeter".into()));
    agent.logger.log("info",  "user.created event emitted",Some("greeter".into()));

    println!("\n🔬 Laboratory Agent running on http://localhost:3210");
    println!("   Open https://lab.moleculer.services and connect to http://localhost:3210\n");

    // Spawn in background thread (tiny_http is synchronous)
    agent.spawn();

    // Keep running
    tokio::signal::ctrl_c().await.unwrap();
    broker.stop().await.unwrap();
    println!("Broker stopped. Goodbye!");
}
