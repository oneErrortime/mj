//! AMQP channel adapter (RabbitMQ / any AMQP 0-9-1 broker).
//!
//! Mirrors `@moleculer/channels` src/adapters/amqp.js.
//!
//! Enable with feature flag `amqp`.

use crate::channels::{ChannelDef, ChannelMessage, AckKind};
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct AmqpOptions {
    pub url: String,
    /// Prefetch (QoS) — max messages in-flight per consumer.
    pub prefetch_count: u16,
    pub durable: bool,
    pub auto_delete: bool,
    pub mandatory: bool,
    pub exchange_type: String,
    pub message_ttl: Option<u32>,
}

impl Default for AmqpOptions {
    fn default() -> Self {
        Self {
            url: "amqp://guest:guest@localhost:5672".into(),
            prefetch_count: 10,
            durable: true,
            auto_delete: false,
            mandatory: false,
            exchange_type: "direct".into(),
            message_ttl: None,
        }
    }
}

pub struct AmqpAdapter {
    opts: AmqpOptions,
    #[cfg(feature = "amqp")]
    channel: tokio::sync::RwLock<Option<lapin::Channel>>,
    #[cfg(not(feature = "amqp"))]
    _ph: std::marker::PhantomData<()>,
}

impl AmqpAdapter {
    pub fn new(opts: AmqpOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "amqp")]
            channel: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "amqp"))]
            _ph: std::marker::PhantomData,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "amqp")]
        {
            use lapin::{Connection, ConnectionProperties};
            let conn = Connection::connect(&self.opts.url, ConnectionProperties::default()).await
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            let ch = conn.create_channel().await
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            *self.channel.write().await = Some(ch);
            log::info!("[AmqpAdapter] connected to {}", self.opts.url);
        }
        Ok(())
    }
}

#[cfg(feature = "amqp")]
impl AmqpAdapter {
    pub async fn publish(&self, queue: &str, payload: &Value, headers: HashMap<String, String>) -> Result<()> {
        use lapin::{BasicProperties, options::*};
        use lapin::types::{AMQPValue, FieldTable, LongString, ShortString};

        let guard = self.channel.read().await;
        let ch = guard.as_ref().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;

        let body = serde_json::to_vec(payload)
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        let mut header_table = FieldTable::default();
        for (k, v) in &headers {
            header_table.insert(
                ShortString::from(k.as_str()),
                AMQPValue::LongString(LongString::from(v.as_bytes())),
            );
        }

        ch.basic_publish(
            "",
            queue,
            BasicPublishOptions::default(),
            &body,
            BasicProperties::default()
                .with_content_type(ShortString::from("application/json"))
                .with_headers(header_table)
                .with_delivery_mode(2), // persistent
        ).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?
            .await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        Ok(())
    }

    pub async fn subscribe_queue(
        &self,
        queue: &str,
        prefetch: u16,
        handler: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        use lapin::{options::*, types::FieldTable};

        let guard = self.channel.read().await;
        let ch = guard.as_ref().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;

        ch.queue_declare(queue, QueueDeclareOptions {
            durable: self.opts.durable,
            auto_delete: self.opts.auto_delete,
            ..Default::default()
        }, FieldTable::default()).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        ch.basic_qos(prefetch, BasicQosOptions::default()).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        let mut consumer = ch.basic_consume(
            queue,
            &format!("moleculer-consumer-{}", uuid::Uuid::new_v4()),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        ).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        let queue_name = queue.to_string();
        tokio::spawn(async move {
            use futures::StreamExt;
            while let Some(delivery) = consumer.next().await {
                match delivery {
                    Ok(d) => {
                        let payload: Value = serde_json::from_slice(&d.data).unwrap_or(Value::Null);
                        let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel::<AckKind>();
                        let msg = ChannelMessage {
                            id: d.delivery_tag.to_string(),
                            channel: queue_name.clone(),
                            group: None,
                            payload,
                            headers: HashMap::new(),
                            retry_count: 0,
                            ack_sender: Some(ack_tx),
                        };
                        let result = handler(msg).await;
                        match result {
                            Ok(_) => { let _ = d.acker.ack(Default::default()).await; }
                            Err(e) => {
                                log::warn!("[AmqpAdapter] handler error, NACKing: {}", e);
                                let _ = d.acker.nack(lapin::options::BasicNackOptions { requeue: true, ..Default::default() }).await;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[AmqpAdapter] delivery error: {}", e);
                        break;
                    }
                }
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "amqp"))]
impl AmqpAdapter {
    pub async fn publish(&self, _: &str, _: &Value, _: HashMap<String, String>) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("amqp feature not enabled".into()))
    }
    pub async fn subscribe_queue(
        &self, _: &str, _: u16,
        _: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("amqp feature not enabled".into()))
    }
}
