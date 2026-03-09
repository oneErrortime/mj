//! # serializers
//!
//! Packet serialisation subsystem — mirrors `src/serializers/` from Moleculer.js.
//!
//! ## Supported backends
//!
//! | Backend             | Feature flag     | Crate          |
//! |---------------------|------------------|----------------|
//! | JSON (built-in)     | always on        | `serde_json`   |
//! | MessagePack         | `msgpack`        | `rmp-serde`    |
//!
//! ## Usage
//!
//! ```rust,no_run
//! use moleculer::serializers::{JsonSerializer, Serializer};
//! use serde_json::json;
//!
//! let s = JsonSerializer;
//! let bytes = s.serialize(&json!({ "a": 1 })).unwrap();
//! let back  = s.deserialize(&bytes).unwrap();
//! assert_eq!(back["a"], 1);
//! ```

use crate::error::{MoleculerError, Result};
use serde_json::Value;

// ─── Trait ───────────────────────────────────────────────────────────────────

/// A serializer converts a [`serde_json::Value`] to/from a byte buffer.
///
/// All packet payloads in the transit layer are [`Value`], so a single
/// trait surface is enough to support any backend.
pub trait Serializer: Send + Sync {
    /// Human-readable name (e.g. `"JSON"`, `"MsgPack"`).
    fn name(&self) -> &'static str;

    /// Serialise `value` into a byte buffer.
    fn serialize(&self, value: &Value) -> Result<Vec<u8>>;

    /// Deserialise a byte buffer into a [`Value`].
    fn deserialize(&self, bytes: &[u8]) -> Result<Value>;
}

// ─── JSON ─────────────────────────────────────────────────────────────────────

/// JSON serializer (always available, no extra feature flag needed).
///
/// Uses `serde_json::to_vec` / `serde_json::from_slice` internally.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonSerializer;

impl Serializer for JsonSerializer {
    fn name(&self) -> &'static str { "JSON" }

    fn serialize(&self, value: &Value) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(MoleculerError::Serialization)
    }

    fn deserialize(&self, bytes: &[u8]) -> Result<Value> {
        serde_json::from_slice(bytes).map_err(MoleculerError::Serialization)
    }
}

// ─── MessagePack ──────────────────────────────────────────────────────────────

/// MessagePack serializer.
///
/// Enable with `features = ["msgpack"]` in Cargo.toml.
/// Requires the `rmp-serde` crate.
///
/// Wire format is ~30–50 % smaller and faster to parse than JSON for
/// typical Moleculer packets.
#[cfg(feature = "msgpack")]
#[derive(Debug, Clone, Copy, Default)]
pub struct MsgPackSerializer;

#[cfg(feature = "msgpack")]
impl Serializer for MsgPackSerializer {
    fn name(&self) -> &'static str { "MsgPack" }

    fn serialize(&self, value: &Value) -> Result<Vec<u8>> {
        rmp_serde::to_vec_named(value).map_err(|e| MoleculerError::Transport(e.to_string()))
    }

    fn deserialize(&self, bytes: &[u8]) -> Result<Value> {
        rmp_serde::from_slice(bytes).map_err(|e| MoleculerError::Transport(e.to_string()))
    }
}

// ─── Factory ─────────────────────────────────────────────────────────────────

/// Supported serializer identifiers (mirrors `SerializerOptions` in `config.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SerializerKind {
    #[default]
    Json,
    #[cfg(feature = "msgpack")]
    MsgPack,
}

/// Create a boxed serializer from a [`SerializerKind`].
///
/// ```rust,no_run
/// use moleculer::serializers::{make_serializer, SerializerKind};
/// let s = make_serializer(SerializerKind::Json);
/// ```
pub fn make_serializer(kind: SerializerKind) -> Box<dyn Serializer> {
    match kind {
        SerializerKind::Json => Box::new(JsonSerializer),
        #[cfg(feature = "msgpack")]
        SerializerKind::MsgPack => Box::new(MsgPackSerializer),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip(s: &dyn Serializer, val: Value) {
        let bytes = s.serialize(&val).expect("serialize");
        let back = s.deserialize(&bytes).expect("deserialize");
        assert_eq!(val, back);
    }

    #[test]
    fn json_roundtrip_object() {
        roundtrip(&JsonSerializer, json!({ "a": 1, "b": "hello", "c": [1, 2, 3] }));
    }

    #[test]
    fn json_roundtrip_null() {
        roundtrip(&JsonSerializer, Value::Null);
    }

    #[test]
    fn json_roundtrip_nested() {
        roundtrip(&JsonSerializer, json!({
            "sender": "node-1",
            "kind": "REQ",
            "payload": { "action": "math.add", "params": { "a": 1, "b": 2 } }
        }));
    }

    #[test]
    fn json_invalid_bytes_err() {
        let s = JsonSerializer;
        assert!(s.deserialize(b"not json!!").is_err());
    }

    #[test]
    fn json_name() {
        assert_eq!(JsonSerializer.name(), "JSON");
    }

    #[cfg(feature = "msgpack")]
    #[test]
    fn msgpack_roundtrip() {
        roundtrip(&MsgPackSerializer, json!({ "x": 42, "y": [1, 2] }));
    }
}
