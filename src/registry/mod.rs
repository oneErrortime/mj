//! Service Registry — local service catalog with load balancing, endpoint tracking,
//! and topology graph (who calls whom).

pub mod node;
pub mod strategy;

use crate::config::Strategy;
use crate::error::{MoleculerError, Result};
use crate::service::{ActionDef, EventDef, ServiceSchema};
use dashmap::DashMap;
use node::NodeInfo;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use rand::Rng;
use chrono::{DateTime, Utc};

// ─── Endpoint types ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ActionEndpoint {
    pub service_name: String,
    pub action_name: String,
    pub node_id: String,
    pub is_local: bool,
    pub action: ActionDef,
    /// Is this endpoint available (false when circuit is open).
    pub available: bool,
}

#[derive(Clone)]
pub struct EventEndpoint {
    pub service_name: String,
    pub event_name: String,
    pub node_id: String,
    pub is_local: bool,
    pub group: Option<String>,
    pub handler: crate::service::EventHandler,
}

// ─── Topology ────────────────────────────────────────────────────────────────

/// A single edge in the service call graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub from: String,
    pub to: String,
    pub req_per_min: f64,
    pub avg_latency_ms: f64,
    pub last_called: DateTime<Utc>,
}

/// Call stats per edge (not serialized directly).
struct EdgeStats {
    count: AtomicU64,
    total_latency_us: AtomicU64,
    last_called: std::sync::Mutex<DateTime<Utc>>,
}

// ─── ServiceRegistry ─────────────────────────────────────────────────────────

pub struct ServiceRegistry {
    pub node_id: String,
    pub nodes: DashMap<String, NodeInfo>,
    pub actions: DashMap<String, Vec<ActionEndpoint>>,
    pub events: DashMap<String, Vec<EventEndpoint>>,
    pub services: DashMap<String, ServiceSchema>,
    strategy: Strategy,
    rr_counters: DashMap<String, AtomicUsize>,
    /// Topology: "from::to" → EdgeStats
    topology: Arc<DashMap<String, EdgeStats>>,
}

impl ServiceRegistry {
    pub fn new(node_id: impl Into<String>, strategy: Strategy) -> Self {
        let node_id = node_id.into();
        let reg = Self {
            node_id: node_id.clone(),
            nodes: DashMap::new(),
            actions: DashMap::new(),
            events: DashMap::new(),
            services: DashMap::new(),
            strategy,
            rr_counters: DashMap::new(),
            topology: Arc::new(DashMap::new()),
        };
        reg.nodes.insert(node_id.clone(), NodeInfo::local(node_id));
        reg
    }

    pub fn register_local_service(&self, schema: &ServiceSchema) {
        let resolved = schema.merged();
        let full_name = resolved.full_name();
        self.services.insert(full_name.clone(), resolved.clone());

        for (action_name, action_def) in &resolved.actions {
            let key = format!("{}.{}", full_name, action_name);
            let ep = ActionEndpoint {
                service_name: full_name.clone(),
                action_name: action_name.clone(),
                node_id: self.node_id.clone(),
                is_local: true,
                action: action_def.clone(),
                available: true,
            };
            self.actions.entry(key).or_insert_with(Vec::new).push(ep);
        }

        for (event_name, event_def) in &resolved.events {
            let ep = EventEndpoint {
                service_name: full_name.clone(),
                event_name: event_name.clone(),
                node_id: self.node_id.clone(),
                is_local: true,
                group: event_def.group.clone(),
                handler: event_def.handler.clone(),
            };
            self.events.entry(event_name.clone()).or_insert_with(Vec::new).push(ep);
        }
    }

    pub fn deregister_service(&self, full_name: &str) {
        self.services.remove(full_name);
        self.actions.retain(|k, _| !k.starts_with(&format!("{}.", full_name)));
        self.events.iter_mut().for_each(|mut e| e.retain(|ep| ep.service_name != full_name));
    }

    pub fn resolve_action(&self, action_key: &str) -> Result<ActionEndpoint> {
        let eps = self.actions.get(action_key)
            .ok_or_else(|| MoleculerError::ActionNotFound(action_key.to_string()))?;

        let available: Vec<&ActionEndpoint> = eps.iter().filter(|ep| ep.available).collect();
        if available.is_empty() {
            return Err(MoleculerError::ActionNotFound(action_key.to_string()));
        }

        let ep = match self.strategy {
            Strategy::RoundRobin => {
                let ctr = self.rr_counters.entry(action_key.to_string()).or_insert_with(|| AtomicUsize::new(0));
                let idx = ctr.fetch_add(1, Ordering::Relaxed) % available.len();
                available[idx].clone()
            }
            Strategy::Random => {
                let idx = rand::thread_rng().gen_range(0..available.len());
                available[idx].clone()
            }
            Strategy::Shard => {
                // Consistent hashing by action key
                let hash = {
                    let mut h = 0usize;
                    for b in action_key.bytes() { h = h.wrapping_mul(31).wrapping_add(b as usize); }
                    h
                };
                available[hash % available.len()].clone()
            }
            _ => available[0].clone(),
        };

        Ok(ep)
    }

