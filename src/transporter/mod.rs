pub mod local;
pub mod fake;
pub mod tcp;
pub mod nats;
pub mod redis_t;

pub use local::LocalTransporter;
pub use fake::FakeTransporter;
pub use tcp::{TcpTransporter, TcpOptions};
pub use nats::{NatsTransporter, NatsOptions};
pub use redis_t::{RedisTransporter, RedisOptions};

use async_trait::async_trait;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PacketKind {
    Info, Discover, Request, Response, Event, Broadcast,
    Ping, Pong, Disconnect, Heartbeat,
    GossipHello, GossipPing, GossipPong, GossipUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub kind: PacketKind,
    pub sender: String,
    pub target: Option<String>,
    pub payload: Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chunks: Option<u32>,
}

impl Packet {
    pub fn new(kind: PacketKind, sender: impl Into<String>, payload: Value) -> Self {
        Self { kind, sender: sender.into(), target: None, payload,
               timestamp: chrono::Utc::now(), correlation_id: None,
               seq: None, total_chunks: None }
    }
    pub fn with_target(mut self, t: impl Into<String>) -> Self { self.target = Some(t.into()); self }
}

#[async_trait]
pub trait Transporter: Send + Sync {
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn send(&self, packet: Packet) -> Result<()>;
    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static;
    async fn on_connected(&self) -> Result<()> { Ok(()) }
    async fn on_disconnected(&self) -> Result<()> { Ok(()) }
}

pub fn create_transporter(spec: &str) -> Box<dyn Transporter> {
    if spec.starts_with("nats://") {
        Box::new(NatsTransporter::new(NatsOptions { url: spec.into(), ..Default::default() }))
    } else if spec.starts_with("redis://") || spec.starts_with("rediss://") {
        Box::new(RedisTransporter::new(RedisOptions { url: spec.into(), ..Default::default() }))
    } else if spec == "tcp" || spec.starts_with("tcp://") {
        Box::new(TcpTransporter::new("node", TcpOptions::default()))
    } else if spec == "fake" {
        Box::new(FakeTransporter::with_defaults())
    } else {
        Box::new(LocalTransporter::new())
    }
}
