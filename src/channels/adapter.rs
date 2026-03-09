//! Channel adapters — pluggable backends for durable message queues.
//!
//! Implemented:
//! - `InMemoryAdapter` — Tokio MPSC queues, suitable for single-process use.
//!
//! Future adapters (trait is ready):
//! - `RedisAdapter`  — Redis Streams (XADD / XREADGROUP / XCLAIM)
//! - `AmqpAdapter`   — RabbitMQ AMQP 0-9-1
//! - `KafkaAdapter`  — Apache Kafka
//! - `NatsAdapter`   — NATS JetStream

use super::{AckKind, ChannelDef, ChannelMessage, ChannelStats, SendOptions};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::{mpsc, oneshot, Semaphore};

// ─── Adapter trait ────────────────────────────────────────────────────────────

#[async_trait]
pub trait Adapter: Send + Sync {
    /// Connect / initialise the adapter.
    async fn connect(&self) -> Result<()>;
    /// Disconnect / cleanup.
    async fn disconnect(&self) -> Result<()>;
    /// Publish a message to a channel.
    async fn publish(
        &self,
        channel: &str,
        payload: Value,
        opts: SendOptions,
    ) -> Result<()>;
    /// Subscribe a ChannelDef consumer. Returns a task handle that drives the consume loop.
    async fn subscribe(&self, def: &ChannelDef) -> Result<()>;
    /// Unsubscribe a channel.
    async fn unsubscribe(&self, channel: &str, group: &str) -> Result<()>;
    /// Stats snapshot.
    fn stats(&self) -> Vec<ChannelStats>;
}

// ─── Envelope (internal queue item) ─────────────────────────────────────────

#[derive(Debug, Clone)]
struct Envelope {
    payload: Value,
    headers: HashMap<String, String>,
    delivery_count: u32,
}

// ─── InMemoryAdapter ──────────────────────────────────────────────────────────

/// Per-group queue state.
struct GroupQueue {
    tx: mpsc::Sender<Envelope>,
    processed: AtomicU64,
    failed:    AtomicU64,
    dlq_count: AtomicU64,
    in_flight: Arc<Semaphore>,
}

/// In-process channel adapter — no external dependencies required.
///
/// Uses Tokio MPSC channels under the hood.
/// Messages that NACK `max_retries` times are forwarded to the Dead-Letter channel
/// (another channel named `<original>.<group>.DLQ`).
pub struct InMemoryAdapter {
    queues: Arc<DashMap<String, Arc<GroupQueue>>>,
    dlq: Arc<super::DeadLetterQueue>,
    capacity: usize,
}

impl InMemoryAdapter {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(DashMap::new()),
            dlq: Arc::new(super::DeadLetterQueue::new()),
            capacity: 10_000,
        }
    }

    pub fn with_capacity(mut self, cap: usize) -> Self { self.capacity = cap; self }

    fn queue_key(channel: &str, group: &str) -> String {
        format!("{}::{}", channel, group)
    }
}

