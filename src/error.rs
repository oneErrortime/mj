//! Error types for moleculer-rs.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MoleculerError>;

#[derive(Debug, Error)]
pub enum MoleculerError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Action not found: {0}")]
    ActionNotFound(String),

    #[error("Request timeout after {0}ms")]
    RequestTimeout(u64),

    #[error("Circuit breaker is open for service: {0}")]
    CircuitBreakerOpen(String),

    #[error("Max retries reached ({0} attempts)")]
    MaxRetriesReached(u32),

    #[error("Node not available: {0}")]
    NodeNotAvailable(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Queue is full (max {0} pending requests)")]
    QueueFull(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
