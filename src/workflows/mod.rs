//! Workflows — declarative multi-step pipelines over Moleculer services.
//!
//! Inspired by `moleculerjs/workflows`.
//!
//! A workflow is a directed graph of `Step`s.
//! Each step is an action call (or event emit) with optional:
//! - `depends_on` — steps that must complete first
//! - `condition`  — only run if predicate returns true
//! - `transform`  — map previous output to this step's params
//! - `retry`      — override retry policy for this step
//! - `timeout`    — override timeout for this step
//!
//! ## Example
//!
//! ```rust,no_run
//! use moleculer::workflows::{Workflow, Step};
//!
//! let wf = Workflow::builder("checkout")
//!     .step(Step::call("payment.charge").params(|ctx| ctx.params.clone()))
//!     .step(Step::call("inventory.reserve").depends_on("payment.charge"))
//!     .step(Step::call("email.send").depends_on("inventory.reserve"))
//!     .build();
//! ```

use crate::broker::ServiceBroker;
use crate::context::Context;
use crate::error::{MoleculerError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// ─── Step ────────────────────────────────────────────────────────────────────

pub type ParamMapper = Arc<dyn Fn(&WorkflowContext) -> Value + Send + Sync>;
pub type StepCondition = Arc<dyn Fn(&WorkflowContext) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct Step {
    pub id: String,
    /// Action to call (e.g. "payment.charge").
    pub action: String,
    /// Steps that must complete before this one.
    pub depends_on: Vec<String>,
    /// Map workflow context → params for this call.
    pub param_mapper: Option<ParamMapper>,
    /// Only run if this predicate returns true.
    pub condition: Option<StepCondition>,
    /// Override timeout (ms).
    pub timeout: Option<u64>,
    /// Override max retries.
    pub max_retries: Option<u32>,
    /// If true, failure of this step does not fail the workflow.
    pub allow_failure: bool,
    /// Collect output into workflow context under this key.
    pub output_key: Option<String>,
}

impl Step {
    pub fn call(action: impl Into<String>) -> Self {
        let action: String = action.into();
        let id = action.replace('.', "_");
        Self {
            id,
            action,
            depends_on: Vec::new(),
            param_mapper: None,
            condition: None,
            timeout: None,
            max_retries: None,
            allow_failure: false,
            output_key: None,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into(); self
    }

    pub fn depends_on(mut self, step_id: impl Into<String>) -> Self {
        self.depends_on.push(step_id.into()); self
    }

    pub fn params<F: Fn(&WorkflowContext) -> Value + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.param_mapper = Some(Arc::new(f)); self
    }

    pub fn when<F: Fn(&WorkflowContext) -> bool + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.condition = Some(Arc::new(f)); self
    }

    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout = Some(ms); self
    }

    pub fn allow_failure(mut self) -> Self {
        self.allow_failure = true; self
    }

    pub fn output_as(mut self, key: impl Into<String>) -> Self {
        self.output_key = Some(key.into()); self
    }
}

// ─── WorkflowContext ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct WorkflowContext {
    /// Initial params passed to the workflow.
    pub params: Value,
    /// Collected outputs from completed steps.
    pub outputs: HashMap<String, Value>,
    /// Step-level errors (for steps with allow_failure).
    pub errors: HashMap<String, String>,
    pub workflow_id: String,
}

impl WorkflowContext {
    pub fn new(workflow_id: String, params: Value) -> Self {
        Self {
            params,
            outputs: HashMap::new(),
            errors: HashMap::new(),
            workflow_id,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.outputs.get(key)
    }
}

// ─── Workflow ─────────────────────────────────────────────────────────────────

pub struct Workflow {
    pub name: String,
    steps: Vec<Step>,
}

impl Workflow {
    pub fn builder(name: impl Into<String>) -> WorkflowBuilder {
        WorkflowBuilder { name: name.into(), steps: Vec::new() }
    }

