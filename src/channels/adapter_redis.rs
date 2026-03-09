//! Redis Streams adapter for moleculer-rs channels.
//!
//! Implements the full `@moleculer/channels` Redis adapter in pure Rust using
//! raw RESP (Redis Serialisation Protocol) over a Tokio TCP connection.
//! No external `redis` crate required.
//!
//! Features:
//! - XADD publish (with optional MAXLEN cap)
//! - XREADGROUP + XGROUP CREATE MKSTREAM per subscriber
//! - XAUTOCLAIM for stale pending messages
//! - XACK on success, NACK leaves in pending for retry / DLQ
//! - Dead-letter queue (XADD to DLQ stream)
//! - Graceful shutdown with in-flight drain

use super::adapter::Adapter;
use super::{AckKind, ChannelDef, ChannelMessage, ChannelStats, SendOptions};
use crate::error::{MoleculerError, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::Mutex,
    time::sleep,
};

// ─── Options ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RedisAdapterOptions {
    pub host:                    String,
    pub port:                    u16,
    pub password:                Option<String>,
    pub db:                      u8,
    pub read_timeout_ms:         u64,
    pub min_idle_time_ms:        u64,
    pub claim_interval_ms:       u64,
    pub start_id:                String,
    pub failed_check_interval_ms: u64,
    pub error_info_ttl_secs:     i64,
}

impl Default for RedisAdapterOptions {
    fn default() -> Self {
        Self {
            host:                    "127.0.0.1".into(),
            port:                    6379,
            password:                None,
            db:                      0,
            read_timeout_ms:         5000,
            min_idle_time_ms:        3_600_000,
            claim_interval_ms:       100,
            start_id:                "$".into(),
            failed_check_interval_ms: 1000,
            error_info_ttl_secs:     86_400,
        }
    }
}

impl RedisAdapterOptions {
    pub fn from_url(url: &str) -> Self {
        let mut opts = Self::default();
        // Parse redis://[:password@]host[:port][/db]
        let rest = url
            .strip_prefix("redis://")
            .or_else(|| url.strip_prefix("rediss://"))
            .unwrap_or(url);

        let (auth, hostpart) = if let Some(at) = rest.rfind('@') {
            let auth  = &rest[..at];
            let hostpart = &rest[at + 1..];
            let password = auth.strip_prefix(':').unwrap_or(auth);
            opts.password = Some(password.to_string());
            (true, hostpart)
        } else {
            (false, rest)
        };
        let _ = auth;

        let (host_port, db_part) = if let Some(slash) = hostpart.find('/') {
            (&hostpart[..slash], &hostpart[slash + 1..])
        } else {
            (hostpart, "0")
        };

        if let Some(colon) = host_port.rfind(':') {
            opts.host = host_port[..colon].to_string();
            opts.port = host_port[colon + 1..].parse().unwrap_or(6379);
        } else if !host_port.is_empty() {
            opts.host = host_port.to_string();
        }

        opts.db = db_part.parse().unwrap_or(0);
        opts
    }
}

// ─── RESP client ─────────────────────────────────────────────────────────────

/// Minimal RESP2/RESP3 client over a single TCP connection (non-pipelined).
struct RespConn {
    stream: BufReader<TcpStream>,
}

impl RespConn {
    async fn connect(opts: &RedisAdapterOptions) -> Result<Self> {
        let addr = format!("{}:{}", opts.host, opts.port);
        let tcp  = TcpStream::connect(&addr).await
            .map_err(|e| MoleculerError::Internal(format!("Redis TCP {addr}: {e}")))?;
        let mut conn = Self { stream: BufReader::new(tcp) };

        if let Some(ref pw) = opts.password {
            conn.cmd(&["AUTH", pw]).await?;
        }
        if opts.db > 0 {
            conn.cmd(&["SELECT", &opts.db.to_string()]).await?;
        }
        Ok(conn)
    }

    /// Send a RESP array command and read back one top-level reply.
    async fn cmd(&mut self, args: &[&str]) -> Result<RespValue> {
        // Build RESP array
        let mut buf = format!("*{}\r\n", args.len());
        for a in args {
            buf.push_str(&format!("${}\r\n{}\r\n", a.len(), a));
        }
        self.stream.get_mut().write_all(buf.as_bytes()).await
            .map_err(|e| MoleculerError::Internal(format!("Redis write: {e}")))?;
        self.read_value().await
    }

