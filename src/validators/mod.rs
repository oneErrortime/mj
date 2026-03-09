//! # validators
//!
//! Parameter validation subsystem — mirrors `src/validators/` from Moleculer.js.
//!
//! ## Architecture
//!
//! ```text
//! Validator (trait)
//!   └─ JsonSchemaValidator   ← built-in, no extra deps (serde_json)
//! ```
//!
//! The broker compiles a [`ParamSchema`] once per action/event definition and
//! calls [`Validator::validate`] on every incoming [`Context`] before the
//! handler runs.  A [`ValidationError`] is returned when validation fails.
//!
//! ## Example
//!
//! ```rust,no_run
//! use moleculer::validators::{JsonSchemaValidator, Validator, ParamSchema};
//! use serde_json::json;
//!
//! let v = JsonSchemaValidator::default();
//! let schema: ParamSchema = serde_json::from_value(json!({
//!     "a": { "type": "number" },
//!     "b": { "type": "string", "optional": true }
//! })).unwrap();
//!
//! let ok = json!({ "a": 42 });
//! assert!(v.validate(&ok, &schema).is_ok());
//!
//! let bad = json!({ "b": "hello" }); // missing required "a"
//! assert!(v.validate(&bad, &schema).is_err());
//! ```

use crate::error::{MoleculerError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ─── Schema types ─────────────────────────────────────────────────────────────

/// A single field constraint in a param schema.
///
/// Matches the subset of `fastest-validator` schema used most often in Moleculer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldConstraint {
    /// Expected JSON type: `"string"`, `"number"`, `"boolean"`, `"object"`, `"array"`, `"any"`.
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,

    /// Whether the field may be absent (default: `false` → required).
    #[serde(default)]
    pub optional: bool,

    /// Minimum value (for numbers) or minimum length (for strings / arrays).
    pub min: Option<f64>,

    /// Maximum value (for numbers) or maximum length (for strings / arrays).
    pub max: Option<f64>,

    /// Allowed string values (enum).
    #[serde(rename = "enum", default)]
    pub enum_values: Option<Vec<Value>>,

    /// Human-readable label used in error messages.
    pub label: Option<String>,
}

/// A full param schema: `{ fieldName: FieldConstraint, … }`.
pub type ParamSchema = HashMap<String, FieldConstraint>;

// ─── Validation error detail ──────────────────────────────────────────────────

/// One validation failure entry (mirrors fastest-validator error objects).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFailure {
    /// Machine-readable error type (e.g. `"required"`, `"string"`, `"number"`).
    pub error_type: String,
    /// The offending field name.
    pub field: String,
    /// Human-readable message.
    pub message: String,
    /// The actual value that was supplied (if any).
    pub actual: Option<Value>,
    /// The expected type / constraint description.
    pub expected: Option<String>,
}

// ─── Validator trait ──────────────────────────────────────────────────────────

/// Trait implemented by every Moleculer validator backend.
pub trait Validator: Send + Sync {
    /// Validate `params` against `schema`.
    ///
    /// Returns `Ok(())` on success, or
    /// `Err(MoleculerError::ValidationError { … })` on failure.
    fn validate(&self, params: &Value, schema: &ParamSchema) -> Result<()>;
}

// ─── Built-in: JsonSchemaValidator ───────────────────────────────────────────

/// Lightweight JSON Schema validator backed by `serde_json`.
///
/// Supports: `type`, `optional`, `min`, `max`, `enum`.
/// Does **not** require `fastest-validator` (a JS library) — instead it
/// implements the same rule subset in pure Rust.
#[derive(Debug, Clone, Default)]
pub struct JsonSchemaValidator;

impl JsonSchemaValidator {
    pub fn new() -> Self {
        Self
    }
}