    /// Execute all steps topologically, respecting dependencies.
    pub async fn run(
        &self,
        broker: &Arc<ServiceBroker>,
        params: Value,
    ) -> Result<WorkflowContext> {
        let wf_id = uuid::Uuid::new_v4().to_string();
        let mut ctx = WorkflowContext::new(wf_id.clone(), params.clone());
        let mut completed: HashMap<String, bool> = HashMap::new();

        log::info!("[Workflow:{}] starting '{}' ({} steps)", wf_id, self.name, self.steps.len());

        // Keep iterating until all steps are done or we can make no progress
        let total = self.steps.len();
        let mut iterations = 0;
        while completed.len() < total {
            iterations += 1;
            if iterations > total * 2 {
                return Err(MoleculerError::Internal(
                    format!("Workflow '{}' deadlock detected", self.name)
                ));
            }

            let mut made_progress = false;
            for step in &self.steps {
                if completed.contains_key(&step.id) { continue; }

                // Check all dependencies are done
                let deps_done = step.depends_on.iter().all(|d| completed.contains_key(d));
                if !deps_done { continue; }

                // Condition guard
                if let Some(cond) = &step.condition {
                    if !cond(&ctx) {
                        log::debug!("[Workflow:{}] step '{}' skipped (condition false)", wf_id, step.id);
                        completed.insert(step.id.clone(), true);
                        made_progress = true;
                        continue;
                    }
                }

                // Build params for this step
                let step_params = match &step.param_mapper {
                    Some(mapper) => mapper(&ctx),
                    None => ctx.params.clone(),
                };

                log::debug!("[Workflow:{}] executing step '{}'", wf_id, step.id);

                let result = broker.call(&step.action, step_params).await;

                match result {
                    Ok(output) => {
                        let key = step.output_key.clone().unwrap_or_else(|| step.id.clone());
                        ctx.outputs.insert(key, output);
                        completed.insert(step.id.clone(), true);
                        made_progress = true;
                        log::debug!("[Workflow:{}] step '{}' succeeded", wf_id, step.id);
                    }
                    Err(e) => {
                        if step.allow_failure {
                            log::warn!("[Workflow:{}] step '{}' failed (allowed): {}", wf_id, step.id, e);
                            ctx.errors.insert(step.id.clone(), e.to_string());
                            completed.insert(step.id.clone(), true);
                            made_progress = true;
                        } else {
                            log::error!("[Workflow:{}] step '{}' failed: {}", wf_id, step.id, e);
                            return Err(MoleculerError::Internal(
                                format!("Workflow '{}' failed at step '{}': {}", self.name, step.id, e)
                            ));
                        }
                    }
                }
            }

            if !made_progress { break; }
        }

        log::info!("[Workflow:{}] '{}' completed ({} steps done, {} errors)",
            wf_id, self.name, completed.len(), ctx.errors.len());

        Ok(ctx)
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

pub struct WorkflowBuilder {
    name: String,
    steps: Vec<Step>,
}

impl WorkflowBuilder {
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    pub fn build(self) -> Workflow {
        Workflow { name: self.name, steps: self.steps }
    }
}

// ─── WorkflowService ─────────────────────────────────────────────────────────

/// A service that can register and execute workflows via actions.
pub struct WorkflowService {
    workflows: dashmap::DashMap<String, Arc<Workflow>>,
    broker: Arc<ServiceBroker>,
}

impl WorkflowService {
    pub fn new(broker: Arc<ServiceBroker>) -> Self {
        Self { workflows: dashmap::DashMap::new(), broker }
    }

    pub fn register(&self, workflow: Workflow) {
        let name = workflow.name.clone();
        self.workflows.insert(name.clone(), Arc::new(workflow));
        log::info!("[WorkflowService] registered workflow: '{}'", name);
    }

    pub async fn run(&self, name: &str, params: Value) -> Result<WorkflowContext> {
        let wf = self.workflows.get(name)
            .ok_or_else(|| MoleculerError::ActionNotFound(format!("Workflow '{}' not found", name)))?
            .clone();
        wf.run(&self.broker, params).await
    }

    /// Expose all registered workflows as actions on a service.
    pub fn as_service_schema(&self) -> crate::service::ServiceSchema {
        // Would iterate workflows and create one action per workflow
        // For now return empty schema
        crate::service::ServiceSchema::new("$workflows")
    }
}
