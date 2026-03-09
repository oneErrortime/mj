//! Circuit Breaker middleware — full CLOSED → HALF_OPEN → OPEN state machine.
//!
//! Mirrors the logic from Moleculer's `src/middlewares/circuit-breaker.js`.
//!
//! ## State transitions
//!
//! ```text
//!  CLOSED ──(failures >= threshold)──► OPEN
//!    ▲                                  │
//!    │  (first request succeeds)        │ (half_open_time elapsed)
//!    │                                  ▼
//!    └──────────────────────────── HALF_OPEN
//!        (HALF_OPEN_WAIT: first req
//!         allowed through; others
//!         are blocked until it resolves)
//! ```

use super::{Middleware, NextHandler};
use crate::config::CircuitBreakerConfig;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CbState {
    Closed,
    HalfOpen,
    HalfOpenWait,
    Open,
}

#[derive(Debug)]
struct EndpointState {
    state: CbState,
    /// Request count in current window.
    count: u32,
    /// Failure count in current window.
    failures: u32,
    /// When the circuit was tripped open.
    opened_at: Option<Instant>,
}

impl EndpointState {
    fn new() -> Self {
        Self { state: CbState::Closed, count: 0, failures: 0, opened_at: None }
    }

    /// Check if this endpoint is currently available for requests.
    fn is_available(&self) -> bool {
        self.state == CbState::Closed || self.state == CbState::HalfOpen
    }
}

pub struct CircuitBreakerMiddleware {
    config: CircuitBreakerConfig,
    store: Arc<DashMap<String, Mutex<EndpointState>>>,
}

impl CircuitBreakerMiddleware {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self { config, store: Arc::new(DashMap::new()) }
    }

    fn get_or_create(&self, key: &str) -> &DashMap<String, Mutex<EndpointState>> {
        if !self.store.contains_key(key) {
            self.store.insert(key.to_string(), Mutex::new(EndpointState::new()));
        }
        &self.store
    }

    fn check_and_transition(&self, key: &str) -> std::result::Result<bool, MoleculerError> {
        let store = self.get_or_create(key);
        let entry = store.get(key).unwrap();
        let mut state = entry.lock().unwrap();

        match state.state {
            CbState::Closed => Ok(true),
            CbState::Open => {
                // Check if half_open_time has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed().as_millis() >= self.config.half_open_time as u128 {
                        log::info!("[CB] '{}' transitioning OPEN → HALF_OPEN", key);
                        state.state = CbState::HalfOpen;
                        return Ok(true);
                    }
                }
                Err(MoleculerError::CircuitBreakerOpen(key.to_string()))
            }
            CbState::HalfOpen => {
                // First request through — transition to waiting
                state.state = CbState::HalfOpenWait;
                Ok(true)
            }
            CbState::HalfOpenWait => {
                // Reject until the single test request resolves
                Err(MoleculerError::CircuitBreakerOpen(key.to_string()))
            }
        }
    }

    fn record_success(&self, key: &str) {
        if let Some(entry) = self.store.get(key) {
            let mut state = entry.lock().unwrap();
            state.count += 1;
            if state.state == CbState::HalfOpenWait {
                // Probe succeeded — close the circuit
                log::info!("[CB] '{}' HALF_OPEN → CLOSED (probe succeeded)", key);
                state.state = CbState::Closed;
                state.count = 0;
                state.failures = 0;
                state.opened_at = None;
            }
        }
    }

    fn record_failure(&self, key: &str) {
        if let Some(entry) = self.store.get(key) {
            let mut state = entry.lock().unwrap();
            state.count += 1;
            state.failures += 1;

            if state.state == CbState::HalfOpenWait {
                // Probe failed — reopen
                log::warn!("[CB] '{}' HALF_OPEN → OPEN (probe failed)", key);
                state.state = CbState::Open;
                state.opened_at = Some(Instant::now());
                return;
            }

            // Check threshold in CLOSED state
            if state.state == CbState::Closed
                && state.count >= self.config.min_request_count
            {
                let rate = state.failures as f64 / state.count as f64;
                if rate >= self.config.threshold {
                    log::warn!(
                        "[CB] '{}' CLOSED → OPEN (failure rate {:.1}% >= threshold {:.1}%)",
                        key, rate * 100.0, self.config.threshold * 100.0
                    );
                    state.state = CbState::Open;
                    state.opened_at = Some(Instant::now());
                }
            }
        }
    }

    /// Snapshot of all endpoint states (for Laboratory).
    pub fn snapshot(&self) -> Vec<(String, CbState, u32, u32)> {
        self.store.iter().map(|e| {
            let s = e.value().lock().unwrap();
            (e.key().clone(), s.state, s.count, s.failures)
        }).collect()
    }
}

#[async_trait]
impl Middleware for CircuitBreakerMiddleware {
    fn name(&self) -> &str { "CircuitBreaker" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        if !self.config.enabled {
            return next(ctx).await;
        }

        let key = ctx.action.clone().unwrap_or_default();

        // Check if we can let the request through
        self.check_and_transition(&key)?;

        let result = next(ctx).await;

        match &result {
            Ok(_) => self.record_success(&key),
            Err(e) if e.is_retryable() => self.record_failure(&key),
            Err(_) => self.record_failure(&key),
        }

        result
    }
}