impl Validator for JsonSchemaValidator {
    fn validate(&self, params: &Value, schema: &ParamSchema) -> Result<()> {
        let mut failures: Vec<ValidationFailure> = Vec::new();

        for (field, constraint) in schema {
            let value = params.get(field);

            // ── Required check ──────────────────────────────────────────────
            if value.is_none() || value == Some(&Value::Null) {
                if !constraint.optional {
                    failures.push(ValidationFailure {
                        error_type: "required".to_string(),
                        field: field.clone(),
                        message: format!("The '{}' field is required.", field),
                        actual: None,
                        expected: Some("defined".to_string()),
                    });
                }
                continue; // no further checks for absent optional fields
            }

            let val = value.unwrap();

            // ── Type check ──────────────────────────────────────────────────
            if let Some(expected_type) = &constraint.field_type {
                if !type_matches(val, expected_type) {
                    failures.push(ValidationFailure {
                        error_type: expected_type.clone(),
                        field: field.clone(),
                        message: format!(
                            "The '{}' field must be a {}.",
                            field, expected_type
                        ),
                        actual: Some(json_type_name(val).into()),
                        expected: Some(expected_type.clone()),
                    });
                    continue; // skip further checks if type is wrong
                }
            }

            // ── Enum check ──────────────────────────────────────────────────
            if let Some(allowed) = &constraint.enum_values {
                if !allowed.contains(val) {
                    failures.push(ValidationFailure {
                        error_type: "enum".to_string(),
                        field: field.clone(),
                        message: format!(
                            "The '{}' field value '{}' is not in the allowed enum list.",
                            field, val
                        ),
                        actual: Some(val.clone()),
                        expected: Some(format!("{:?}", allowed)),
                    });
                }
            }

            // ── Min / Max checks ────────────────────────────────────────────
            if let Some(min) = constraint.min {
                let len = numeric_or_len(val);
                if let Some(n) = len {
                    if n < min {
                        failures.push(ValidationFailure {
                            error_type: "min".to_string(),
                            field: field.clone(),
                            message: format!(
                                "The '{}' field must be greater than or equal to {}.",
                                field, min
                            ),
                            actual: Some(Value::Number(
                                serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
                            )),
                            expected: Some(format!(">= {}", min)),
                        });
                    }
                }
            }

            if let Some(max) = constraint.max {
                let len = numeric_or_len(val);
                if let Some(n) = len {
                    if n > max {
                        failures.push(ValidationFailure {
                            error_type: "max".to_string(),
                            field: field.clone(),
                            message: format!(
                                "The '{}' field must be less than or equal to {}.",
                                field, max
                            ),
                            actual: Some(Value::Number(
                                serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
                            )),
                            expected: Some(format!("<= {}", max)),
                        });
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(MoleculerError::Validation {
                message: "Parameters validation error!".to_string(),
                failures: failures
                    .into_iter()
                    .map(|f| serde_json::to_value(f).unwrap_or_default())
                    .collect(),
            })
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn type_matches(val: &Value, expected: &str) -> bool {
    match expected {
        "any" => true,
        "string" => val.is_string(),
        "number" => val.is_number(),
        "boolean" | "bool" => val.is_boolean(),
        "object" => val.is_object(),
        "array" => val.is_array(),
        "null" => val.is_null(),
        _ => true, // unknown type — pass through
    }
}

fn json_type_name(val: &Value) -> &'static str {
    match val {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Returns the numeric value for a number, or the length/count for strings/arrays.
fn numeric_or_len(val: &Value) -> Option<f64> {
    match val {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => Some(s.len() as f64),
        Value::Array(a) => Some(a.len() as f64),
        _ => None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(json: serde_json::Value) -> ParamSchema {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn required_field_missing() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({ "name": { "type": "string" } }));
        assert!(v.validate(&json!({}), &s).is_err());
    }

    #[test]
    fn optional_field_absent_ok() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({ "name": { "type": "string", "optional": true } }));
        assert!(v.validate(&json!({}), &s).is_ok());
    }

    #[test]
    fn wrong_type() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({ "age": { "type": "number" } }));
        assert!(v.validate(&json!({ "age": "twenty" }), &s).is_err());
    }

    #[test]
    fn min_max_ok() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({ "score": { "type": "number", "min": 0, "max": 100 } }));
        assert!(v.validate(&json!({ "score": 50 }), &s).is_ok());
        assert!(v.validate(&json!({ "score": -1 }), &s).is_err());
        assert!(v.validate(&json!({ "score": 101 }), &s).is_err());
    }

    #[test]
    fn enum_check() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({ "color": { "type": "string", "enum": ["red", "green", "blue"] } }));
        assert!(v.validate(&json!({ "color": "red" }), &s).is_ok());
        assert!(v.validate(&json!({ "color": "purple" }), &s).is_err());
    }

    #[test]
    fn valid_params_pass() {
        let v = JsonSchemaValidator::new();
        let s = schema(json!({
            "a": { "type": "number" },
            "b": { "type": "string", "optional": true }
        }));
        assert!(v.validate(&json!({ "a": 42 }), &s).is_ok());
        assert!(v.validate(&json!({ "a": 1, "b": "hi" }), &s).is_ok());
    }
}
