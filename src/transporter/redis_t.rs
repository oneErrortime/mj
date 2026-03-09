//! Redis Transporter — uses Redis pub/sub for message distribution.
//!
//! Mirrors `moleculer/src/transporters/redis.js`.
//!
//! Enable with feature flag `redis` in Cargo.toml.

use super::{Packet, PacketKind, Transporter};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;

pub struct RedisOptions {
    pub url: String,
    pub namespace: String,
    pub prefix: String,
}

impl Default for RedisOptions {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".into(),
            namespace: String::new(),
            prefix: "MOL".into(),
        }
    }
}

pub struct RedisTransporter {
    opts: RedisOptions,
    #[cfg(feature = "redis")]
    client: tokio::sync::RwLock<Option<redis::aio::ConnectionManager>>,
    #[cfg(not(feature = "redis"))]
    _ph: std::marker::PhantomData<()>,
}

impl RedisTransporter {
    pub fn new(opts: RedisOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "redis")]
            client: tokio::sync::RwLock::new(None),
            #[cfg(not(feature = "redis"))]
            _ph: std::marker::PhantomData,
        }
    }

    fn channel_name(&self, kind: &PacketKind, target: Option<&str>) -> String {
        let prefix = if self.opts.namespace.is_empty() {
            self.opts.prefix.clone()
        } else {
            format!("{}-{}", self.opts.prefix, self.opts.namespace)
        };
        let kind_str = match kind {
            PacketKind::Discover   => "DISCOVER",
            PacketKind::Info       => "INFO",
            PacketKind::Heartbeat  => "HEARTBEAT",
            PacketKind::Request    => "REQUEST",
            PacketKind::Response   => "RESPONSE",
            PacketKind::Event      => "EVENT",
            PacketKind::Broadcast  => "BROADCAST",
            PacketKind::Ping       => "PING",
            PacketKind::Pong       => "PONG",
            PacketKind::Disconnect => "DISCONNECT",
        };
        match target {
            Some(t) => format!("{}.{}.{}", prefix, kind_str, t),
            None    => format!("{}.{}", prefix, kind_str),
        }
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl Transporter for RedisTransporter {
    async fn connect(&self) -> Result<()> {
        let client = redis::Client::open(self.opts.url.as_str())
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        let mgr = redis::aio::ConnectionManager::new(client).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        *self.client.write().await = Some(mgr);
        log::info!("[RedisTransporter] connected to {}", self.opts.url);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        *self.client.write().await = None;
        Ok(())
    }

    async fn send(&self, packet: Packet) -> Result<()> {
        use redis::AsyncCommands;
        let channel = self.channel_name(&packet.kind, packet.target.as_deref());
        let payload = serde_json::to_string(&packet)
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        let mut guard = self.client.write().await;
        let conn = guard.as_mut().ok_or_else(|| MoleculerError::Transport("Redis not connected".into()))?;
        let _: () = conn.publish(channel, payload).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static {
        let client = redis::Client::open(self.opts.url.as_str())
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        let conn = client.get_async_connection().await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        let mut pubsub = conn.into_pubsub();
        let prefix = if self.opts.namespace.is_empty() {
            self.opts.prefix.clone()
        } else {
            format!("{}-{}", self.opts.prefix, self.opts.namespace)
        };
        pubsub.psubscribe(format!("{}.*", prefix)).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let payload: String = msg.get_payload().unwrap_or_default();
                if let Ok(pkt) = serde_json::from_str::<Packet>(&payload) {
                    handler(pkt);
                }
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "redis"))]
#[async_trait]
impl Transporter for RedisTransporter {
    async fn connect(&self) -> Result<()> {
        Err(MoleculerError::Transport("Redis feature not enabled. Add `redis` to features.".into()))
    }
    async fn disconnect(&self) -> Result<()> { Ok(()) }
    async fn send(&self, _: Packet) -> Result<()> {
        Err(MoleculerError::Transport("Redis feature not enabled".into()))
    }
    async fn subscribe<F>(&self, _: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static {
        Err(MoleculerError::Transport("Redis feature not enabled".into()))
    }
}
