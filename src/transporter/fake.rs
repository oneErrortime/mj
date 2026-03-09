//! # FakeTransporter
//!
//! In-process test transporter — mirrors `src/transporters/fake.js` from Moleculer.js.
//!
//! ## Purpose
//!
//! `FakeTransporter` is designed exclusively for **unit and integration tests**.
//! It provides full [`Transporter`] semantics without any network I/O:
//!
//! - All sent packets are stored in an inspectable `sent` log
//! - Subscribers are notified synchronously (via tokio broadcast, same as Local)
//! - `connect()` / `disconnect()` can be rigged to fail via [`FakeTransporterOptions`]
//! - The entire state can be reset between test cases with [`FakeTransporter::reset`]
//! - Packet delivery can be paused / resumed (simulates network partition)
//!
//! ## Example
//!
//! ```rust,no_run
//! use moleculer::transporter::fake::{FakeTransporter, FakeTransporterOptions};
//! use moleculer::transporter::{Packet, PacketKind};
//! use std::sync::Arc;
//!
//! #[tokio::test]
//! async fn test_ping_pong() {
//!     let opts = FakeTransporterOptions::default();
//!     let t = Arc::new(FakeTransporter::new(opts));
//!
//!     t.connect().await.unwrap();
//!
//!     // Inject an incoming packet as if it arrived from a remote node
//!     t.inject(Packet {
//!         kind: PacketKind::Ping,
//!         sender: "remote-1".into(),
//!         target: Some("local".into()),
//!         payload: serde_json::json!({}),
//!         timestamp: chrono::Utc::now(),
//!     }).await;
//!
//!     // Assert what was sent
//!     let sent = t.sent_packets();
//!     assert_eq!(sent.len(), 0); // nothing sent yet
//! }
//! ```

use super::{Packet, PacketKind, Transporter};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::broadcast;

// ─── Options ─────────────────────────────────────────────────────────────────

/// Configuration for [`FakeTransporter`] fault injection.
#[derive(Debug, Clone)]
pub struct FakeTransporterOptions {
    /// If `true`, `connect()` always returns an error.
    pub fail_connect: bool,

    /// If `true`, `disconnect()` always returns an error.
    pub fail_disconnect: bool,

    /// If `true`, `send()` always returns an error (simulates write failure).
    pub fail_send: bool,

    /// Broadcast channel capacity (default 1024).
    pub channel_capacity: usize,
}

impl Default for FakeTransporterOptions {
    fn default() -> Self {
        Self {
            fail_connect: false,
            fail_disconnect: false,
            fail_send: false,
            channel_capacity: 1024,
        }
    }
}

// ─── FakeTransporter ─────────────────────────────────────────────────────────

/// Test-only transporter that records every packet sent and allows injecting
/// incoming packets programmatically.
pub struct FakeTransporter {
    opts: FakeTransporterOptions,

    /// All packets passed to `send()` (in order).
    sent: Mutex<Vec<Packet>>,

    /// Broadcast channel — subscribers listen here; `inject()` publishes here.
    tx: broadcast::Sender<Packet>,

    /// Connection state flag.
    connected: AtomicBool,

    /// Delivery paused flag — when `true`, `send()` still records but does NOT
    /// broadcast (simulates a network partition).
    paused: AtomicBool,

    /// Total bytes "sent" (sum of serialised JSON lengths).
    bytes_sent: AtomicU64,

    /// Total packets injected (received).
    packets_received: AtomicU64,
}

