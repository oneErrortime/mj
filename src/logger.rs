//! # logger
//!
//! Logging subsystem — mirrors `src/logger/` from Moleculer.js.
//!
//! ## Architecture
//!
//! ```text
//! Logger (trait)
//!   ├─ ConsoleLogger   ← human-readable, coloured, with timestamps
//!   ├─ JsonLogger      ← structured JSON — one object per line (log shipping)
//!   └─ NoopLogger      ← silences all output (useful in tests)
//! ```
//!
//! A [`MoleculerLogger`] wraps any backend and adds:
//! - **Module prefix** — printed as `[SERVICE/NODE]`
//! - **Log level filtering** — messages below the configured level are dropped
//! - **Bindings** — key-value pairs attached to every log entry (mirrors
//!   `logger.child({ requestID })` in Moleculer.js)
//!
//! ## Example
//!
//! ```rust,no_run
//! use moleculer::logger::{MoleculerLogger, ConsoleLogger, LogLevel};
//! use std::sync::Arc;
//!
//! let backend = Arc::new(ConsoleLogger::new(true)); // coloured = true
//! let log = MoleculerLogger::new("MY-SERVICE", "node-1", LogLevel::Debug, backend);
//!
//! log.info("Service started", &[]);
//! log.debug("Calling action", &[("action", "math.add")]);
//! log.error("Something failed", &[("err", "timeout")]);
//! ```

use crate::config::LogLevel;
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

// ─── Logger back-end trait ────────────────────────────────────────────────────

/// Low-level sink: receives a fully-formed log record and writes it somewhere.
pub trait LoggerBackend: Send + Sync {
    /// Write a single log record.
    fn write(&self, record: &LogRecord);
}

// ─── LogRecord ───────────────────────────────────────────────────────────────

/// A single log event — all fields are resolved before reaching the backend.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// UTC timestamp (ISO 8601).
    pub timestamp: String,
    /// Severity level.
    pub level: LogLevel,
    /// Module / service name (e.g. `"BROKER"`, `"MY-SERVICE"`).
    pub module: String,
    /// Node identifier.
    pub node_id: String,
    /// Human-readable message.
    pub message: String,
    /// Extra key-value pairs attached to this record.
    pub bindings: HashMap<String, String>,
}

impl LogRecord {
    /// Build a `serde_json::Value` representation of this record
    /// (used by [`JsonLogger`]).
    pub fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("time".into(), json!(self.timestamp));
        obj.insert("level".into(), json!(format!("{}", self.level)));
        obj.insert("module".into(), json!(self.module));
        obj.insert("nodeID".into(), json!(self.node_id));
        obj.insert("msg".into(), json!(self.message));
        for (k, v) in &self.bindings {
            obj.insert(k.clone(), json!(v));
        }
        Value::Object(obj)
    }
}

// ─── LogLevel Display ─────────────────────────────────────────────────────────

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info  => write!(f, "INFO"),
            LogLevel::Warn  => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Fatal => write!(f, "FATAL"),
        }
    }
}

impl LogLevel {
    /// Numeric priority — lower means more verbose.
    pub fn priority(&self) -> u8 {
        match self {
            LogLevel::Trace => 0,
            LogLevel::Debug => 1,
            LogLevel::Info  => 2,
            LogLevel::Warn  => 3,
            LogLevel::Error => 4,
            LogLevel::Fatal => 5,
        }
    }

    /// Returns `true` when `self` is at least as severe as `min`.
    pub fn is_enabled(&self, min: &LogLevel) -> bool {
        self.priority() >= min.priority()
    }

    /// ANSI colour code for terminal output.
    fn ansi_color(&self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[37m",      // white
            LogLevel::Debug => "\x1b[36m",      // cyan
            LogLevel::Info  => "\x1b[32m",      // green
            LogLevel::Warn  => "\x1b[33m",      // yellow
            LogLevel::Error => "\x1b[31m",      // red
            LogLevel::Fatal => "\x1b[35m",      // magenta
        }
    }
}

// ─── ConsoleLogger ────────────────────────────────────────────────────────────

/// Prints log records to `stderr` in a human-readable format.
///
/// Output format (coloured):
/// ```text
/// [2024-01-15T10:23:45Z] INFO  MY-SERVICE/node-1: Message key=value
/// ```
pub struct ConsoleLogger {
    /// Whether to emit ANSI colour codes.
    pub coloured: bool,
    lock: Mutex<()>,
}

impl ConsoleLogger {
    pub fn new(coloured: bool) -> Self {
        Self { coloured, lock: Mutex::new(()) }
    }

    fn level_tag(&self, level: &LogLevel) -> String {
        let tag = format!("{:<5}", format!("{}", level));
        if self.coloured {
            format!("{}{}\x1b[0m", level.ansi_color(), tag)
        } else {
            tag
        }
    }
}

