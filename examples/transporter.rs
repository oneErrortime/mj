//! Example: Transporters — TCP, NATS, Redis.
use moleculer::prelude::*;
use moleculer::transporter::{TcpTransporter, TcpOptions};
use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Info).init();

    let broker = ServiceBroker::new(BrokerConfig::default().node_id("demo-node-1").with_metrics());
    broker.install_default_middlewares().await;

    broker.add_service(
        ServiceSchema::new("greeter")
            .action(ActionDef::new("hello", |ctx| async move {
                let name = ctx.params["name"].as_str().unwrap_or("World");
                Ok(json!({ "message": format!("Hello, {}!", name) }))
            }))
    ).await;

    broker.start().await.unwrap();

    let result = broker.call("greeter.hello", json!({ "name": "moleculer-rs" })).await.unwrap();
    println!("Result: {}", result);

    println!("\nAvailable transporter specs:");
    println!("  (none/empty)           — in-process LocalTransporter (default)");
    println!("  nats://localhost:4222  — NATS (requires 'nats' feature)");
    println!("  redis://localhost:6379 — Redis (requires 'redis' feature)");
    println!("  tcp                   — TCP/UDP Gossip P2P");

    // Show TCP transporter can be constructed
    let _tcp = TcpTransporter::new("node-1", TcpOptions {
        port: 7869,
        udp_discovery: true,
        gossip_period: 2,
        ..TcpOptions::default()
    });
    println!("\nTCP Transporter instantiated OK");

    broker.stop().await.unwrap();
    println!("Done.");
}