impl FakeTransporter {
    /// Create a new `FakeTransporter` with the given options.
    pub fn new(opts: FakeTransporterOptions) -> Self {
        let (tx, _) = broadcast::channel(opts.channel_capacity);
        Self {
            tx,
            opts,
            sent: Mutex::new(Vec::new()),
            connected: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            bytes_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    /// Returns a snapshot of every packet that was passed to `send()`.
    pub fn sent_packets(&self) -> Vec<Packet> {
        self.sent.lock().unwrap().clone()
    }

    /// Returns only packets of the given kind.
    pub fn sent_of_kind(&self, kind: PacketKind) -> Vec<Packet> {
        self.sent_packets()
            .into_iter()
            .filter(|p| p.kind == kind)
            .collect()
    }

    /// Total number of packets recorded via `send()`.
    pub fn sent_count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    /// Total bytes "sent" (JSON length of each payload).
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Total packets injected from the outside via [`inject`].
    pub fn received_count(&self) -> u64 {
        self.packets_received.load(Ordering::Relaxed)
    }

    /// Whether the transporter is currently in the "connected" state.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Clear the sent-packet log (useful between test cases).
    pub fn reset(&self) {
        self.sent.lock().unwrap().clear();
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.packets_received.store(0, Ordering::Relaxed);
    }

    /// Pause packet delivery.
    ///
    /// While paused, `send()` still *records* packets but does NOT broadcast
    /// them to subscribers — simulates a network partition or message queue
    /// back-pressure.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    /// Resume packet delivery (reverse of [`pause`]).
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Inject a packet as if it arrived from a remote node.
    ///
    /// All current subscribers will receive it.  The packet is **not** added to
    /// the `sent` log (it is incoming, not outgoing).
    pub async fn inject(&self, packet: Packet) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
        // Ignore send errors — may happen if no subscribers yet
        let _ = self.tx.send(packet);
    }

    /// Inject multiple packets in order.
    pub async fn inject_all(&self, packets: Vec<Packet>) {
        for p in packets {
            self.inject(p).await;
        }
    }
}

#[async_trait]
impl Transporter for FakeTransporter {
    async fn connect(&self) -> Result<()> {
        if self.opts.fail_connect {
            return Err(MoleculerError::Internal(
                "FakeTransporter: connect() rigged to fail".into(),
            ));
        }
        self.connected.store(true, Ordering::Relaxed);
        log::debug!("[FakeTransporter] connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        if self.opts.fail_disconnect {
            return Err(MoleculerError::Internal(
                "FakeTransporter: disconnect() rigged to fail".into(),
            ));
        }
        self.connected.store(false, Ordering::Relaxed);
        log::debug!("[FakeTransporter] disconnected");
        Ok(())
    }

    async fn send(&self, packet: Packet) -> Result<()> {
        if self.opts.fail_send {
            return Err(MoleculerError::Internal(
                "FakeTransporter: send() rigged to fail".into(),
            ));
        }

        // Track bytes
        let json_len = serde_json::to_string(&packet.payload)
            .map(|s| s.len() as u64)
            .unwrap_or(0);
        self.bytes_sent.fetch_add(json_len, Ordering::Relaxed);

        // Record in sent log
        self.sent.lock().unwrap().push(packet.clone());

        // Broadcast to subscribers (unless delivery is paused)
        if !self.paused.load(Ordering::Relaxed) {
            let _ = self.tx.send(packet);
        }

        Ok(())
    }

    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(Packet) + Send + Sync + 'static,
    {
        let mut rx = self.tx.subscribe();
        tokio::spawn(async move {
            while let Ok(p) = rx.recv().await {
                handler(p);
            }
        });
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transporter::{Packet, PacketKind};
    use chrono::Utc;
    use serde_json::json;

    fn make_packet(kind: PacketKind) -> Packet {
        Packet {
            kind,
            sender: "node-1".into(),
            target: None,
            payload: json!({}),
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn connect_sets_connected_flag() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        assert!(!t.is_connected());
        t.connect().await.unwrap();
        assert!(t.is_connected());
    }

    #[tokio::test]
    async fn disconnect_clears_connected_flag() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.connect().await.unwrap();
        t.disconnect().await.unwrap();
        assert!(!t.is_connected());
    }

    #[tokio::test]
    async fn fail_connect_returns_error() {
        let t = FakeTransporter::new(FakeTransporterOptions {
            fail_connect: true,
            ..Default::default()
        });
        assert!(t.connect().await.is_err());
    }

    #[tokio::test]
    async fn fail_send_returns_error() {
        let t = FakeTransporter::new(FakeTransporterOptions {
            fail_send: true,
            ..Default::default()
        });
        assert!(t.send(make_packet(PacketKind::Ping)).await.is_err());
    }

    #[tokio::test]
    async fn send_records_packet() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        t.send(make_packet(PacketKind::Heartbeat)).await.unwrap();
        assert_eq!(t.sent_count(), 2);
    }

