//! Channels example — durable queues with DLQ and consumer groups.

use moleculer::prelude::*;
use moleculer::channels::{ChannelDef, ChannelMessage};
use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::init();

    let broker = ServiceBroker::new(BrokerConfig::default().with_channels());

    // Consumer group 1: email processor
    broker.subscribe_channel(
        ChannelDef::new("notifications", |msg: ChannelMessage| async move {
            println!("[email] Processing: {}", msg.payload);
            msg.ack().await
        }).group("email").max_in_flight(3).max_retries(2).dead_letter("notifications.DLQ"),
    ).await.unwrap();

    // Consumer group 2: sms processor (independent group — also receives all msgs)
    broker.subscribe_channel(
        ChannelDef::new("notifications", |msg: ChannelMessage| async move {
            println!("[sms] Processing: {}", msg.payload);
            msg.ack().await
        }).group("sms").max_in_flight(2),
    ).await.unwrap();

    broker.start().await.unwrap();

    for i in 0..5 {
        broker.send_to_channel("notifications", json!({ "id": i, "text": format!("Message {}", i) })).await.unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    println!("Channel stats: {:?}", broker.channel_stats());
    broker.stop().await.unwrap();
}
