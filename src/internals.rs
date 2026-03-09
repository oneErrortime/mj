//! Built-in `$node` service.
//!
//! Mirrors `moleculer/src/internals.js`.
//!
//! Provides cluster introspection actions:
//! - `$node.list`        — list known nodes
//! - `$node.services`   — list services (with optional filtering)
//! - `$node.actions`    — list all actions
//! - `$node.events`     — list all events
//! - `$node.health`     — local node health
//! - `$node.options`    — broker options
//! - `$node.metrics`    — metrics snapshot

use crate::broker::ServiceBroker;
use crate::context::Context;
use crate::error::Result;
use crate::service::{ActionDef, ServiceSchema};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_node_service(broker: Arc<ServiceBroker>) -> ServiceSchema {
    let b1 = Arc::clone(&broker);
    let b2 = Arc::clone(&broker);
    let b3 = Arc::clone(&broker);
    let b4 = Arc::clone(&broker);
    let b5 = Arc::clone(&broker);
    let b6 = Arc::clone(&broker);

    ServiceSchema::new("$node")
        // ── $node.list ────────────────────────────────────────────
        .action(ActionDef::new("list", move |_ctx: Context| {
            let b = Arc::clone(&b1);
            async move {
                let nodes: Vec<Value> = b.registry.nodes.iter().map(|e| {
                    json!({
                        "id":        e.key(),
                        "available": e.value().available,
                        "local":     e.value().is_local,
                        "hostname":  e.value().hostname,
                        "ipList":    e.value().ip_list,
                        "seq":       e.value().seq,
                    })
                }).collect();
                Ok(json!(nodes))
            }
        }))
        // ── $node.services ────────────────────────────────────────
        .action(ActionDef::new("services", move |ctx: Context| {
            let b = Arc::clone(&b2);
            async move {
                let with_actions = ctx.params.get("withActions").and_then(|v| v.as_bool()).unwrap_or(false);
                let with_events  = ctx.params.get("withEvents") .and_then(|v| v.as_bool()).unwrap_or(false);

                let services: Vec<Value> = b.registry.services.iter().map(|e| {
                    let svc = e.value();
                    let mut obj = json!({
                        "name":    &svc.name,
                        "version": svc.version.as_deref().unwrap_or(""),
                        "nodeID":  b.node_id,
                    });
                    if with_actions {
                        let actions: Vec<&str> = svc.actions.iter().map(|a| a.name.as_str()).collect();
                        obj["actions"] = json!(actions);
                    }
                    if with_events {
                        let events: Vec<&str> = svc.events.iter().map(|e| e.name.as_str()).collect();
                        obj["events"] = json!(events);
                    }
                    obj
                }).collect();
                Ok(json!(services))
            }
        }))
        // ── $node.actions ────────────────────────────────────────
        .action(ActionDef::new("actions", move |ctx: Context| {
            let b = Arc::clone(&b3);
            async move {
                let skip_internal = ctx.params.get("skipInternal").and_then(|v| v.as_bool()).unwrap_or(false);
                let actions: Vec<Value> = b.registry.actions.iter()
                    .filter(|e| !skip_internal || !e.key().starts_with('$'))
                    .flat_map(|e| {
                        e.value().iter().map(|ep| json!({
                            "name":    &ep.action_name,
                            "service": &ep.service_name,
                            "nodeID":  &ep.node_id,
                        })).collect::<Vec<_>>()
                    })
                    .collect();
                Ok(json!(actions))
            }
        }))
        // ── $node.events ─────────────────────────────────────────
        .action(ActionDef::new("events", move |_ctx: Context| {
            let b = Arc::clone(&b4);
            async move {
                let events: Vec<Value> = b.registry.events.iter()
                    .flat_map(|e| {
                        e.value().iter().map(|ep| json!({
                            "name":    &ep.event_name,
                            "service": &ep.service_name,
                            "nodeID":  &ep.node_id,
                            "group":   ep.group.as_deref().unwrap_or(""),
                        })).collect::<Vec<_>>()
                    })
                    .collect();
                Ok(json!(events))
            }
        }))
        // ── $node.health ─────────────────────────────────────────
        .action(ActionDef::new("health", move |_ctx: Context| {
            let b = Arc::clone(&b5);
            async move {
                Ok(json!({
                    "nodeID": b.node_id,
                    "instanceID": b.instance_id,
                    "uptime": 0, // Would track start time
                    "services": b.registry.services.len(),
                    "actions": b.registry.actions.len(),
                    "events": b.registry.events.len(),
                }))
            }
        }))
        // ── $node.options ─────────────────────────────────────────
        .action(ActionDef::new("options", move |_ctx: Context| {
            let b = Arc::clone(&b6);
            async move {
                let cfg = &b.config;
                Ok(json!({
                    "namespace":        cfg.namespace,
                    "nodeID":           b.node_id,
                    "requestTimeout":   cfg.request_timeout,
                    "maxCallLevel":     cfg.max_call_level,
                    "heartbeatInterval":cfg.heartbeat_interval,
                    "preferLocal":      cfg.prefer_local,
                    "strategy":         format!("{:?}", cfg.strategy),
                    "circuitBreaker":   cfg.circuit_breaker.enabled,
                    "retry":            cfg.retry.enabled,
                    "bulkhead":         cfg.bulkhead.enabled,
                    "cacher":           cfg.cacher.enabled,
                    "channels":         cfg.channels.enabled,
                    "metrics":          cfg.metrics.enabled,
                    "tracing":          cfg.tracing.enabled,
                }))
            }
        }))
}