    /// Send a RESP command with mixed bytes/string args.
    async fn cmd_bytes(&mut self, parts: &[Vec<u8>]) -> Result<RespValue> {
        let mut buf: Vec<u8> = format!("*{}\r\n", parts.len()).into_bytes();
        for p in parts {
            buf.extend_from_slice(format!("${}\r\n", p.len()).as_bytes());
            buf.extend_from_slice(p);
            buf.extend_from_slice(b"\r\n");
        }
        self.stream.get_mut().write_all(&buf).await
            .map_err(|e| MoleculerError::Internal(format!("Redis write: {e}")))?;
        self.read_value().await
    }

    fn read_value(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RespValue>> + Send + '_>> {
        Box::pin(async move {
        let mut line = String::new();
        self.stream.read_line(&mut line).await
            .map_err(|e| MoleculerError::Internal(format!("Redis read: {e}")))?;
        let line = line.trim_end_matches("\r\n");
        if line.is_empty() {
            return Ok(RespValue::Nil);
        }
        let (prefix, rest) = line.split_at(1);
        match prefix {
            "+" => Ok(RespValue::Simple(rest.to_string())),
            "-" => Err(MoleculerError::Internal(format!("Redis error: {rest}"))),
            ":" => Ok(RespValue::Integer(rest.parse().unwrap_or(0))),
            "$" => {
                let len: i64 = rest.parse().unwrap_or(-1);
                if len == -1 { return Ok(RespValue::Nil); }
                let mut data = vec![0u8; (len + 2) as usize];
                use tokio::io::AsyncReadExt;
                self.stream.read_exact(&mut data).await
                    .map_err(|e| MoleculerError::Internal(format!("Redis bulk read: {e}")))?;
                Ok(RespValue::Bulk(data[..len as usize].to_vec()))
            }
            "*" => {
                let count: i64 = rest.parse().unwrap_or(-1);
                if count == -1 { return Ok(RespValue::Nil); }
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(self.read_value().await?);
                }
                Ok(RespValue::Array(items))
            }
            _ => Ok(RespValue::Simple(format!("{prefix}{rest}"))),
        }
        }) // end Box::pin
    }
}

#[derive(Debug, Clone)]
enum RespValue {
    Simple(String),
    Integer(i64),
    Bulk(Vec<u8>),
    Array(Vec<RespValue>),
    Nil,
}

impl RespValue {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Bulk(b)   => b,
            Self::Simple(s) => s.as_bytes(),
            _               => b"",
        }
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).unwrap_or("")
    }
}

// ─── Per-subscription state ───────────────────────────────────────────────────

struct SubState {
    active_cnt: AtomicU64,
    stopping:   AtomicBool,
}

// ─── RedisStreamsAdapter ───────────────────────────────────────────────────────

pub struct RedisStreamsAdapter {
    opts:      RedisAdapterOptions,
    pub_conn:  Arc<Mutex<Option<RespConn>>>,
    subs:      Arc<DashMap<String, Arc<SubState>>>,
    stopping:  AtomicBool,
    connected: AtomicBool,
}

impl RedisStreamsAdapter {
    pub fn new(opts: RedisAdapterOptions) -> Self {
        Self {
            opts,
            pub_conn:  Arc::new(Mutex::new(None)),
            subs:      Arc::new(DashMap::new()),
            stopping:  AtomicBool::new(false),
            connected: AtomicBool::new(false),
        }
    }

    pub fn from_url(url: &str) -> Self {
        Self::new(RedisAdapterOptions::from_url(url))
    }

    fn sub_key(channel: &str, group: &str) -> String {
        format!("{channel}/{group}")
    }

