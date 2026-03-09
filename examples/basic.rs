//! Basic example — two services communicating via a broker.

use moleculer::{BrokerConfig, ServiceBroker, ServiceSchema, service::ActionDef};
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    env_logger::init();

    let broker = ServiceBroker::new(BrokerConfig::default());

    broker.add_service(
        ServiceSchema::new("posts")
            .action(ActionDef::new("list", |_ctx| async move {
                Ok(json!([
                    { "id": 1, "title": "Hello Moleculer-RS" },
                    { "id": 2, "title": "Rust microservices are fast!" },
                ]))
            }))
    ).await;

    broker.start().await.unwrap();

    let posts = broker.call("posts.list", json!({})).await.unwrap();
    println!("Posts: {}", serde_json::to_string_pretty(&posts).unwrap());

    broker.stop().await.unwrap();
}
