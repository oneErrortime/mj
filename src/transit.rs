//! # transit
//!
//! Inter-node communication layer — mirrors `src/transit.js` from Moleculer.js.
//!
//! [`Transit`] sits between the [`ServiceBroker`] and a [`Transporter`].
//! It is responsible for:
//!
//! - **Connecting / reconnecting** to the transporter with exponential back-off.
//! - **Subscribing** to the correct topic channels for this node.
//! - **Serialising** outgoing packets and **deserialising** incoming ones.
//! - **Pending request tracking** — correlating REQ → RES packets.
//! - **Heartbeat** — sending periodic HEARTBEAT packets so peers know we're alive.
//! - **Discovery** — sending INFO / DISCOVER packets on connect/reconnect.
//! - **Ping / Pong** — measuring round-trip time to remote nodes.
//! - **Packet statistics** — counting sent/received bytes and packets.
//!
//! ## Relationship to other modules
//!
//! ```text
//! ServiceBroker
//!   └─ Transit
//!        ├─ Transporter (trait) — sends/receives raw bytes
//!        ├─ Serializer  (trait) — converts Value ↔ bytes
//!        └─ ServiceRegistry     — registers remote nodes / actions
//! ```

use crate::constants::*;
use crate::error::{MoleculerError, Result};
use crate::registry::ServiceRegistry;
use crate::serializers::{JsonSerializer, Serializer};
use crate::transporter::{Packet, PacketKind, Transporter};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time;

// ─── Packet statistics ────────────────────────────────────────────────────────

/// Counters for packets sent and received.
#[derive(Debug, Default)]
pub struct PacketStats {
    pub sent_count: AtomicU64,
    pub sent_bytes: AtomicU64,
    pub received_count: AtomicU64,
    pub received_bytes: AtomicU64,
}

