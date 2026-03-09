use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub hostname: String,
    pub ip_list: Vec<String>,
    pub is_local: bool,
    pub available: bool,
    pub last_heartbeat: DateTime<Utc>,
    pub cpu: f64,
    pub latency_ms: u64,
    pub services: Vec<String>,
    pub version: String,
    pub instance_id: String,
}
impl NodeInfo {
    pub fn local(id: impl Into<String>) -> Self {
        let hostname = std::process::Command::new("hostname")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            id: id.into(),
            hostname,
            ip_list: Vec::new(),
            is_local: true,
            available: true,
            last_heartbeat: Utc::now(),
            cpu: 0.0,
            latency_ms: 0,
            services: Vec::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            instance_id: uuid::Uuid::new_v4().to_string(),
        }
    }
    pub fn update_heartbeat(&mut self) { self.last_heartbeat = Utc::now(); self.available = true; }
    pub fn mark_unavailable(&mut self) { self.available = false; }
}
