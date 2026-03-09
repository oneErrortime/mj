//! Runner — loads and starts broker + services from environment or config file.
//!
//! Mirrors `moleculer/src/runner.js`.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use moleculer::runner::Runner;
//!
//! #[tokio::main]
//! async fn main() {
//!     Runner::new().run().await;
//! }
//! ```

use crate::broker::ServiceBroker;
use crate::config::BrokerConfig;
use std::sync::Arc;

/// Environment variable names (mirrors Moleculer's runner).
pub const ENV_NODE_ID: &str = "NODEID";
pub const ENV_NAMESPACE: &str = "NAMESPACE";
pub const ENV_LOG_LEVEL: &str = "LOGLEVEL";
pub const ENV_TRANSPORTER: &str = "TRANSPORTER";
pub const ENV_REQUEST_TIMEOUT: &str = "REQUEST_TIMEOUT";
pub const ENV_HEARTBEAT_INTERVAL: &str = "HEARTBEAT_INTERVAL";
pub const ENV_MAX_CALL_LEVEL: &str = "MAX_CALL_LEVEL";
pub const ENV_HOT_RELOAD: &str = "HOT_RELOAD";

pub struct Runner {
    config: BrokerConfig,
}

impl Runner {
    pub fn new() -> Self {
        let mut config = BrokerConfig::default();
        Self::apply_env(&mut config);
        Self { config }
    }

    pub fn with_config(config: BrokerConfig) -> Self {
        let mut c = config;
        Self::apply_env(&mut c);
        Self { config: c }
    }

    /// Merge environment variables into config.
    fn apply_env(config: &mut BrokerConfig) {
        if let Ok(v) = std::env::var(ENV_NODE_ID) {
            config.node_id = Some(v);
        }
        if let Ok(v) = std::env::var(ENV_NAMESPACE) {
            config.namespace = v;
        }
        if let Ok(v) = std::env::var(ENV_LOG_LEVEL) {
            config.log_level = match v.to_lowercase().as_str() {
                "trace" => crate::config::LogLevel::Trace,
                "debug" => crate::config::LogLevel::Debug,
                "info"  => crate::config::LogLevel::Info,
                "warn"  => crate::config::LogLevel::Warn,
                "error" => crate::config::LogLevel::Error,
                "fatal" => crate::config::LogLevel::Fatal,
                _ => crate::config::LogLevel::Info,
            };
        }
        if let Ok(v) = std::env::var(ENV_REQUEST_TIMEOUT) {
            if let Ok(n) = v.parse::<u64>() {
                config.request_timeout = n;
            }
        }
        if let Ok(v) = std::env::var(ENV_HEARTBEAT_INTERVAL) {
            if let Ok(n) = v.parse::<u64>() {
                config.heartbeat_interval = n;
            }
        }
        if let Ok(v) = std::env::var(ENV_MAX_CALL_LEVEL) {
            if let Ok(n) = v.parse::<u32>() {
                config.max_call_level = n;
            }
        }
    }

    pub async fn run(self) -> Arc<ServiceBroker> {
        let log_level = match self.config.log_level {
            crate::config::LogLevel::Trace => log::LevelFilter::Trace,
            crate::config::LogLevel::Debug => log::LevelFilter::Debug,
            crate::config::LogLevel::Info  => log::LevelFilter::Info,
            crate::config::LogLevel::Warn  => log::LevelFilter::Warn,
            crate::config::LogLevel::Error => log::LevelFilter::Error,
            crate::config::LogLevel::Fatal => log::LevelFilter::Error,
        };

        let _ = env_logger::Builder::new()
            .filter_level(log_level)
            .try_init();

        let broker = ServiceBroker::new(self.config);
        broker.install_default_middlewares().await;

        log::info!("[Runner] moleculer-rs starting (node: {})", broker.node_id);

        // Set up Ctrl-C handler for graceful shutdown
        let broker_clone = Arc::clone(&broker);
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                log::info!("[Runner] received Ctrl-C, stopping broker...");
                let _ = broker_clone.stop().await;
                std::process::exit(0);
            }
        });

        broker.start().await.expect("[Runner] broker failed to start");
        broker
    }
}

impl Default for Runner {
    fn default() -> Self { Self::new() }
}