impl PacketStats {
    pub fn snapshot(&self) -> PacketStatsSnapshot {
        PacketStatsSnapshot {
            sent_count: self.sent_count.load(Ordering::Relaxed),
            sent_bytes: self.sent_bytes.load(Ordering::Relaxed),
            received_count: self.received_count.load(Ordering::Relaxed),
            received_bytes: self.received_bytes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PacketStatsSnapshot {
    pub sent_count: u64,
    pub sent_bytes: u64,
    pub received_count: u64,
    pub received_bytes: u64,
}

// ─── Pending request entry ────────────────────────────────────────────────────

struct PendingRequest {
    /// Resolves the caller's `await broker.call(...)`.
    resolve: oneshot::Sender<Result<Value>>,
    /// When the request was sent (for timeout calculation).
    started_at: u64,
    /// Action name, for logging.
    action: String,
}

// ─── Transit ─────────────────────────────────────────────────────────────────

/// The inter-node communication layer.
pub struct Transit {
    pub node_id: String,
    pub namespace: String,

    transporter: Arc<dyn Transporter>,
    serializer: Arc<dyn Serializer>,
    registry: Arc<ServiceRegistry>,

    /// Pending outgoing requests, keyed by request-id.
    pending: Mutex<HashMap<String, PendingRequest>>,

    /// Packet statistics.
    pub stat: Arc<PacketStats>,

    pub connected: RwLock<bool>,
    pub is_ready: RwLock<bool>,

    /// Heartbeat interval (seconds).
    heartbeat_interval: u64,
    /// If `true`, do not attempt reconnection.
    disable_reconnect: bool,
}

impl Transit {
    /// Create a new [`Transit`] instance.
    ///
    /// In most cases the [`ServiceBroker`] creates this automatically.
    pub fn new(
        node_id: impl Into<String>,
        namespace: impl Into<String>,
        transporter: Arc<dyn Transporter>,
        registry: Arc<ServiceRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            node_id: node_id.into(),
            namespace: namespace.into(),
            transporter,
            serializer: Arc::new(JsonSerializer),
            registry,
            pending: Mutex::new(HashMap::new()),
            stat: Arc::new(PacketStats::default()),
            connected: RwLock::new(false),
            is_ready: RwLock::new(false),
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            disable_reconnect: false,
        })
    }

    // ─── Connect / disconnect ──────────────────────────────────────────────

    /// Connect to the transporter.
    ///
    /// Retries indefinitely (with exponential back-off up to 30 s)
    /// unless `disable_reconnect` is set.
    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        let mut attempt = 0u32;
        loop {
            match self.transporter.connect().await {
                Ok(_) => {
                    self.after_connect(attempt > 0).await?;
                    return Ok(());
                }
                Err(e) => {
                    if self.disable_reconnect {
                        return Err(e);
                    }
                    let delay = Duration::from_secs(std::cmp::min(5 * (attempt as u64 + 1), 30));
                    eprintln!(
                        "[transit] Connection failed (attempt {}): {}. Retrying in {:?}…",
                        attempt + 1, e, delay
                    );
                    time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Disconnect cleanly.
    pub async fn disconnect(&self) -> Result<()> {
        *self.connected.write().await = false;
        *self.is_ready.write().await = false;

        // Reject all pending requests
        let mut pending = self.pending.lock().await;
        for (_, req) in pending.drain() {
            let _ = req.resolve.send(Err(MoleculerError::Transport(
                "Transit disconnected".to_string(),
            )));
        }
        drop(pending);

        self.transporter.disconnect().await
    }

    // ─── After connect ─────────────────────────────────────────────────────

    async fn after_connect(self: &Arc<Self>, was_reconnect: bool) -> Result<()> {
        self.make_subscriptions().await?;

        if was_reconnect {
            self.send_node_info(None).await?;
        } else {
            self.discover_all_nodes().await?;
        }

        // Small settling delay to allow INFO packets to arrive
        time::sleep(Duration::from_millis(500)).await;

        *self.connected.write().await = true;
        *self.is_ready.write().await = true;

        Ok(())
    }

    // ─── Topic subscriptions ───────────────────────────────────────────────

    /// Subscribe to all topics relevant for this node.
    async fn make_subscriptions(self: &Arc<Self>) -> Result<()> {
        let ns = if self.namespace.is_empty() {
            String::new()
        } else {
            format!("{}.", self.namespace)
        };

        // Broadcast topics (all nodes receive)
        let broadcast_topics = vec![
            format!("{}MOL.DISCOVER", ns),
            format!("{}MOL.INFO", ns),
            format!("{}MOL.DISCONNECT", ns),
            format!("{}MOL.HEARTBEAT", ns),
            format!("{}MOL.PING", ns),
        ];

        // Node-specific topics (only this node receives)
        let node_topics = vec![
            format!("{}MOL.DISCOVER.{}", ns, self.node_id),
            format!("{}MOL.INFO.{}", ns, self.node_id),
            format!("{}MOL.REQ.{}", ns, self.node_id),
            format!("{}MOL.RES.{}", ns, self.node_id),
            format!("{}MOL.EVENT.{}", ns, self.node_id),
            format!("{}MOL.PING.{}", ns, self.node_id),
            format!("{}MOL.PONG.{}", ns, self.node_id),
        ];

        let self_arc = Arc::clone(self);
        let handler = Arc::new(move |packet: Packet| {
            let transit = Arc::clone(&self_arc);
            tokio::spawn(async move {
                if let Err(e) = transit.message_handler(packet).await {
                    eprintln!("[transit] messageHandler error: {}", e);
                }
            });
        });

        for _topic in broadcast_topics.iter().chain(node_topics.iter()) {
            self.transporter.subscribe((*handler).clone()).await?;
        }

        Ok(())
    }

    // ─── Message handler ───────────────────────────────────────────────────

    /// Dispatch an incoming [`Packet`] to the appropriate handler.
    pub async fn message_handler(&self, packet: Packet) -> Result<()> {
        // Update receive stats
        let raw_bytes = self.serializer.serialize(&packet.payload)
            .map(|b| b.len() as u64)
            .unwrap_or(0);
        self.stat.received_count.fetch_add(1, Ordering::Relaxed);
        self.stat.received_bytes.fetch_add(raw_bytes, Ordering::Relaxed);

        match packet.kind {
            PacketKind::Discover => self.handle_discover(&packet).await,
            PacketKind::Info => self.handle_info(&packet).await,
            PacketKind::Disconnect => self.handle_disconnect(&packet).await,
            PacketKind::Heartbeat => self.handle_heartbeat(&packet).await,
            PacketKind::Request => self.handle_request(&packet).await,
            PacketKind::Response => self.handle_response(&packet).await,
            PacketKind::Event => self.handle_event(&packet).await,
            PacketKind::Ping => self.handle_ping(&packet).await,
            PacketKind::Pong => self.handle_pong(&packet).await,
            _ => Ok(()),
        }
    }

    // ─── Packet handlers ───────────────────────────────────────────────────

    async fn handle_discover(&self, packet: &Packet) -> Result<()> {
        let sender = packet.sender.clone();
        self.send_node_info(Some(sender)).await
    }

    async fn handle_info(&self, packet: &Packet) -> Result<()> {
        let sender = packet.sender.clone();
        let services = packet.payload.get("services").cloned().unwrap_or_default();

        // Register the remote node
        self.registry.register_remote_node(
            sender.clone(),
            services,
        );

        Ok(())
    }

    async fn handle_disconnect(&self, packet: &Packet) -> Result<()> {
        let sender = &packet.sender;
        self.registry.unregister_node(sender);
        Ok(())
    }

    async fn handle_heartbeat(&self, packet: &Packet) -> Result<()> {
        let sender = &packet.sender;
        self.registry.heartbeat_received(sender);
        Ok(())
    }

    async fn handle_request(&self, _packet: &Packet) -> Result<()> {
        // Remote request handling will be wired through ServiceBroker.
        // For now we acknowledge receipt.
        Ok(())
    }

    async fn handle_response(&self, packet: &Packet) -> Result<()> {
        let request_id = packet.payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut pending = self.pending.lock().await;
        if let Some(req) = pending.remove(&request_id) {
            let result = if packet.payload.get("success") == Some(&Value::Bool(true)) {
                Ok(packet.payload.get("data").cloned().unwrap_or(Value::Null))
            } else {
                let msg = packet.payload
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Remote error")
                    .to_string();
                Err(MoleculerError::Internal(msg))
            };
            let _ = req.resolve.send(result);
        } else {
            eprintln!(
                "[transit] Orphan response received for request '{}'",
                request_id
            );
        }

        Ok(())
    }

    async fn handle_event(&self, _packet: &Packet) -> Result<()> {
        // Remote event delivery will be wired through ServiceBroker.
        Ok(())
    }

    async fn handle_ping(&self, packet: &Packet) -> Result<()> {
        let sender = packet.sender.clone();
        let id = packet.payload.get("id").cloned().unwrap_or(Value::Null);
        let sent_at = packet.payload.get("time").cloned().unwrap_or(Value::Null);

        let pong = Packet {
            kind: PacketKind::Pong,
            sender: self.node_id.clone(),
            target: Some(sender),
            payload: json!({
                "id": id,
                "received": now_ms(),
                "time": sent_at,
            }),
            timestamp: chrono::Utc::now(),
        };
        self.publish(pong).await
    }

    async fn handle_pong(&self, packet: &Packet) -> Result<()> {
        let sent_at = packet.payload.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
        let received = packet.payload.get("received").and_then(|v| v.as_u64()).unwrap_or(0);
        let now = now_ms();

        let elapsed_ms = now.saturating_sub(sent_at) as i64;
        let time_diff = (received as i64) - (sent_at as i64) - elapsed_ms / 2;

        eprintln!(
            "[transit] Pong from '{}': RTT={}ms, time diff={}ms",
            packet.sender, elapsed_ms, time_diff
        );

        Ok(())
    }

    // ─── Outgoing packets ──────────────────────────────────────────────────

    /// Publish a packet via the transporter.
    pub async fn publish(&self, packet: Packet) -> Result<()> {
        let bytes = self.serializer.serialize(&packet.payload)?;
        let byte_len = bytes.len() as u64;

        self.transporter.send(packet).await?;

        self.stat.sent_count.fetch_add(1, Ordering::Relaxed);
        self.stat.sent_bytes.fetch_add(byte_len, Ordering::Relaxed);

        Ok(())
    }

    /// Send an INFO packet advertising this node's services.
    ///
    /// If `target` is `Some(node_id)`, the packet is sent directly.
    /// Otherwise it is broadcast to all nodes.
    pub async fn send_node_info(&self, target: Option<String>) -> Result<()> {
        let services: Vec<Value> = self.registry
            .services
            .iter()
            .map(|entry| {
                json!({
                    "name": entry.key(),
                })
            })
            .collect();

        let packet = Packet {
            kind: PacketKind::Info,
            sender: self.node_id.clone(),
            target,
            payload: json!({
                "services": services,
                "ipList": [],
                "hostname": hostname(),
                "client": {
                    "type": "rust",
                    "version": env!("CARGO_PKG_VERSION"),
                    "langVersion": "rust"
                },
                "seq": 1,
                "instanceID": uuid::Uuid::new_v4().to_string(),
                "metadata": {}
            }),
            timestamp: chrono::Utc::now(),
        };
        self.publish(packet).await
    }

    /// Broadcast a DISCOVER packet to find all remote nodes.
    pub async fn discover_all_nodes(&self) -> Result<()> {
        let packet = Packet {
            kind: PacketKind::Discover,
            sender: self.node_id.clone(),
            target: None,
            payload: json!({}),
            timestamp: chrono::Utc::now(),
        };
        self.publish(packet).await
    }

    /// Send a HEARTBEAT packet to all nodes.
    pub async fn send_heartbeat(&self, cpu: f64) -> Result<()> {
        let packet = Packet {
            kind: PacketKind::Heartbeat,
            sender: self.node_id.clone(),
            target: None,
            payload: json!({ "cpu": cpu }),
            timestamp: chrono::Utc::now(),
        };
        self.publish(packet).await
    }

    /// Send a PING packet to a specific node (or broadcast if `target` is `None`).
    pub async fn send_ping(&self, target: Option<String>) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let packet = Packet {
            kind: PacketKind::Ping,
            sender: self.node_id.clone(),
            target,
            payload: json!({ "id": id, "time": now_ms() }),
            timestamp: chrono::Utc::now(),
        };
        self.publish(packet).await?;
        Ok(id)
    }

    /// Send a DISCONNECT packet so peers can clean up our registrations.
    pub async fn send_disconnect(&self) -> Result<()> {
        let packet = Packet {
            kind: PacketKind::Disconnect,
            sender: self.node_id.clone(),
            target: None,
            payload: json!({}),
            timestamp: chrono::Utc::now(),
        };
        self.publish(packet).await
    }

    /// Register a pending outgoing request; returns the response receiver.
    pub async fn register_pending_request(
        &self,
        request_id: impl Into<String>,
        action: impl Into<String>,
    ) -> oneshot::Receiver<Result<Value>> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        pending.insert(
            request_id.into(),
            PendingRequest {
                resolve: tx,
                started_at: now_ms(),
                action: action.into(),
            },
        );
        rx
    }

    // ─── Heartbeat loop ────────────────────────────────────────────────────

    /// Spawn a background task that sends heartbeats every `heartbeat_interval` seconds.
    pub fn start_heartbeat_loop(self: &Arc<Self>) {
        let transit = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(transit.heartbeat_interval));
            loop {
                interval.tick().await;
                if *transit.connected.read().await {
                    if let Err(e) = transit.send_heartbeat(0.0).await {
                        eprintln!("[transit] {}: {}", FAILED_SEND_HEARTBEAT_PACKET, e);
                    }
                }
            }
        });
    }

    // ─── Stats snapshot ────────────────────────────────────────────────────

    pub fn packet_stats(&self) -> PacketStatsSnapshot {
        self.stat.snapshot()
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
