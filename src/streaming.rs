//! Action streaming — chunked response delivery.
//!
//! Mirrors `moleculer/src/transit.js` stream handling and the
//! `broker.call()` streaming API from Moleculer.js.
//!
//! In Moleculer JS, actions can return a Node.js readable stream.
//! In moleculer-rs, they return an `async_stream::stream!{}` or
//! `tokio::sync::mpsc::Receiver<Chunk>`.
//!
//! ## Architecture
//!
//! ```text
//! Producer (action handler)
//!   └─ StreamSender::send(chunk) → channel (tokio mpsc)
//!
//! Consumer (caller)
//!   └─ ActionStream::next() → Option<Chunk>
//!   └─ ActionStream::collect_all() → Vec<u8>
//! ```
//!
//! On the network, each chunk becomes a separate RESPONSE packet with
//! `seq` and `total_chunks` fields set. The transit layer reassembles
//! them in order.

use crate::error::{MoleculerError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use std::task::{Context as StdCtx, Poll};
use tokio::sync::mpsc;
use futures::Stream;

// ─── Chunk ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Sequence number (1-based).
    pub seq: u32,
    /// Raw payload bytes.
    pub data: Vec<u8>,
    /// True if this is the final chunk (EOF).
    pub eof: bool,
    /// Optional metadata.
    pub meta: Option<Value>,
}

impl StreamChunk {
    pub fn data(seq: u32, data: Vec<u8>) -> Self {
        Self { seq, data, eof: false, meta: None }
    }

    pub fn eof(seq: u32) -> Self {
        Self { seq, data: Vec::new(), eof: true, meta: None }
    }

    pub fn text(seq: u32, s: &str) -> Self {
        Self::data(seq, s.as_bytes().to_vec())
    }
}

// ─── ActionStream ─────────────────────────────────────────────────────────────

/// Async stream of chunks returned by a streaming action.
///
/// ```rust,no_run
/// let stream = broker.call_stream("files.download", json!({ "path": "/data.csv" })).await?;
/// while let Some(chunk) = stream.next().await {
///     process(chunk?.data);
/// }
/// ```
pub struct ActionStream {
    rx: mpsc::Receiver<Result<StreamChunk>>,
}

impl ActionStream {
    pub fn new(rx: mpsc::Receiver<Result<StreamChunk>>) -> Self { Self { rx } }

    /// Collect all chunks into a single byte buffer.
    pub async fn collect_all(mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        while let Some(chunk) = self.rx.recv().await {
            let c = chunk?;
            if c.eof { break; }
            buf.extend_from_slice(&c.data);
        }
        Ok(buf)
    }

    /// Collect all chunks and parse as UTF-8 string.
    pub async fn collect_string(self) -> Result<String> {
        let bytes = self.collect_all().await?;
        String::from_utf8(bytes).map_err(|e| MoleculerError::Internal(e.to_string()))
    }

    /// Collect all chunks and parse as JSON.
    pub async fn collect_json(self) -> Result<Value> {
        let s = self.collect_string().await?;
        serde_json::from_str(&s).map_err(Into::into)
    }

    pub async fn next(&mut self) -> Option<Result<StreamChunk>> {
        self.rx.recv().await
    }
}

impl Stream for ActionStream {
    type Item = Result<StreamChunk>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut StdCtx<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

// ─── StreamSender ─────────────────────────────────────────────────────────────

/// Producer end of a stream — used inside action handlers.
///
/// ```rust,no_run
/// ActionDef::new_stream("files.download", |ctx, mut sender| async move {
///     let data = read_file(&ctx.params["path"]).await?;
///     for chunk in data.chunks(65536) {
///         sender.send(chunk.to_vec()).await?;
///     }
///     sender.finish().await
/// })
/// ```
pub struct StreamSender {
    tx: mpsc::Sender<Result<StreamChunk>>,
    seq: u32,
}

impl StreamSender {
    pub fn new(tx: mpsc::Sender<Result<StreamChunk>>) -> Self { Self { tx, seq: 0 } }

    /// Send a data chunk.
    pub async fn send(&mut self, data: Vec<u8>) -> Result<()> {
        self.seq += 1;
        self.tx.send(Ok(StreamChunk::data(self.seq, data))).await
            .map_err(|_| MoleculerError::Internal("stream receiver dropped".into()))
    }

    /// Send a string chunk.
    pub async fn send_str(&mut self, s: &str) -> Result<()> {
        self.send(s.as_bytes().to_vec()).await
    }

    /// Send a JSON chunk.
    pub async fn send_json(&mut self, v: &Value) -> Result<()> {
        self.send(serde_json::to_vec(v)?).await
    }

    /// Send an error to the stream consumer.
    pub async fn error(self, err: MoleculerError) -> Result<()> {
        self.tx.send(Err(err)).await
            .map_err(|_| MoleculerError::Internal("stream receiver dropped".into()))
    }

    /// Signal end-of-stream.
    pub async fn finish(mut self) -> Result<()> {
        self.seq += 1;
        self.tx.send(Ok(StreamChunk::eof(self.seq))).await
            .map_err(|_| MoleculerError::Internal("stream receiver dropped".into()))
    }
}

// ─── Streaming action builder ─────────────────────────────────────────────────

/// Create an (ActionStream, StreamSender) pair with a given buffer size.
pub fn stream_channel(buffer: usize) -> (ActionStream, StreamSender) {
    let (tx, rx) = mpsc::channel(buffer);
    (ActionStream::new(rx), StreamSender::new(tx))
}

// ─── Bytes-to-lines transformer ───────────────────────────────────────────────

/// Wrap an ActionStream and yield one line at a time (splitting on `\n`).
///
/// Useful for consuming streaming text responses like log tails.
pub struct LineStream {
    inner: ActionStream,
    buf: String,
}

impl LineStream {
    pub fn new(inner: ActionStream) -> Self { Self { inner, buf: String::new() } }

    pub async fn next_line(&mut self) -> Option<Result<String>> {
        loop {
            // Check if there's a complete line in the buffer
            if let Some(pos) = self.buf.find('\n') {
                let line = self.buf[..pos].to_string();
                self.buf.drain(..=pos);
                return Some(Ok(line));
            }
            // Read more data
            match self.inner.next().await {
                None => {
                    if self.buf.is_empty() { return None; }
                    let line = std::mem::take(&mut self.buf);
                    return Some(Ok(line));
                }
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(chunk)) => {
                    if chunk.eof { continue; }
                    if let Ok(s) = std::str::from_utf8(&chunk.data) {
                        self.buf.push_str(s);
                    }
                }
            }
        }
    }
}