impl LoggerBackend for ConsoleLogger {
    fn write(&self, record: &LogRecord) {
        let _guard = self.lock.lock().unwrap();

        let mut line = format!(
            "[{}] {} {}/{}: {}",
            record.timestamp,
            self.level_tag(&record.level),
            record.module,
            record.node_id,
            record.message,
        );

        if !record.bindings.is_empty() {
            let pairs: Vec<String> = record.bindings
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            line.push(' ');
            line.push_str(&pairs.join(" "));
        }

        let stderr = io::stderr();
        let mut out = stderr.lock();
        let _ = writeln!(out, "{}", line);
    }
}

// ─── JsonLogger ───────────────────────────────────────────────────────────────

/// Emits one JSON object per line to `stderr` (NDJSON / log-shipping friendly).
///
/// Example output:
/// ```json
/// {"time":"2024-01-15T10:23:45Z","level":"INFO","module":"BROKER","nodeID":"node-1","msg":"Broker started"}
/// ```
pub struct JsonLogger {
    lock: Mutex<()>,
}

impl JsonLogger {
    pub fn new() -> Self {
        Self { lock: Mutex::new(()) }
    }
}

impl Default for JsonLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggerBackend for JsonLogger {
    fn write(&self, record: &LogRecord) {
        let _guard = self.lock.lock().unwrap();
        let json = record.to_json().to_string();
        let stderr = io::stderr();
        let mut out = stderr.lock();
        let _ = writeln!(out, "{}", json);
    }
}

// ─── NoopLogger ───────────────────────────────────────────────────────────────

/// Silently discards all log records.
///
/// Useful in unit tests where you want zero output.
#[derive(Default, Clone)]
pub struct NoopLogger;

impl LoggerBackend for NoopLogger {
    fn write(&self, _record: &LogRecord) {}
}

// ─── MoleculerLogger ──────────────────────────────────────────────────────────

/// High-level logger used throughout the broker.
///
/// Wraps any [`LoggerBackend`] and enriches records with:
/// - module name & node ID prefix
/// - minimum log-level filtering
/// - persistent key-value bindings (e.g. `requestID`)
///
/// Mirrors the interface of Moleculer.js `logger.*` calls.
pub struct MoleculerLogger {
    module:   String,
    node_id:  String,
    min_level: LogLevel,
    backend:  Arc<dyn LoggerBackend>,
    bindings: HashMap<String, String>,
}

impl MoleculerLogger {
    /// Create a new logger.
    ///
    /// * `module`    — module / service name shown in every line
    /// * `node_id`   — broker node ID shown in every line
    /// * `min_level` — messages below this level are silently dropped
    /// * `backend`   — sink that does the actual I/O
    pub fn new(
        module: impl Into<String>,
        node_id: impl Into<String>,
        min_level: LogLevel,
        backend: Arc<dyn LoggerBackend>,
    ) -> Self {
        Self {
            module: module.into().to_uppercase(),
            node_id: node_id.into(),
            min_level,
            backend,
            bindings: HashMap::new(),
        }
    }

    /// Create a child logger with additional persistent bindings.
    ///
    /// Mirrors `logger.child({ requestID: ctx.requestID })` in Moleculer.js.
    pub fn child(&self, bindings: &[(&str, &str)]) -> Self {
        let mut b = self.bindings.clone();
        for (k, v) in bindings {
            b.insert(k.to_string(), v.to_string());
        }
        Self {
            module: self.module.clone(),
            node_id: self.node_id.clone(),
            min_level: self.min_level.clone(),
            backend: Arc::clone(&self.backend),
            bindings: b,
        }
    }

    // ── Core emit ────────────────────────────────────────────────────────────

    fn emit(&self, level: LogLevel, message: &str, extra: &[(&str, &str)]) {
        if !level.is_enabled(&self.min_level) {
            return;
        }

        let mut bindings = self.bindings.clone();
        for (k, v) in extra {
            bindings.insert(k.to_string(), v.to_string());
        }

        let record = LogRecord {
            timestamp: Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            level,
            module: self.module.clone(),
            node_id: self.node_id.clone(),
            message: message.to_string(),
            bindings,
        };

        self.backend.write(&record);
    }

    // ── Public API — mirrors logger.trace/debug/info/warn/error/fatal ─────────

    pub fn trace(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Trace, msg, extra);
    }

    pub fn debug(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Debug, msg, extra);
    }

    pub fn info(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Info, msg, extra);
    }

    pub fn warn(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Warn, msg, extra);
    }

    pub fn error(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Error, msg, extra);
    }

    pub fn fatal(&self, msg: &str, extra: &[(&str, &str)]) {
        self.emit(LogLevel::Fatal, msg, extra);
    }
}

