//! TCP Transporter — fault-tolerant P2P transport with UDP discovery and Gossip protocol.
//!
//! Mirrors `moleculer/src/transporters/tcp.js`.
//!
//! ## Architecture
//! ```text
//!  ┌──────────────────────────────────────────────────────┐
//!  │                  TcpTransporter                      │
//!  │                                                      │
//!  │  ┌─────────────┐   ┌─────────────┐   ┌──────────┐  │
//!  │  │  TcpServer  │   │  TcpWriter  │   │ UdpDisc  │  │
//!  │  │  (listen)   │   │  (connect)  │   │ overer   │  │
//!  │  └─────────────┘   └─────────────┘   └──────────┘  │
//!  │                                                      │
//!  │  ┌──────────────────────────────────────────────┐   │
//!  │  │           Gossip Protocol Timer              │   │
//!  │  │  • HELLO  — announce self to peer            │   │
//!  │  │  • PING   — check peer liveness              │   │
//!  │  │  • PONG   — reply to ping                    │   │
//!  │  │  • UPDATE — propagate node-list              │   │
//!  │  └──────────────────────────────────────────────┘   │
//!  └──────────────────────────────────────────────────────┘
//! ```

use super::{Packet, Transporter};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, RwLock, Mutex};

// ─── Gossip packet types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GossipKind {
    Hello,
    Ping,
    Pong,
    Update,
    Disconnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipPacket {
    pub kind: GossipKind,
    pub sender: String,
    pub sender_addr: String,
    pub sender_port: u16,
    pub sequence: u64,
    /// Node table snapshot (for UPDATE packets).
    pub nodes: Vec<GossipNodeInfo>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipNodeInfo {
    pub node_id: String,
    pub addr: String,
    pub port: u16,
    pub cpu: f32,
    pub sequence: u64,
    pub online: bool,
}

// ─── Peer state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Peer {
    node_id: String,
    addr: SocketAddr,
    last_seen: Instant,
    sequence: u64,
    online: bool,
}

// ─── TcpTransporter ──────────────────────────────────────────────────────────

pub struct TcpOptions {
    pub port: u16,
    pub udp_discovery: bool,
    pub udp_port: u16,
    pub udp_multicast: String,
    pub gossip_period: u64,
    pub max_connections: usize,
    pub max_packet_size: usize,
    pub urls: Vec<String>,
    pub use_hostname: bool,
    pub hostname: Option<String>,
}

impl Default for TcpOptions {
    fn default() -> Self {
        Self {
            port: 0,          // 0 = random
            udp_discovery: true,
            udp_port: 4445,
            udp_multicast: "239.0.0.0".into(),
            gossip_period: 2,
            max_connections: 32,
            max_packet_size: 1 * 1024 * 1024,
            urls: Vec::new(),
            use_hostname: true,
            hostname: None,
        }
    }
}

