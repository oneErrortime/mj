//! NATS JetStream channel adapter. Enable with feature flag `nats`.

use crate::channels::{ChannelMessage, AckKind};
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct NatsJetStreamOptions {
    pub url: String,
    pub stream_name: String,
    pub ack_wait_secs: u64,
}

impl Default for NatsJetStreamOptions {
    fn default() -> Self {
        Self { url: "nats://localhost:4222".into(), stream_name: "MOLECULER".into(), ack_wait_secs: 30 }
    }
}

pub struct NatsJetStreamAdapter {
    opts: NatsJetStreamOptions,
    #[cfg(feature = "nats")]
    client: tokio::sync::RwLock<Option<async_nats::Client>>,
    #[cfg(not(feature = "nats"))]
    _ph: std::marker::PhantomData<()>,
}

impl NatsJetStreamAdapter {
    pub fn new(opts: NatsJetStreamOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "nats")]
            client: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "nats"))]
            _ph: std::marker::PhantomData,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "nats")]
        {
            let client = async_nats::connect(&self.opts.url).await
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            *self.client.write().await = Some(client);
            log::info!("[NatsJetStreamAdapter] connected to {}", self.opts.url);
        }
        Ok(())
    }
}

#[cfg(feature = "nats")]
impl NatsJetStreamAdapter {
    pub async fn publish(&self, subject: &str, payload: &Value) -> Result<()> {
        let guard = self.client.read().await;
        let client = guard.as_ref().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;
        let js = async_nats::jetstream::new(client.clone());
        let body = serde_json::to_vec(payload).map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        js.publish(subject.to_string(), bytes::Bytes::from(body)).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?
            .await.map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        Ok(())
    }

    pub async fn subscribe_stream(
        &self,
        subject: &str,
        durable_name: &str,
        handler: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let client = guard.as_ref().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;
        let js = async_nats::jetstream::new(client.clone());
        let stream_name = self.opts.stream_name.clone();
        let ack_wait = self.opts.ack_wait_secs;

        let _ = js.get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.clone(),
            subjects: vec![format!("{}.>", subject.split('.').next().unwrap_or(subject))],
            ..Default::default()
        }).await.map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        let stream = js.get_stream(&stream_name).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        let consumer = stream.get_or_create_consumer(durable_name, async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(durable_name.to_string()),
            filter_subject: subject.to_string(),
            ack_wait: std::time::Duration::from_secs(ack_wait),
            ..Default::default()
        }).await.map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;

        let subject_owned = subject.to_string();
        tokio::spawn(async move {
            loop {
                match consumer.fetch().max_messages(10).messages().await {
                    Err(e) => { log::error!("[NatsJetStreamAdapter] {}", e); tokio::time::sleep(std::time::Duration::from_secs(1)).await; }
                    Ok(mut messages) => {
                        use futures::StreamExt;
                        while let Some(Ok(msg)) = messages.next().await {
                            let payload: Value = serde_json::from_slice(&msg.payload).unwrap_or(Value::Null);
                            let delivery_count = msg.info().map(|i| i.num_delivered as u32).unwrap_or(1);
                            let (ack_tx, _) = tokio::sync::oneshot::channel::<AckKind>();
                            let cm = ChannelMessage::new(
                                subject_owned.clone(), "default".to_string(),
                                payload, HashMap::new(), delivery_count, ack_tx,
                            );
                            let result = handler(cm).await;
                            if result.is_ok() { let _ = msg.ack().await; } else { let _ = msg.nak().await; }
                        }
                    }
                }
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "nats"))]
impl NatsJetStreamAdapter {
    pub async fn publish(&self, _: &str, _: &Value) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("nats feature not enabled".into()))
    }
    pub async fn subscribe_stream(
        &self, _: &str, _: &str,
        _: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("nats feature not enabled".into()))
    }
}