// ─── Factory helper ───────────────────────────────────────────────────────────

/// Build a [`MoleculerLogger`] from broker config values.
///
/// Selects the backend based on `format`:
/// - `"json"` → [`JsonLogger`]
/// - `"noop"` / `"silent"` → [`NoopLogger`]
/// - anything else → [`ConsoleLogger`] (coloured by default)
pub fn build_logger(
    module: impl Into<String>,
    node_id: impl Into<String>,
    min_level: LogLevel,
    format: &str,
    coloured: bool,
) -> MoleculerLogger {
    let backend: Arc<dyn LoggerBackend> = match format {
        "json" => Arc::new(JsonLogger::new()),
        "noop" | "silent" | "false" => Arc::new(NoopLogger),
        _ => Arc::new(ConsoleLogger::new(coloured)),
    };
    MoleculerLogger::new(module, node_id, min_level, backend)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Capture backend — collects records in a Vec for assertions.
    struct CaptureBackend {
        records: Mutex<Vec<LogRecord>>,
    }

    impl CaptureBackend {
        fn new() -> Arc<Self> {
            Arc::new(Self { records: Mutex::new(Vec::new()) })
        }
        fn records(&self) -> Vec<LogRecord> {
            self.records.lock().unwrap().clone()
        }
    }

    impl LoggerBackend for CaptureBackend {
        fn write(&self, r: &LogRecord) {
            self.records.lock().unwrap().push(r.clone());
        }
    }

    fn make_logger(min: LogLevel, backend: Arc<dyn LoggerBackend>) -> MoleculerLogger {
        MoleculerLogger::new("TEST", "node-1", min, backend)
    }

    #[test]
    fn info_message_is_captured() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Info, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        log.info("hello world", &[]);
        let recs = cap.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].message, "hello world");
        assert!(matches!(recs[0].level, LogLevel::Info));
    }

    #[test]
    fn debug_filtered_when_min_is_info() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Info, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        log.debug("should not appear", &[]);
        assert!(cap.records().is_empty());
    }

    #[test]
    fn extra_bindings_attached() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Trace, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        log.info("calling action", &[("action", "math.add"), ("nodeID", "remote-1")]);
        let recs = cap.records();
        assert_eq!(recs[0].bindings.get("action").map(String::as_str), Some("math.add"));
    }

    #[test]
    fn child_logger_inherits_bindings() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Info, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        let child = log.child(&[("requestID", "req-abc")]);
        child.info("child log", &[]);
        let recs = cap.records();
        assert_eq!(recs[0].bindings.get("requestID").map(String::as_str), Some("req-abc"));
    }

    #[test]
    fn fatal_always_passes_highest_filter() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Fatal, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        log.fatal("crash", &[]);
        assert_eq!(cap.records().len(), 1);
    }

    #[test]
    fn log_level_priority_ordering() {
        assert!(LogLevel::Fatal.priority() > LogLevel::Error.priority());
        assert!(LogLevel::Error.priority() > LogLevel::Warn.priority());
        assert!(LogLevel::Warn.priority() > LogLevel::Info.priority());
        assert!(LogLevel::Info.priority() > LogLevel::Debug.priority());
        assert!(LogLevel::Debug.priority() > LogLevel::Trace.priority());
    }

    #[test]
    fn noop_logger_writes_nothing() {
        let backend = Arc::new(NoopLogger);
        let log = make_logger(LogLevel::Trace, backend);
        // Should not panic and produces no output
        log.trace("silent", &[]);
        log.info("silent", &[]);
        log.fatal("silent", &[]);
    }

    #[test]
    fn json_record_has_expected_keys() {
        let cap = CaptureBackend::new();
        let log = make_logger(LogLevel::Info, Arc::clone(&cap) as Arc<dyn LoggerBackend>);
        log.warn("watch out", &[("code", "429")]);
        let rec = &cap.records()[0];
        let json = rec.to_json();
        assert!(json.get("time").is_some());
        assert_eq!(json["level"], "WARN");
        assert_eq!(json["module"], "TEST");
        assert_eq!(json["nodeID"], "node-1");
        assert_eq!(json["msg"], "watch out");
        assert_eq!(json["code"], "429");
    }

    #[test]
    fn build_logger_json_format() {
        // Should not panic; output goes to stderr which is fine in tests
        let log = build_logger("BROKER", "n1", LogLevel::Info, "json", false);
        log.info("json logger works", &[]);
    }

    #[test]
    fn build_logger_noop_format() {
        let log = build_logger("BROKER", "n1", LogLevel::Trace, "noop", false);
        log.fatal("this should not crash", &[]);
    }
}
