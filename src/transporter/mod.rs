//! Transporter — communication bus between nodes.
//!
//! Currently ships with an in-process (local) transporter.
//! The trait is designed for NATS, Redis, TCP, AMQP etc.

pub mod local;

use async_trait::async_trait;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A packet transmitted between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub kind: PacketKind,
    pub sender: String,
    pub target: Option<String>,
    pub payload: Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PacketKind {
    /// Node info / heartbeat
    Info,
    /// Service discovery
    Discover,
    /// Action request
    Request,
    /// Action response
    Response,
    /// Balanced event
    Event,
    /// Broadcast event
    Broadcast,
    /// Ping
    Ping,
    /// Pong
    Pong,
    /// Node disconnected
    Disconnect,
}

/// Transporter trait — implement this to add custom transport.
#[async_trait]
pub trait Transporter: Send + Sync {
    /// Connect to the transport.
    async fn connect(&self) -> Result<()>;
    /// Disconnect from the transport.
    async fn disconnect(&self) -> Result<()>;
    /// Send a packet to a specific target or broadcast.
    async fn send(&self, packet: Packet) -> Result<()>;
    /// Subscribe to incoming packets and invoke callback.
    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(Packet) + Send + Sync + 'static;
}
