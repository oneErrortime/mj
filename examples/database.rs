//! Example: @moleculer/database — CRUD service in Rust.
//!
//! Run: `cargo run --example database`

use moleculer::prelude::*;
use moleculer::database::{DatabaseMixin, MemoryAdapter, DatabaseOptions, database_service};
use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let broker = ServiceBroker::new(
        BrokerConfig::default()
            .with_metrics()
            .with_tracing()
    );

    broker.install_default_middlewares().await;

    // ── Register a CRUD service using DatabaseMixin ──
    let users_service = DatabaseMixin::with_options(
        MemoryAdapter::new(),
        DatabaseOptions {
            default_page_size: 10,
            cache_enabled: true,
            ..Default::default()
        },
    ).apply(ServiceSchema::new("users"));

    broker.add_service(users_service).await;

    // Also use the convenience function
    let posts_service = database_service("posts", MemoryAdapter::new());
    broker.add_service(posts_service).await;

    broker.start().await.unwrap();

    // Create some users
    let alice = broker.call("users.create", json!({
        "name": "Alice",
        "email": "alice@example.com",
        "role": "admin"
    })).await.unwrap();
    println!("Created: {}", alice);

    let bob = broker.call("users.create", json!({
        "name": "Bob",
        "email": "bob@example.com",
        "role": "user"
    })).await.unwrap();
    println!("Created: {}", bob);

    // List users (paginated)
    let list = broker.call("users.list", json!({
        "page": 1,
        "pageSize": 10,
    })).await.unwrap();
    println!("List: {}", serde_json::to_string_pretty(&list).unwrap());

    // Find with filter
    let admins = broker.call("users.find", json!({
        "query": { "role": "admin" }
    })).await.unwrap();
    println!("Admins: {}", admins);

    // Count
    let total = broker.call("users.count", json!({})).await.unwrap();
    println!("Total users: {}", total);

    // Get by ID
    let user_id = alice["id"].as_str().unwrap();
    let user = broker.call("users.get", json!({ "id": user_id })).await.unwrap();
    println!("Get user: {}", user);

    // Update
    let updated = broker.call("users.update", json!({
        "id": user_id,
        "role": "superadmin"
    })).await.unwrap();
    println!("Updated: {}", updated);

    // Remove
    let removed = broker.call("users.remove", json!({ "id": user_id })).await.unwrap();
    println!("Removed: {}", removed);

    // Final count
    let final_count = broker.call("users.count", json!({})).await.unwrap();
    println!("Remaining users: {}", final_count);

    broker.stop().await.unwrap();
}
