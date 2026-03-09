//! Channel registry — tracks all subscribed channels per service.

use super::ChannelDef;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ChannelEntry {
    pub service_name: String,
    pub def: ChannelDef,
}

pub struct ChannelRegistry {
    pub entries: DashMap<String, Vec<ChannelEntry>>,
}

impl ChannelRegistry {
    pub fn new() -> Self { Self { entries: DashMap::new() } }

    pub fn register(&self, service_name: &str, def: ChannelDef) {
        let key = format!("{}::{}", def.name, def.group.as_deref().unwrap_or("default"));
        self.entries.entry(key).or_insert_with(Vec::new)
            .push(ChannelEntry { service_name: service_name.to_string(), def });
    }

    pub fn unregister(&self, service_name: &str) {
        self.entries.retain(|_, v| {
            v.retain(|e| e.service_name != service_name);
            !v.is_empty()
        });
    }

    pub fn all(&self) -> Vec<ChannelEntry> {
        self.entries.iter().flat_map(|e| e.value().clone()).collect()
    }
}

impl Default for ChannelRegistry { fn default() -> Self { Self::new() } }
