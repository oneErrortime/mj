pub mod local;
use async_trait::async_trait;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PacketKind { Info, Discover, Request, Response, Event, Broadcast, Ping, Pong, Disconnect, Heartbeat }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet { pub kind: PacketKind, pub sender: String, pub target: Option<String>, pub payload: Value, pub timestamp: chrono::DateTime<chrono::Utc> }

#[async_trait]
pub trait Transporter: Send + Sync {
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn send(&self, packet: Packet) -> Result<()>;
    async fn subscribe<F>(&self, handler: F) -> Result<()> where F: Fn(Packet) + Send + Sync + 'static;
}
