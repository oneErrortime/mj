//! Redis Streams channel adapter.
//!
//! Mirrors `@moleculer/channels` src/adapters/redis.js.
//!
//! Uses Redis Streams (XADD / XREADGROUP / XACK) for durable, consumer-group
//! aware message delivery. Enable with feature flag `redis`.

use crate::channels::{ChannelMessage, AckKind};
use crate::error::{MoleculerError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct RedisStreamOptions {
    pub url: String,
    pub read_timeout_ms: u64,
    pub min_idle_time_ms: u64,
    pub claim_interval_ms: u64,
    pub start_id: String,
}

impl Default for RedisStreamOptions {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".into(),
            read_timeout_ms: 0,
            min_idle_time_ms: 3_600_000,
            claim_interval_ms: 100,
            start_id: "$".into(),
        }
    }
}

pub struct RedisStreamAdapter {
    opts: RedisStreamOptions,
    #[cfg(feature = "redis")]
    client: tokio::sync::RwLock<Option<redis::aio::ConnectionManager>>,
    #[cfg(not(feature = "redis"))]
    _ph: std::marker::PhantomData<()>,
}

impl RedisStreamAdapter {
    pub fn new(opts: RedisStreamOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "redis")]
            client: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "redis"))]
            _ph: std::marker::PhantomData,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        #[cfg(feature = "redis")]
        {
            let client = redis::Client::open(self.opts.url.as_str())
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            let mgr = redis::aio::ConnectionManager::new(client).await
                .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
            *self.client.write().await = Some(mgr);
            log::info!("[RedisStreamAdapter] connected to {}", self.opts.url);
        }
        Ok(())
    }
}

#[cfg(feature = "redis")]
impl RedisStreamAdapter {
    async fn ensure_group(&self, stream: &str, group: &str) -> Result<()> {
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;
        let _: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE").arg(stream).arg(group).arg(&self.opts.start_id).arg("MKSTREAM")
            .query_async(conn).await;
        Ok(())
    }

    pub async fn publish(&self, channel: &str, payload: &Value, headers: HashMap<String, String>) -> Result<String> {
        use redis::AsyncCommands;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::ChannelAdapter("not connected".into()))?;
        let body = serde_json::to_string(payload)
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        let mut cmd = redis::cmd("XADD");
        cmd.arg(channel).arg("*").arg("payload").arg(&body);
        for (k, v) in &headers { cmd.arg(k).arg(v); }
        let id: String = cmd.query_async(conn).await
            .map_err(|e| MoleculerError::ChannelAdapter(e.to_string()))?;
        Ok(id)
    }

    pub async fn subscribe_stream(
        &self,
        channel: &str,
        group: &str,
        consumer: &str,
        max_in_flight: usize,
        handler: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        self.ensure_group(channel, group).await?;
        let url = self.opts.url.clone();
        let channel = channel.to_string();
        let group = group.to_string();
        let consumer = consumer.to_string();
        let timeout = self.opts.read_timeout_ms;

        tokio::spawn(async move {
            let client = match redis::Client::open(url.as_str()) {
                Ok(c) => c,
                Err(e) => { log::error!("[RedisStreamAdapter] {}", e); return; }
            };
            let mut conn = match client.get_async_connection().await {
                Ok(c) => c,
                Err(e) => { log::error!("[RedisStreamAdapter] {}", e); return; }
            };
            loop {
                let result: redis::RedisResult<Vec<(String, Vec<(String, HashMap<String, String>)>)>> =
                    redis::cmd("XREADGROUP")
                        .arg("GROUP").arg(&group).arg(&consumer)
                        .arg("COUNT").arg(max_in_flight)
                        .arg("BLOCK").arg(timeout)
                        .arg("STREAMS").arg(&channel).arg(">")
                        .query_async(&mut conn).await;
                match result {
                    Err(e) => {
                        log::warn!("[RedisStreamAdapter] XREADGROUP: {}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                    Ok(streams) => {
                        for (_stream, entries) in streams {
                            for (msg_id, fields) in entries {
                                let payload: Value = fields.get("payload")
                                    .and_then(|s| serde_json::from_str(s).ok())
                                    .unwrap_or(Value::Null);
                                let delivery_count: u32 = fields.get("x-redelivered-count")
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0) + 1;
                                let (ack_tx, _) = tokio::sync::oneshot::channel::<AckKind>();
                                let msg = ChannelMessage::new(
                                    channel.clone(), group.clone(),
                                    payload, fields, delivery_count, ack_tx,
                                );
                                let _ = handler(msg).await;
                                let _: redis::RedisResult<()> = redis::cmd("XACK")
                                    .arg(&channel).arg(&group).arg(&msg_id)
                                    .query_async(&mut conn).await;
                            }
                        }
                    }
                }
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "redis"))]
impl RedisStreamAdapter {
    pub async fn publish(&self, _: &str, _: &Value, _: HashMap<String, String>) -> Result<String> {
        Err(MoleculerError::ChannelAdapter("redis feature not enabled".into()))
    }
    pub async fn subscribe_stream(
        &self, _: &str, _: &str, _: &str, _: usize,
        _: Arc<dyn Fn(ChannelMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Result<()> {
        Err(MoleculerError::ChannelAdapter("redis feature not enabled".into()))
    }
}