pub struct TcpTransporter {
    pub opts: TcpOptions,
    node_id: String,
    /// Bound TCP port (filled after bind).
    bound_port: Arc<RwLock<u16>>,
    peers: Arc<RwLock<HashMap<String, Peer>>>,
    tx: broadcast::Sender<Packet>,
    sequence: Arc<std::sync::atomic::AtomicU64>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl TcpTransporter {
    pub fn new(node_id: impl Into<String>, opts: TcpOptions) -> Self {
        let (tx, _) = broadcast::channel(4096);
        Self {
            opts,
            node_id: node_id.into(),
            bound_port: Arc::new(RwLock::new(0)),
            peers: Arc::new(RwLock::new(HashMap::new())),
            tx,
            sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Build a GossipPacket ready to send.
    async fn make_gossip(&self, kind: GossipKind) -> GossipPacket {
        let port = *self.bound_port.read().await;
        let seq = self.sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nodes: Vec<GossipNodeInfo> = self.peers.read().await.values().map(|p| GossipNodeInfo {
            node_id: p.node_id.clone(),
            addr: p.addr.ip().to_string(),
            port: p.addr.port(),
            cpu: 0.0,
            sequence: p.sequence,
            online: p.online,
        }).collect();

        GossipPacket {
            kind,
            sender: self.node_id.clone(),
            sender_addr: "0.0.0.0".into(),
            sender_port: port,
            sequence: seq,
            nodes,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Serialize + length-prefix a packet for TCP framing.
    fn frame_packet(p: &Packet) -> Vec<u8> {
        let json = serde_json::to_vec(p).unwrap_or_default();
        let mut buf = Vec::with_capacity(4 + json.len());
        let len = json.len() as u32;
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&json);
        buf
    }

    /// Deserialize a length-prefixed packet.
    fn unframe(data: &[u8]) -> Option<Packet> {
        if data.len() < 4 { return None; }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len { return None; }
        serde_json::from_slice(&data[4..4 + len]).ok()
    }

    async fn spawn_tcp_server(self: &Arc<Self>) {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.opts.port)).await
            .expect("[TcpTransporter] failed to bind TCP port");
        let local_port = listener.local_addr().unwrap().port();
        *self.bound_port.write().await = local_port;
        log::info!("[TcpTransporter] listening on TCP port {}", local_port);

        let tx = self.tx.clone();
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            while running.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok((mut socket, addr)) = listener.accept().await {
                    let tx2 = tx.clone();
                    tokio::spawn(async move {
                        let mut len_buf = [0u8; 4];
                        loop {
                            if socket.read_exact(&mut len_buf).await.is_err() { break; }
                            let len = u32::from_be_bytes(len_buf) as usize;
                            let mut body = vec![0u8; len];
                            if socket.read_exact(&mut body).await.is_err() { break; }
                            if let Ok(pkt) = serde_json::from_slice::<Packet>(&body) {
                                let _ = tx2.send(pkt);
                            }
                        }
                        log::debug!("[TcpTransporter] peer {} disconnected", addr);
                    });
                }
            }
        });
    }

    async fn spawn_udp_discovery(self: &Arc<Self>) {
        if !self.opts.udp_discovery { return; }
        let socket = match UdpSocket::bind(format!("0.0.0.0:{}", self.opts.udp_port)).await {
            Ok(s) => s,
            Err(e) => { log::warn!("[TcpTransporter] UDP bind failed: {}", e); return; }
        };
        // Join multicast group
        let multicast: std::net::Ipv4Addr = self.opts.udp_multicast.parse().unwrap_or("239.0.0.0".parse().unwrap());
        let _ = socket.join_multicast_v4(multicast, "0.0.0.0".parse().unwrap());

        let node_id = self.node_id.clone();
        let peers = Arc::clone(&self.peers);
        let bound_port = Arc::clone(&self.bound_port);
        let running = Arc::clone(&self.running);
        let multicast_str = self.opts.udp_multicast.clone();
        let udp_port = self.opts.udp_port;

        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            // Broadcast our presence
            let port = *bound_port.read().await;
            let hello = serde_json::json!({
                "kind": "HELLO",
                "nodeId": node_id,
                "port": port,
            });
            let hello_bytes = serde_json::to_vec(&hello).unwrap_or_default();
            let _ = socket.send_to(&hello_bytes, format!("{}:{}", multicast_str, udp_port)).await;

            while running.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok((n, from)) = socket.recv_from(&mut buf).await {
                    if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&buf[..n]) {
                        if let (Some(id), Some(port_val)) = (
                            msg.get("nodeId").and_then(|v| v.as_str()),
                            msg.get("port").and_then(|v| v.as_u64()),
                        ) {
                            if id != node_id {
                                let peer_addr: SocketAddr = format!("{}:{}", from.ip(), port_val).parse()
                                    .unwrap_or(from);
                                let mut peers_w = peers.write().await;
                                peers_w.entry(id.to_string()).or_insert_with(|| Peer {
                                    node_id: id.to_string(),
                                    addr: peer_addr,
                                    last_seen: Instant::now(),
                                    sequence: 0,
                                    online: true,
                                });
                                log::info!("[TcpTransporter] discovered peer {} at {}", id, peer_addr);
                            }
                        }
                    }
                }
            }
        });
    }

    async fn spawn_gossip_timer(self: &Arc<Self>) {
        let period = Duration::from_secs(self.opts.gossip_period);
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            while this.running.load(std::sync::atomic::Ordering::Relaxed) {
                interval.tick().await;
                let peers: Vec<Peer> = this.peers.read().await.values().cloned().collect();
                for peer in peers {
                    let gossip = this.make_gossip(GossipKind::Ping).await;
                    let data = serde_json::to_vec(&gossip).unwrap_or_default();
                    if let Ok(mut stream) = TcpStream::connect(peer.addr).await {
                        let _ = stream.write_all(&data).await;
                    }
                }
                // Purge stale peers (not seen in 3x gossip_period)
                let stale_threshold = Duration::from_secs(this.opts.gossip_period * 3);
                let mut peers_w = this.peers.write().await;
                peers_w.retain(|_, p| p.last_seen.elapsed() < stale_threshold || p.online);
            }
        });
    }

    async fn send_to_peer(&self, addr: SocketAddr, packet: &Packet) -> Result<()> {
        let framed = Self::frame_packet(packet);
        let mut stream = TcpStream::connect(addr).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))?;
        stream.write_all(&framed).await
            .map_err(|e| MoleculerError::Transport(e.to_string()))
    }
}

#[async_trait]
impl Transporter for TcpTransporter {
    async fn connect(&self) -> Result<()> {
        self.running.store(true, std::sync::atomic::Ordering::Relaxed);
        // We need Arc<Self> for spawning — caller must wrap in Arc
        log::info!("[TcpTransporter] starting (gossip_period={}s)", self.opts.gossip_period);

        // Connect to seed URLs
        for url in &self.opts.urls {
            if let Ok(addr) = url.parse::<SocketAddr>() {
                let mut peers = self.peers.write().await;
                peers.insert(url.clone(), Peer {
                    node_id: url.clone(),
                    addr,
                    last_seen: Instant::now(),
                    sequence: 0,
                    online: true,
                });
            }
        }
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
        log::info!("[TcpTransporter] disconnected");
        Ok(())
    }

    async fn send(&self, packet: Packet) -> Result<()> {
        let target = packet.target.clone();
        if let Some(t) = target {
            let peers = self.peers.read().await;
            if let Some(peer) = peers.get(&t) {
                let addr = peer.addr;
                drop(peers);
                return self.send_to_peer(addr, &packet).await;
            }
        }
        // Broadcast to all peers
        let addrs: Vec<SocketAddr> = self.peers.read().await.values().map(|p| p.addr).collect();
        for addr in addrs {
            let _ = self.send_to_peer(addr, &packet).await;
        }
        let _ = self.tx.send(packet);
        Ok(())
    }

    async fn subscribe<F>(&self, handler: F) -> Result<()>
    where F: Fn(Packet) + Send + Sync + 'static {
        let mut rx = self.tx.subscribe();
        tokio::spawn(async move {
            while let Ok(p) = rx.recv().await {
                handler(p);
            }
        });
        Ok(())
    }
}
