//! Kafka channel adapter. Enable with feature flag `kafka`.

use crate::channels::{ChannelMessage, AckKind};
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct KafkaOptions {
    pub brokers: Vec<String>,
    pub client_id: String,
    pub group_id: String,
    pub session_timeout_ms: u32,
    pub auto_offset_reset: String,
}

impl Default for KafkaOptions {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".into()],
            client_id: "moleculer-rs".into(),
            group_id: "moleculer-group".into(),
            session_timeout_ms: 30_000,
            auto_offset_reset: "earliest".into(),
        }
    }
}

pub struct KafkaAdapter {
    opts: KafkaOptions,
    #[cfg(feature = "kafka")]
    producer: tokio::sync::RwLock<Option<rdkafka::producer::FutureProducer>>,
    #[cfg(not(feature = "kafka"))]
    _ph: std::marker::PhantomData<()>,
}

impl KafkaAdapter {
    pub fn new(opts: KafkaOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "kafka")]
            producer: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "kafka"))]
            _ph: std::marker::PhantomData,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "kafka")]
        {
            use rdkafka::config::ClientConfig;
            let brokers = self.opts.brokers.join(",");
            let producer = ClientConfig::new()
                .set("bootstrap.servers", &brokers)
                .set("client.id", &self.opts.client_id)
                .create::<rdkafka::producer::FutureProducer>()
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            *self.producer.write().await = Some(producer);
            log::info!("[KafkaAdapter] connected to {}", brokers);
        }
        Ok(())
    }
}

#[cfg(feature = "kafka")]
impl KafkaAdapter {
    pub async fn publish(&self, topic: &str, payload: &Value, key: Option<&str>) -> Result<()> {
        use rdkafka::producer::FutureRecord;
        let guard = self.producer.read().await;
        let producer = guard.as_ref().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;
        let body = serde_json::to_vec(payload).map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        let mut record = FutureRecord::to(topic).payload(&body);
        if let Some(k) = key { record = record.key(k); }
        producer.send(record, std::time::Duration::from_secs(5)).await
            .map_err(|(e, _)| MoleculerError::ChannelAdapter(e.to_string()))?;
        Ok(())
    }

    pub async fn subscribe_topic(
        &self,
        topic: &str,
        handler: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        use rdkafka::config::ClientConfig;
        use rdkafka::consumer::{StreamConsumer, Consumer};
        let brokers = self.opts.brokers.join(",");
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("group.id", &self.opts.group_id)
            .set("auto.offset.reset", &self.opts.auto_offset_reset)
            .set("enable.auto.commit", "false")
            .create()
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        consumer.subscribe(&[topic]).map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        let topic_owned = topic.to_string();
        tokio::spawn(async move {
            use rdkafka::message::Message;
            use futures::StreamExt;
            let mut stream = consumer.stream();
            while let Some(Ok(msg)) = stream.next().await {
                let payload: Value = serde_json::from_slice(msg.payload().unwrap_or_default()).unwrap_or(Value::Null);
                let mut headers = HashMap::new();
                headers.insert("kafka-offset".into(), msg.offset().to_string());
                headers.insert("kafka-partition".into(), msg.partition().to_string());
                let (ack_tx, _) = tokio::sync::oneshot::channel::<AckKind>();
                let cm = ChannelMessage::new(topic_owned.clone(), "default".to_string(),
                    payload, headers, 1, ack_tx);
                let _ = handler(cm).await;
                let _ = consumer.commit_message(&msg, rdkafka::consumer::CommitMode::Async);
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "kafka"))]
impl KafkaAdapter {
    pub async fn publish(&self, _: &str, _: &Value, _: Option<&str>) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("kafka feature not enabled".into()))
    }
    pub async fn subscribe_topic(
        &self, _: &str,
        _: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("kafka feature not enabled".into()))
    }
}