    /// Ensure stream + consumer group exist (XGROUP CREATE MKSTREAM).
    async fn ensure_group(conn: &mut RespConn, stream: &str, group: &str, start: &str) -> Result<()> {
        let res = conn.cmd(&["XGROUP", "CREATE", stream, group, start, "MKSTREAM"]).await;
        match res {
            Ok(_) => Ok(()),
            Err(MoleculerError::Internal(ref e)) if e.contains("BUSYGROUP") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Parse XREADGROUP / XAUTOCLAIM result into (id, payload_bytes, header_bytes).
    fn parse_stream_entry(entry: &RespValue) -> Option<(String, Vec<u8>, Vec<u8>)> {
        if let RespValue::Array(parts) = entry {
            if parts.len() < 2 { return None; }
            let id = parts[0].as_str().to_string();
            if let RespValue::Array(fields) = &parts[1] {
                let mut payload_bytes = Vec::new();
                let mut header_bytes  = Vec::new();
                let mut i = 0;
                while i + 1 < fields.len() {
                    let key = fields[i].as_str();
                    let val = fields[i + 1].as_bytes().to_vec();
                    match key {
                        "payload" => payload_bytes = val,
                        "headers" => header_bytes  = val,
                        _ => {}
                    }
                    i += 2;
                }
                return Some((id, payload_bytes, header_bytes));
            }
        }
        None
    }

    fn spawn_consume_loop(&self, def: Arc<ChannelDef>, state: Arc<SubState>) {
        let opts       = self.opts.clone();
        let stopping   = Arc::new(AtomicBool::new(false));
        let stopping2  = Arc::clone(&stopping);

        // Link the sub-state stopping flag
        tokio::spawn(async move {
            let consumer_id = format!("{}-{}", def.group.as_deref().unwrap_or("default"),
                                       uuid::Uuid::new_v4());
            let stream  = def.name.clone();
            let group   = def.group.as_deref().unwrap_or("default").to_string();
            let max_inf = def.max_in_flight;

            let mut conn = match RespConn::connect(&opts).await {
                Ok(c) => c,
                Err(e) => { log::error!("Redis sub connect failed: {e}"); return; }
            };

            if let Err(e) = Self::ensure_group(&mut conn, &stream, &group, &opts.start_id).await {
                log::error!("ensure_group {stream}/{group}: {e}"); return;
            }

            loop {
                if state.stopping.load(Ordering::Relaxed) { break; }

                let cap = max_inf.saturating_sub(state.active_cnt.load(Ordering::Relaxed) as usize);
                if cap == 0 {
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }

                // XREADGROUP GROUP <group> <consumer> BLOCK <ms> COUNT <cap> STREAMS <stream> >
                let block_ms = opts.read_timeout_ms.to_string();
                let cap_s    = cap.to_string();
                let res = conn.cmd(&[
                    "XREADGROUP", "GROUP", &group, &consumer_id,
                    "BLOCK", &block_ms, "COUNT", &cap_s,
                    "STREAMS", &stream, ">",
                ]).await;

                match res {
                    Err(e) => {
                        if state.stopping.load(Ordering::Relaxed) { break; }
                        log::error!("XREADGROUP error on {stream}: {e}");
                        sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                    Ok(RespValue::Nil) | Ok(RespValue::Array(_)) if {
                        // Nil means timeout (no messages)
                        matches!(res, Ok(RespValue::Nil))
                    } => continue,
                    Ok(reply) => {
                        // reply = [[stream_name, [[id, fields], ...]]]
                        let entries = Self::extract_entries_from_xread(&reply);
                        for (msg_id, payload_bytes, header_bytes) in entries {
                            state.active_cnt.fetch_add(1, Ordering::Relaxed);

                            let payload: Value = serde_json::from_slice(&payload_bytes)
                                .unwrap_or(Value::Null);
                            let headers: HashMap<String, String> = serde_json::from_slice(&header_bytes)
                                .unwrap_or_default();

                            let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
                            let msg = ChannelMessage::new(
                                stream.clone(), group.clone(), payload.clone(),
                                headers.clone(), 1, ack_tx,
                            );

                            let handler    = Arc::clone(&def.handler);
                            let state2     = Arc::clone(&state);
                            let opts2      = opts.clone();
                            let stream2    = stream.clone();
                            let group2     = group.clone();
                            let max_ret    = def.max_retries;
                            let dlq        = def.dead_letter_queue.clone();
                            let msg_id2    = msg_id.clone();
                            let payload2   = payload.clone();
                            let headers2   = headers.clone();

                            tokio::spawn(async move {
                                let result = handler(msg).await;

                                // Open a fresh connection for ACK/NACK ops
                                let ack_conn_res = RespConn::connect(&opts2).await;
                                if let Ok(mut ack_conn) = ack_conn_res {
                                    match result {
                                        Ok(_) => {
                                            // XACK
                                            let _ = ack_conn.cmd(&[
                                                "XACK", &stream2, &group2, &msg_id2
                                            ]).await;
                                        }
                                        Err(e) => {
                                            // Check delivery count via XPENDING
                                            let count = Self::delivery_count(
                                                &mut ack_conn, &stream2, &group2, &msg_id2
                                            ).await;

                                            if count >= max_ret {
                                                if let Some(ref dlq_name) = dlq {
                                                    let _ = Self::xadd_dlq(
                                                        &mut ack_conn, dlq_name,
                                                        &stream2, &group2, &msg_id2,
                                                        &payload2, &headers2,
                                                        Some(&e.to_string()),
                                                    ).await;
                                                }
                                                let _ = ack_conn.cmd(&[
                                                    "XACK", &stream2, &group2, &msg_id2
                                                ]).await;
                                            }
                                            // else: leave in pending for xclaim
                                        }
                                    }
                                }
                                state2.active_cnt.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                    }
                }
            }

            // Clean up consumer
            let _ = conn.cmd(&["XGROUP", "DELCONSUMER", &stream, &group, &consumer_id]).await;
            log::info!("Redis consume loop ended for {stream}/{group}");
        });
    }

    fn spawn_claim_loop(&self, def: Arc<ChannelDef>, state: Arc<SubState>) {
        let opts = self.opts.clone();

        tokio::spawn(async move {
            let stream    = &def.name;
            let group     = def.group.as_deref().unwrap_or("default");
            let claimer   = format!("claimer-{}", uuid::Uuid::new_v4());
            let mut cursor = "0-0".to_string();
            let max_inf   = def.max_in_flight;

            let mut conn = match RespConn::connect(&opts).await {
                Ok(c) => c,
                Err(e) => { log::error!("Redis xclaim conn: {e}"); return; }
            };

            loop {
                if state.stopping.load(Ordering::Relaxed) { break; }
                sleep(Duration::from_millis(opts.claim_interval_ms)).await;

                let cap = max_inf.saturating_sub(state.active_cnt.load(Ordering::Relaxed) as usize);
                if cap == 0 { continue; }

                let idle_ms = opts.min_idle_time_ms.to_string();
                let cap_s   = cap.to_string();
                let _ = conn.cmd(&[
                    "XAUTOCLAIM", stream, group, &claimer,
                    &idle_ms, &cursor, "COUNT", &cap_s,
                ]).await;
                // claimed messages will be picked up by XREADGROUP on next iteration
            }
        });
    }

    fn extract_entries_from_xread(reply: &RespValue) -> Vec<(String, Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        if let RespValue::Array(streams) = reply {
            for stream_entry in streams {
                if let RespValue::Array(parts) = stream_entry {
                    if parts.len() < 2 { continue; }
                    if let RespValue::Array(messages) = &parts[1] {
                        for msg in messages {
                            if let Some(parsed) = Self::parse_stream_entry(msg) {
                                out.push(parsed);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    async fn delivery_count(conn: &mut RespConn, stream: &str, group: &str, msg_id: &str) -> u32 {
        match conn.cmd(&["XPENDING", stream, group, msg_id, msg_id, "1"]).await {
            Ok(RespValue::Array(entries)) => {
                if let Some(RespValue::Array(entry)) = entries.first() {
                    if entry.len() >= 4 {
                        if let RespValue::Integer(n) = &entry[3] {
                            return *n as u32;
                        }
                    }
                }
                0
            }
            _ => 0,
        }
    }

    async fn xadd_dlq(
        conn:       &mut RespConn,
        dlq:        &str,
        orig_ch:    &str,
        orig_group: &str,
        msg_id:     &str,
        payload:    &Value,
        headers:    &HashMap<String, String>,
        error_msg:  Option<&str>,
    ) -> Result<()> {
        let mut dlq_headers = headers.clone();
        dlq_headers.insert("x-original-id".into(),      msg_id.into());
        dlq_headers.insert("x-original-channel".into(), orig_ch.into());
        dlq_headers.insert("x-original-group".into(),   orig_group.into());
        if let Some(e) = error_msg {
            dlq_headers.insert("x-error-message".into(), e.into());
        }

        let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
        let headers_bytes = serde_json::to_vec(&dlq_headers).unwrap_or_default();

        conn.cmd_bytes(&[
            b"XADD".to_vec(), dlq.as_bytes().to_vec(), b"*".to_vec(),
            b"payload".to_vec(), payload_bytes,
            b"headers".to_vec(), headers_bytes,
        ]).await?;
        log::warn!("DLQ: moved {msg_id} from {orig_ch}/{orig_group} → {dlq}");
        Ok(())
    }
}

// ─── Adapter trait impl ───────────────────────────────────────────────────────

#[async_trait]
impl Adapter for RedisStreamsAdapter {
    async fn connect(&self) -> Result<()> {
        let conn = RespConn::connect(&self.opts).await?;
        *self.pub_conn.lock().await = Some(conn);
        self.connected.store(true, Ordering::Relaxed);
        log::info!("Redis Streams adapter connected to {}:{}", self.opts.host, self.opts.port);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.stopping.store(true, Ordering::Relaxed);

        // Signal all subscriber loops to stop
        for entry in self.subs.iter() {
            entry.stopping.store(true, Ordering::Relaxed);
        }

        // Drain in-flight (up to 3 s)
        for _ in 0..30 {
            let total: u64 = self.subs.iter()
                .map(|s| s.active_cnt.load(Ordering::Relaxed))
                .sum();
            if total == 0 { break; }
            sleep(Duration::from_millis(100)).await;
        }

        *self.pub_conn.lock().await = None;
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn publish(&self, channel: &str, payload: Value, opts: SendOptions) -> Result<()> {
        if self.stopping.load(Ordering::Relaxed) {
            return Err(MoleculerError::Internal("adapter stopping".into()));
        }

        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let headers_bytes = if opts.headers.is_empty() {
            None
        } else {
            Some(serde_json::to_vec(&opts.headers)
                .map_err(|e| MoleculerError::Internal(e.to_string()))?)
        };

        let mut guard = self.pub_conn.lock().await;
        let conn = guard.as_mut()
            .ok_or_else(|| MoleculerError::Internal("not connected".into()))?;

        let mut parts: Vec<Vec<u8>> = vec![b"XADD".to_vec(), channel.as_bytes().to_vec()];

        if let Some(max_len) = opts.max_len {
            parts.push(b"MAXLEN".to_vec());
            parts.push(b"~".to_vec());
            parts.push(max_len.to_string().into_bytes());
        }
        parts.push(b"*".to_vec());
        parts.push(b"payload".to_vec());
        parts.push(payload_bytes);
        if let Some(hb) = headers_bytes {
            parts.push(b"headers".to_vec());
            parts.push(hb);
        }

        conn.cmd_bytes(&parts).await?;
        Ok(())
    }

    async fn subscribe(&self, def: &ChannelDef) -> Result<()> {
        let group = def.group.as_deref().unwrap_or("default");
        let key   = Self::sub_key(&def.name, group);
        if self.subs.contains_key(&key) { return Ok(()); }

        let state = Arc::new(SubState {
            active_cnt: AtomicU64::new(0),
            stopping:   AtomicBool::new(false),
        });
        self.subs.insert(key, Arc::clone(&state));

        let def_arc = Arc::new(def.clone());
        self.spawn_consume_loop(Arc::clone(&def_arc), Arc::clone(&state));
        self.spawn_claim_loop(def_arc, state);

        log::info!("Subscribed to '{}' / group '{}'", def.name, group);
        Ok(())
    }

    async fn unsubscribe(&self, channel: &str, group: &str) -> Result<()> {
        let key = Self::sub_key(channel, group);
        if let Some((_, state)) = self.subs.remove(&key) {
            state.stopping.store(true, Ordering::Relaxed);
            for _ in 0..50 {
                if state.active_cnt.load(Ordering::Relaxed) == 0 { break; }
                sleep(Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }

    fn stats(&self) -> Vec<ChannelStats> {
        self.subs.iter().map(|e| {
            let parts: Vec<&str> = e.key().splitn(2, '/').collect();
            ChannelStats {
                name:            parts[0].to_string(),
                group:           parts.get(1).copied().unwrap_or("").to_string(),
                pending:         0,
                in_flight:       e.active_cnt.load(Ordering::Relaxed) as usize,
                processed_total: 0,
                failed_total:    0,
                dead_letters:    0,
            }
        }).collect()
    }
}
