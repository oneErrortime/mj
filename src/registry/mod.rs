//! Service registry — stores all services, actions, events, and nodes.

pub mod node;
pub mod strategy;

use crate::config::Strategy;
use crate::error::{MoleculerError, Result};
use crate::service::{ActionDef, EventDef, ServiceSchema};
use dashmap::DashMap;
use node::NodeInfo;
use std::sync::atomic::{AtomicUsize, Ordering};
use rand::Rng;

/// A registered action endpoint.
#[derive(Clone, Debug)]
pub struct ActionEndpoint {
    pub service_name: String,
    pub action_name: String,
    pub node_id: String,
    pub is_local: bool,
    pub action: ActionDef,
}

/// A registered event endpoint.
#[derive(Clone)]
pub struct EventEndpoint {
    pub service_name: String,
    pub event_name: String,
    pub node_id: String,
    pub is_local: bool,
    pub group: Option<String>,
    pub handler: crate::service::EventHandler,
}

/// The central service registry.
pub struct ServiceRegistry {
    pub node_id: String,
    pub nodes: DashMap<String, NodeInfo>,
    pub actions: DashMap<String, Vec<ActionEndpoint>>,
    pub events: DashMap<String, Vec<EventEndpoint>>,
    pub services: DashMap<String, ServiceSchema>,
    strategy: Strategy,
    rr_counters: DashMap<String, AtomicUsize>,
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
        };
        reg.nodes.insert(node_id.clone(), NodeInfo::local(node_id));
        reg
    }

    pub fn register_local_service(&self, schema: &ServiceSchema) {
        let full_name = schema.full_name();
        self.services.insert(full_name.clone(), schema.clone());

        for (action_name, action_def) in &schema.actions {
            let key = format!("{}.{}", full_name, action_name);
            let ep = ActionEndpoint {
                service_name: full_name.clone(),
                action_name: action_name.clone(),
                node_id: self.node_id.clone(),
                is_local: true,
                action: action_def.clone(),
            };
            self.actions.entry(key).or_insert_with(Vec::new).push(ep);
        }

        for (event_name, event_def) in &schema.events {
            let ep = EventEndpoint {
                service_name: full_name.clone(),
                event_name: event_name.clone(),
                node_id: self.node_id.clone(),
                is_local: true,
                group: event_def.group.clone(),
                handler: event_def.handler.clone(),
            };
            self.events
                .entry(event_name.clone())
                .or_insert_with(Vec::new)
                .push(ep);
        }
    }

    pub fn deregister_service(&self, full_name: &str) {
        self.services.remove(full_name);
        self.actions.retain(|k, _| !k.starts_with(&format!("{}.", full_name)));
        self.events.iter_mut().for_each(|mut entry| {
            entry.retain(|ep| ep.service_name != full_name);
        });
    }

    pub fn resolve_action(&self, action_key: &str) -> Result<ActionEndpoint> {
        let endpoints = self
            .actions
            .get(action_key)
            .ok_or_else(|| MoleculerError::ActionNotFound(action_key.to_string()))?;

        if endpoints.is_empty() {
            return Err(MoleculerError::ActionNotFound(action_key.to_string()));
        }

        let ep = match self.strategy {
            Strategy::RoundRobin => {
                let counter = self
                    .rr_counters
                    .entry(action_key.to_string())
                    .or_insert_with(|| AtomicUsize::new(0));
                let idx = counter.fetch_add(1, Ordering::Relaxed) % endpoints.len();
                endpoints[idx].clone()
            }
            Strategy::Random => {
                let idx = rand::thread_rng().gen_range(0..endpoints.len());
                endpoints[idx].clone()
            }
            _ => endpoints[0].clone(),
        };

        Ok(ep)
    }

    pub fn resolve_event_listeners(&self, event_name: &str) -> Vec<EventEndpoint> {
        self.events
            .get(event_name)
            .map(|eps| eps.clone())
            .unwrap_or_default()
    }

    pub fn service_names(&self) -> Vec<String> {
        self.services.iter().map(|e| e.key().clone()).collect()
    }

    pub fn has_action(&self, key: &str) -> bool {
        self.actions.get(key).map_or(false, |v| !v.is_empty())
    }
}
