//! NATS Transporter — wraps `async-nats` for Moleculer message passing.
//!
//! Mirrors `moleculer/src/transporters/nats.js`.
//!
//! Enable with feature flag `nats` in Cargo.toml.
//!
//! ## Connection string formats
//! - `nats://localhost:4222`
//! - `nats://user:pass@localhost:4222`
//! - `tls://hostname:4222`

use super::{Packet, PacketKind, Transporter};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "nats")]
use async_nats::{Client, ConnectOptions, Message};

pub struct NatsOptions {
    pub url: String,
    pub namespace: String,
    pub prefix: String,
    /// Reconnect max attempts (0 = infinite).
    pub max_reconnect_attempts: u32,
    pub reconnect_wait: u64,
    pub tls: bool,
}

impl Default for NatsOptions {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".into(),
            namespace: String::new(),
            prefix: "MOL".into(),
            max_reconnect_attempts: 0,
            reconnect_wait: 2000,
            tls: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

pub struct NatsTransporter {
    opts: NatsOptions,
    #[cfg(feature = "nats")]
    client: RwLock<Option<Client>>,
    #[cfg(not(feature = "nats"))]
    _phantom: std::marker::PhantomData<()>,
}

impl NatsTransporter {
    pub fn new(opts: NatsOptions) -> Self {
        Self {
            opts,
            #[cfg(feature = "nats")]
            client: RwLock::new(None),
            #[cfg(not(feature = "nats"))]
            _phantom: std::marker::PhantomData,
        }
    }

    fn topic(&self, kind: &PacketKind, target: Option<&str>) -> String {
        let prefix = if self.opts.namespace.is_empty() {
            self.opts.prefix.clone()
        } else {
            format!("{}-{}", self.opts.prefix, self.opts.namespace)
        };

        let suffix = match kind {
            PacketKind::Discover => "DISCOVER",
            PacketKind::Info => "INFO",
            PacketKind::Heartbeat => "HEARTBEAT",
            PacketKind::Request => "REQUEST",
            PacketKind::Response => "RESPONSE",
            PacketKind::Event => "EVENT",
            PacketKind::Broadcast => "BROADCAST",
            PacketKind::Ping => "PING",
            PacketKind::Pong => "PONG",
            PacketKind::Disconnect => "DISCONNECT",
        };

        match target {
            Some(t) => format!("{}.{}.{}", prefix, suffix, t),
            None => format!("{}.{}", prefix, suffix),
        }
    }
}

// ─── Feature-gated impl ──────────────────────────────────────────────────────

#[cfg(feature = "nats")]
#[async_trait]
impl Transporter for NatsTransporter {
    async fn connect(&self) -> Result<()> {
        let opts = ConnectOptions::new();
        let client = async_nats::connect_with_options(&self.opts.url, opts)
            .await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        *self.client.write().await = Some(client);
        log::info!("[NatsTransporter] connected to {}", self.opts.url);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        if let Some(client) = self.client.write().await.take() {
            let _ = client.flush().await;
        }
        log::info!("[NatsTransporter] disconnected");
        Ok(())
    }

    async fn send(&self, packet: Packet) -> Result<()> {
        let guard = self.client.read().await;
        let client = guard.as_ref().ok_or_else(|| MoleculerError::Transport("NATS not connected".into()))?;
        let topic = self.topic(&packet.kind, packet.target.as_deref());
        let payload = serde_json::to_vec(&packet)
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        client.publish(topic, payload.into()).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static {
        let guard = self.client.read().await;
        let client = guard.as_ref().ok_or_else(|| MoleculerError::Transport("NATS not connected".into()))?;

        // Subscribe to wildcard topics
        let prefix = if self.opts.namespace.is_empty() {
            self.opts.prefix.clone()
        } else {
            format!("{}-{}", self.opts.prefix, self.opts.namespace)
        };

        let mut sub = client.subscribe(format!("{}.>", prefix)).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;

        tokio::spawn(async move {
            while let Some(msg) = sub.next().await {
                if let Ok(pkt) = serde_json::from_slice::<Packet>(&msg.payload) {
                    handler(pkt);
                }
            }
        });
        Ok(())
    }
}

#[cfg(not(feature = "nats"))]
#[async_trait]
impl Transporter for NatsTransporter {
    async fn connect(&self) -> Result<()> {
        Err(MoleculerError::Transport(
            "NATS transporter requires feature `nats` — add `nats` to features in Cargo.toml".into(),
        ))
    }
    async fn disconnect(&self) -> Result<()> { Ok(()) }
    async fn send(&self, _: Packet) -> Result<()> {
        Err(MoleculerError::Transport("NATS feature not enabled".into()))
    }
    async fn subscribe<F>(&self, _: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static {
        Err(MoleculerError::Transport("NATS feature not enabled".into()))
    }
}
