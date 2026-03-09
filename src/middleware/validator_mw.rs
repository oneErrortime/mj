//! Validator middleware — validates action params before handler runs.
//!
//! Mirrors `moleculer/src/middlewares/validator.js`.
//!
//! Intercepts every `call()` and runs the action's `params` schema through
//! [`JsonSchemaValidator`]. Returns [`MoleculerError::ValidationFailed`] if
//! validation fails, allowing the broker to return a proper error response.

use super::{Middleware, NextHandler};
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use crate::registry::ServiceRegistry;
use crate::validators::{JsonSchemaValidator, Validator};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct ValidatorMiddleware {
    validator: JsonSchemaValidator,
    registry: Arc<ServiceRegistry>,
}

impl ValidatorMiddleware {
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        Self { validator: JsonSchemaValidator::default(), registry }
    }
}

#[async_trait]
impl Middleware for ValidatorMiddleware {
    fn name(&self) -> &str { "Validator" }

    async fn call_action(&self, ctx: Context, next: NextHandler) -> Result<Value> {
        // Look up action definition for this context's action name
        if let Some(action_name) = ctx.action.as_deref() {
            if let Some(endpoint) = self.registry.find_action(action_name) {
                if let Some(schema) = &endpoint.action.params {
                    // Parse the schema
                    if let Ok(param_schema) = serde_json::from_value(schema.clone()) {
                        self.validator.validate(&ctx.params, &param_schema)
                            .map_err(|e| MoleculerError::ValidationFailed(e.to_string()))?;
                    }
                }
            }
        }
        next(ctx).await
    }
}
