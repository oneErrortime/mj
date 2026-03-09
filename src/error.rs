//! Error types — mirrors Moleculer's error hierarchy.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MoleculerError>;

#[derive(Debug, Error)]
pub enum MoleculerError {
    // ------ Request / routing errors ------
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Action not found: {0}")]
    ActionNotFound(String),

    #[error("Request timeout after {0}ms")]
    RequestTimeout(u64),

    #[error("Circuit breaker is open for '{0}'")]
    CircuitBreakerOpen(String),

    #[error("Max call level ({0}) reached")]
    MaxCallLevelReached(u32),

    #[error("Max retries reached ({0} attempts)")]
    MaxRetriesReached(u32),

    #[error("Node not available: {0}")]
    NodeNotAvailable(String),

    // ------ Bulkhead ------
    #[error("Queue is full (max {0} pending requests)")]
    QueueFull(usize),

    // ------ Channels ------
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Channel delivery failed after {0} retries — moved to DLQ")]
    DeadLetter(u32),

    #[error("Channel adapter error: {0}")]
    ChannelAdapter(String),

    // ------ Validation ------
    #[error("Validation error: {message}")]
    Validation {
        message: String,
        /// Individual field failures (serialised `ValidationFailure` objects).
        failures: Vec<serde_json::Value>,
    },

    // ------ Serialization ------
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    // ------ Transport ------
    #[error("Transport error: {0}")]
    Transport(String),

    // ------ Cache ------
    #[error("Cache error: {0}")]
    Cache(String),

    // ------ Generic ------
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Retryable error: {0}")]
    Retryable(String),
}

impl MoleculerError {
    /// Whether this error should trigger retry logic.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            MoleculerError::Retryable(_)
                | MoleculerError::RequestTimeout(_)
                | MoleculerError::NodeNotAvailable(_)
                | MoleculerError::Transport(_)
        )
    }

    /// Numeric error code (mirrors Moleculer JS codes).
    pub fn code(&self) -> u32 {
        match self {
            MoleculerError::ServiceNotFound(_) => 404,
            MoleculerError::ActionNotFound(_) => 404,
            MoleculerError::RequestTimeout(_) => 504,
            MoleculerError::CircuitBreakerOpen(_) => 503,
            MoleculerError::QueueFull(_) => 429,
            MoleculerError::Validation { .. } => 422,
            MoleculerError::MaxCallLevelReached(_) => 500,
            _ => 500,
        }
    }
}