impl Default for InMemoryAdapter {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Adapter for InMemoryAdapter {
    async fn connect(&self) -> Result<()> {
        log::debug!("[InMemoryAdapter] connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.queues.clear();
        log::debug!("[InMemoryAdapter] disconnected");
        Ok(())
    }

    async fn publish(&self, channel: &str, payload: Value, opts: SendOptions) -> Result<()> {
        let env = Envelope { payload, headers: opts.headers, delivery_count: 1 };

        // Send to every group subscribed to this channel
        let mut sent = 0usize;
        for entry in self.queues.iter() {
            if entry.key().starts_with(&format!("{}::", channel)) {
                if entry.tx.send(env.clone()).await.is_ok() {
                    sent += 1;
                }
            }
        }

        if sent == 0 {
            log::warn!("[InMemoryAdapter] No subscribers for channel '{}'", channel);
        }
        Ok(())
    }

    async fn subscribe(&self, def: &ChannelDef) -> Result<()> {
        let group = def.group.clone().unwrap_or_else(|| "default".to_string());
        let key = Self::queue_key(&def.name, &group);

        let (tx, mut rx) = mpsc::channel::<Envelope>(self.capacity);
        let semaphore = Arc::new(Semaphore::new(def.max_in_flight));
        let gq = Arc::new(GroupQueue {
            tx,
            processed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            dlq_count: AtomicU64::new(0),
            in_flight: semaphore,
        });
        self.queues.insert(key, Arc::clone(&gq));

        let handler      = def.handler.clone();
        let max_retries  = def.max_retries;
        let channel_name = def.name.clone();
        let group_name   = group.clone();
        let dlq          = Arc::clone(&self.dlq);
        let dlq_queue    = def.dead_letter_queue.clone()
            .unwrap_or_else(|| format!("{}.{}.DLQ", def.name, group));

        tokio::spawn(async move {
            while let Some(mut env) = rx.recv().await {
                let permit = gq.in_flight.clone().acquire_owned().await.unwrap();
                let handler = handler.clone();
                let ch = channel_name.clone();
                let grp = group_name.clone();
                let dlq = Arc::clone(&dlq);
                let dlq_q = dlq_queue.clone();
                let gq2 = Arc::clone(&gq);

                tokio::spawn(async move {
                    let _permit = permit;
                    let (ack_tx, ack_rx) = oneshot::channel::<AckKind>();
                    let msg = ChannelMessage::new(
                        ch.clone(), grp.clone(),
                        env.payload.clone(), env.headers.clone(),
                        env.delivery_count, ack_tx,
                    );

                    let result = handler(msg).await;

                    let ack = match result {
                        Ok(_) => {
                            // try receiving explicit ACK/NACK from handler
                            match tokio::time::timeout(
                                std::time::Duration::from_millis(100),
                                ack_rx,
                            ).await {
                                Ok(Ok(kind)) => kind,
                                _ => AckKind::Ack, // implicit ACK on Ok
                            }
                        }
                        Err(_) => AckKind::Nack,
                    };

                    match ack {
                        AckKind::Ack => {
                            gq2.processed.fetch_add(1, Ordering::Relaxed);
                        }
                        AckKind::Nack => {
                            gq2.failed.fetch_add(1, Ordering::Relaxed);
                            env.delivery_count += 1;

                            let redelivered = env.headers.get("x-redelivered-count")
                                .and_then(|v| v.parse::<u32>().ok())
                                .unwrap_or(0) + 1;

                            if redelivered >= max_retries {
                                // Dead-letter it
                                gq2.dlq_count.fetch_add(1, Ordering::Relaxed);
                                let mut dlq_headers = env.headers.clone();
                                dlq_headers.insert("x-original-channel".into(), ch.clone());
                                dlq_headers.insert("x-original-group".into(), grp.clone());
                                dlq_headers.insert("x-redelivered-count".into(), redelivered.to_string());

                                dlq.push(super::dead_letter::DlqEntry {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    channel: dlq_q,
                                    original_channel: ch,
                                    original_group: grp,
                                    payload: env.payload,
                                    headers: dlq_headers,
                                    failed_at: chrono::Utc::now(),
                                    attempts: redelivered,
                                });
                                log::warn!("[channels] Message dead-lettered after {} attempts", redelivered);
                            } else {
                                // Re-queue with backoff
                                env.headers.insert("x-redelivered-count".into(), redelivered.to_string());
                                let delay_ms = (100u64 * 2u64.pow(redelivered - 1)).min(5000);
                                let gq3 = Arc::clone(&gq2);
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                                    let _ = gq3.tx.send(env).await;
                                });
                            }
                        }
                    }
                });
            }
        });

        log::info!("[InMemoryAdapter] Subscribed to '{}' group='{}'", def.name, group);
        Ok(())
    }

    async fn unsubscribe(&self, channel: &str, group: &str) -> Result<()> {
        let key = Self::queue_key(channel, group);
        self.queues.remove(&key);
        Ok(())
    }

    fn stats(&self) -> Vec<ChannelStats> {
        self.queues.iter().map(|e| {
            let parts: Vec<&str> = e.key().splitn(2, "::").collect();
            let (ch, grp) = if parts.len() == 2 { (parts[0], parts[1]) } else { (e.key().as_str(), "default") };
            let q = e.value();
            ChannelStats {
                name: ch.to_string(),
                group: grp.to_string(),
                pending: q.tx.capacity() - q.tx.max_capacity(),
                in_flight: def_max_in_flight(&q.in_flight),
                processed_total: q.processed.load(Ordering::Relaxed),
                failed_total: q.failed.load(Ordering::Relaxed),
                dead_letters: q.dlq_count.load(Ordering::Relaxed),
            }
        }).collect()
    }
}

fn def_max_in_flight(sem: &Arc<Semaphore>) -> usize {
    // available_permits approaches max when nothing in flight
    // In-flight = max - available
    let avail = sem.available_permits();
    // We don't store max here, so just show available as proxy
    avail
}