    #[tokio::test]
    async fn sent_of_kind_filters_correctly() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        t.send(make_packet(PacketKind::Heartbeat)).await.unwrap();
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        assert_eq!(t.sent_of_kind(PacketKind::Ping).len(), 2);
        assert_eq!(t.sent_of_kind(PacketKind::Heartbeat).len(), 1);
    }

    #[tokio::test]
    async fn reset_clears_sent_log() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        assert_eq!(t.sent_count(), 1);
        t.reset();
        assert_eq!(t.sent_count(), 0);
        assert_eq!(t.bytes_sent(), 0);
        assert_eq!(t.received_count(), 0);
    }

    #[tokio::test]
    async fn bytes_sent_accumulates() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        let p = Packet {
            kind: PacketKind::Request,
            sender: "node-1".into(),
            target: None,
            payload: json!({ "a": 1, "b": "hello" }),
            timestamp: Utc::now(),
        };
        t.send(p).await.unwrap();
        assert!(t.bytes_sent() > 0);
    }

    #[tokio::test]
    async fn inject_increments_received_count() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.inject(make_packet(PacketKind::Pong)).await;
        t.inject(make_packet(PacketKind::Pong)).await;
        assert_eq!(t.received_count(), 2);
    }

    #[tokio::test]
    async fn inject_does_not_appear_in_sent_log() {
        let t = FakeTransporter::new(FakeTransporterOptions::default());
        t.inject(make_packet(PacketKind::Info)).await;
        assert_eq!(t.sent_count(), 0);
    }

    #[tokio::test]
    async fn subscribe_receives_sent_packets() {
        let t = Arc::new(FakeTransporter::new(FakeTransporterOptions::default()));
        let received: Arc<Mutex<Vec<Packet>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        t.subscribe(move |p| {
            received_clone.lock().unwrap().push(p);
        })
        .await
        .unwrap();

        t.send(make_packet(PacketKind::Heartbeat)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn paused_blocks_broadcast_but_records() {
        let t = Arc::new(FakeTransporter::new(FakeTransporterOptions::default()));
        let received: Arc<Mutex<Vec<Packet>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        t.subscribe(move |p| {
            received_clone.lock().unwrap().push(p);
        })
        .await
        .unwrap();

        t.pause();
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // recorded in sent log
        assert_eq!(t.sent_count(), 1);
        // but NOT delivered to subscribers
        assert_eq!(received.lock().unwrap().len(), 0);

        // resume and send another
        t.resume();
        t.send(make_packet(PacketKind::Ping)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn inject_reaches_subscribers() {
        let t = Arc::new(FakeTransporter::new(FakeTransporterOptions::default()));
        let received: Arc<Mutex<Vec<Packet>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        t.subscribe(move |p| {
            received_clone.lock().unwrap().push(p);
        })
        .await
        .unwrap();

        t.inject(make_packet(PacketKind::Discover)).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let recs = received.lock().unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, PacketKind::Discover);
    }

    #[tokio::test]
    async fn inject_all_delivers_in_order() {
        let t = Arc::new(FakeTransporter::new(FakeTransporterOptions::default()));
        let received: Arc<Mutex<Vec<Packet>>> = Arc::new(Mutex::new(Vec::new()));
        let rc = Arc::clone(&received);

        t.subscribe(move |p| { rc.lock().unwrap().push(p); }).await.unwrap();

        t.inject_all(vec![
            make_packet(PacketKind::Info),
            make_packet(PacketKind::Discover),
            make_packet(PacketKind::Heartbeat),
        ])
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let recs = received.lock().unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].kind, PacketKind::Info);
        assert_eq!(recs[1].kind, PacketKind::Discover);
        assert_eq!(recs[2].kind, PacketKind::Heartbeat);
    }
}
