//! Channels — durable message queue equivalent of `@moleculer/channels`.
//!
//! ## Concept
//!
//! Moleculer's built-in `emit`/`broadcast` are fire-and-forget events — NOT durable.
//! Channels solve this with **persistent queues**:
//!
//! - Messages are stored until successfully ACK'd
//! - Consumers belong to **groups** (competing consumers within a group share load)
//! - Messages that fail `max_retries` times go to the **Dead-Letter Queue**
//! - `max_in_flight` limits concurrent in-progress messages per consumer
//!
//! ## Architecture (reverse-engineered from @moleculer/channels)
//!
//! ```text
//!  Producer                     Adapter (In-Mem / Redis / AMQP / Kafka)
//!  broker.send_to_channel()  →  adapter.publish(channel, payload, headers)
//!
//!  Consumer                     per-group competing receive loop
//!  ChannelDef handler      ←    adapter.subscribe(channel_def, service)
//!
//!  On failure (NACK):
//!    retry_count < max_retries  →  re-queue with backoff + incremented header
//!    retry_count >= max_retries →  move to Dead-Letter Queue
//! ```
//!
//! ## Headers (mirrors @moleculer/channels constants)
//!
//! | Header                   | Description                            |
//! |--------------------------|----------------------------------------|
//! | `x-redelivered-count`    | How many times this msg was retried    |
//! | `x-group`                | Consumer group name                    |
//! | `x-original-channel`     | Channel where the error occurred       |
//! | `x-original-group`       | Group that could not process it        |
//! | `x-error-message`        | Error message on DLQ entry             |
//! | `x-error-type`           | Error type string                      |

pub mod adapter;
pub mod adapter_redis;
pub mod dead_letter;
pub mod registry;

pub use adapter::{Adapter, InMemoryAdapter};
pub use dead_letter::DeadLetterQueue;
pub use registry::ChannelRegistry;

use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::oneshot;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub type MsgFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
pub type ChannelHandler = Arc<dyn Fn(ChannelMessage) -> MsgFuture + Send + Sync>;

// ─── Message ACK kind ────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckKind { Ack, Nack }

// ─── ChannelMessage ───────────────────────────────────────────────────────────
/// A message delivered from a channel. Call `.ack()` or `.nack()` after processing.
#[derive(Clone)]
pub struct ChannelMessage {
    pub id: String,
    pub channel: String,
    pub group: String,
    pub payload: Value,
    pub headers: HashMap<String, String>,
    /// Delivery attempt number (starts at 1).
    pub delivery_count: u32,
    pub timestamp: DateTime<Utc>,
    ack_tx: Option<Arc<tokio::sync::Mutex<Option<oneshot::Sender<AckKind>>>>>,
}

impl std::fmt::Debug for ChannelMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelMessage")
            .field("id", &self.id)
            .field("channel", &self.channel)
            .field("group", &self.group)
            .field("delivery_count", &self.delivery_count)
            .finish()
    }
}

impl ChannelMessage {
    pub(crate) fn new(
        channel: impl Into<String>,
        group: impl Into<String>,
        payload: Value,
        headers: HashMap<String, String>,
        delivery_count: u32,
        ack_tx: oneshot::Sender<AckKind>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            channel: channel.into(),
            group: group.into(),
            payload, headers, delivery_count,
            timestamp: Utc::now(),
            ack_tx: Some(Arc::new(tokio::sync::Mutex::new(Some(ack_tx)))),
        }
    }

    /// Create a test/manual message (no ACK sender — ACK is a no-op).
    pub fn manual(channel: impl Into<String>, payload: Value) -> Self {
        let (tx, _rx) = oneshot::channel();
        Self::new(channel, "default", payload, HashMap::new(), 1, tx)
    }

    /// Acknowledge — message processed successfully.
    pub async fn ack(self) -> Result<()> { self.send_ack(AckKind::Ack).await }
    /// Negative acknowledge — will be retried or dead-lettered.
    pub async fn nack(self) -> Result<()> { self.send_ack(AckKind::Nack).await }

    async fn send_ack(self, kind: AckKind) -> Result<()> {
        if let Some(tx_arc) = &self.ack_tx {
            let mut lock = tx_arc.lock().await;
            if let Some(tx) = lock.take() { let _ = tx.send(kind); }
        }
        Ok(())
    }

    pub fn redelivered_count(&self) -> u32 {
        self.headers.get("x-redelivered-count")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
}

// ─── ChannelDef ───────────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct ChannelDef {
    pub name: String,
    pub handler: ChannelHandler,
    pub group: Option<String>,
    pub max_in_flight: usize,
    pub max_retries: u32,
    pub context: bool,
    pub dead_letter_queue: Option<String>,
}

impl ChannelDef {
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(ChannelMessage) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        Self {
            name: name.into(),
            handler: Arc::new(move |msg| Box::pin(handler(msg))),
            group: None, max_in_flight: 1, max_retries: 3,
            context: false, dead_letter_queue: None,
        }
    }
    pub fn group(mut self, g: impl Into<String>) -> Self { self.group = Some(g.into()); self }
    pub fn max_in_flight(mut self, n: usize) -> Self { self.max_in_flight = n; self }
    pub fn max_retries(mut self, n: u32) -> Self { self.max_retries = n; self }
    pub fn dead_letter(mut self, q: impl Into<String>) -> Self { self.dead_letter_queue = Some(q.into()); self }
}

// ─── Send options ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct SendOptions {
    pub headers: HashMap<String, String>,
    pub key: Option<String>,
    /// Optional MAXLEN for Redis Streams (XADD MAXLEN ~ N)
    pub max_len: Option<u64>,
}
impl SendOptions {
    pub fn new() -> Self { Self::default() }
    pub fn header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.insert(k.into(), v.into()); self
    }
}

// ─── Stats ────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelStats {
    pub name: String,
    pub group: String,
    pub pending: usize,
    pub in_flight: usize,
    pub processed_total: u64,
    pub failed_total: u64,
    pub dead_letters: u64,
}

impl std::fmt::Debug for ChannelDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelDef")
            .field("name", &self.name)
            .field("group", &self.group)
            .field("max_in_flight", &self.max_in_flight)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}
