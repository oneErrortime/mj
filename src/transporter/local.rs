use super::{Packet, Transporter};
use crate::error::Result;
use async_trait::async_trait;
use tokio::sync::broadcast;

pub struct LocalTransporter { tx: broadcast::Sender<Packet> }
impl LocalTransporter { pub fn new() -> Self { let (tx, _) = broadcast::channel(1024); Self { tx } } }
impl Default for LocalTransporter { fn default() -> Self { Self::new() } }
#[async_trait]
impl Transporter for LocalTransporter {
    async fn connect(&self) -> Result<()> { log::debug!("[LocalTransporter] connected"); Ok(()) }
    async fn disconnect(&self) -> Result<()> { Ok(()) }
    async fn send(&self, packet: Packet) -> Result<()> { let _ = self.tx.send(packet); Ok(()) }
    async fn subscribe<F>(&self, handler: F) -> Result<()> where F: Fn(Packet) + Send + Sync + 'static {
        let mut rx = self.tx.subscribe();
        tokio::spawn(async move { while let Ok(p) = rx.recv().await { handler(p); } });
        Ok(())
    }
}