    pub fn resolve_event_listeners(&self, event_name: &str) -> Vec<EventEndpoint> {
        self.events.get(event_name).map(|v| v.clone()).unwrap_or_default()
    }

    /// Record a call in the topology graph.
    pub fn record_topology_call(
        &self,
        from: &str,
        to: &str,
        latency_us: u64,
    ) {
        let key = format!("{}::{}", from, to);
        let entry = self.topology.entry(key).or_insert_with(|| EdgeStats {
            count: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            last_called: std::sync::Mutex::new(Utc::now()),
        });
        entry.count.fetch_add(1, Ordering::Relaxed);
        entry.total_latency_us.fetch_add(latency_us, Ordering::Relaxed);
        *entry.last_called.lock().unwrap() = Utc::now();
    }

    /// Snapshot of topology edges for the Lab frontend.
    pub fn topology_snapshot(&self) -> Vec<TopologyEdge> {
        self.topology.iter().map(|e| {
            let parts: Vec<&str> = e.key().splitn(2, "::").collect();
            let (from, to) = if parts.len() == 2 { (parts[0], parts[1]) } else { ("?", "?") };
            let count = e.count.load(Ordering::Relaxed);
            let total_us = e.total_latency_us.load(Ordering::Relaxed);
            let avg_latency_ms = if count > 0 { (total_us as f64 / count as f64) / 1000.0 } else { 0.0 };
            // Very rough req/min (assume window = 1 min for simplicity)
            let req_per_min = count as f64; // reset would give actual rate

            TopologyEdge {
                from: from.to_string(),
                to: to.to_string(),
                req_per_min,
                avg_latency_ms,
                last_called: *e.last_called.lock().unwrap(),
            }
        }).collect()
    }

    pub fn service_names(&self) -> Vec<String> { self.services.iter().map(|e| e.key().clone()).collect() }
    pub fn has_action(&self, key: &str) -> bool { self.actions.get(key).map_or(false, |v| !v.is_empty()) }
    pub fn set_endpoint_available(&self, action_key: &str, available: bool) {
        if let Some(mut eps) = self.actions.get_mut(action_key) {
            for ep in eps.iter_mut() { ep.available = available; }
        }
    }

    // ─── Transit helpers ──────────────────────────────────────────────────

    /// Register (or update) a remote node's service list received via INFO packet.
    pub fn register_remote_node(&self, node_id: String, services: serde_json::Value) {
        // Update / insert the NodeInfo entry
        let info = NodeInfo {
            node_id: node_id.clone(),
            ip_list: vec![],
            hostname: String::new(),
            client: node::ClientInfo { lang_type: "unknown".into(), version: "0".into(), lang_version: "0".into() },
            seq: 0,
            instance_id: String::new(),
            metadata: serde_json::Value::Null,
            available: true,
            last_heartbeat: chrono::Utc::now(),
            cpu: 0.0,
        };
        self.nodes.insert(node_id.clone(), info);

        // Register remote action endpoints from the services list
        if let Some(arr) = services.as_array() {
            for svc in arr {
                let svc_name = svc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if let Some(actions) = svc.get("actions").and_then(|v| v.as_object()) {
                    for action_name in actions.keys() {
                        let full_key = format!("{}.{}", svc_name, action_name);
                        // Create a stub remote endpoint — real handler is never called locally
                        let stub_action = ActionDef::new(&full_key, |_ctx| {
                            Box::pin(async { Err(crate::error::MoleculerError::Internal("remote stub".into())) })
                        });
                        let ep = ActionEndpoint {
                            service_name: svc_name.clone(),
                            action_name: full_key.clone(),
                            node_id: node_id.clone(),
                            is_local: false,
                            action: stub_action,
                            available: true,
                        };
                        self.actions.entry(full_key).or_default().push(ep);
                    }
                }
            }
        }
    }

    /// Remove all endpoints registered for a remote node (on DISCONNECT).
    pub fn unregister_node(&self, node_id: &str) {
        self.nodes.remove(node_id);
        // Remove remote endpoints for this node
        for mut entry in self.actions.iter_mut() {
            entry.value_mut().retain(|ep| ep.node_id != node_id);
        }
        for mut entry in self.events.iter_mut() {
            entry.value_mut().retain(|ep| ep.node_id != node_id);
        }
    }

    /// Update the heartbeat timestamp for a remote node.
    pub fn heartbeat_received(&self, node_id: &str) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            node.last_heartbeat = chrono::Utc::now();
            node.available = true;
        }
    }
}

    /// Find the first available endpoint for an action (for validation / introspection).
    pub fn find_action(&self, action_key: &str) -> Option<ActionEndpoint> {
        self.actions.get(action_key).and_then(|v| v.first().cloned())
    }
}

pub mod discoverers;
pub use discoverers::{Discoverer, LocalDiscoverer, RedisDiscoverer, Etcd3Discoverer};
