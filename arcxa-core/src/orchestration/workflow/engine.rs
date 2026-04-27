//! Workflow engine for orchestrating multi-step governance operations

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock as StdRwLock};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::dag::DagExecutor;
use super::definition::{WorkflowDefinition, WorkflowStep};
use super::executor::{
    DbExtractCallback, DbLoaderCallback, ExecutionContext, SosValidationCallback,
    TransformerCallback, WorkflowExecutor, WorkflowResult,
};
use super::lineage_tracker::LineageTracker;
use super::row_lineage_context::RowLineageContext;
use crate::orchestration::ml::ModelInvoker;
use crate::orchestration::rules::RuleExecutor;

/// Workflow engine managing workflow lifecycle
pub struct WorkflowEngine {
    /// Registered workflows by ID
    workflows: Arc<RwLock<HashMap<String, RegisteredWorkflow>>>,
    /// Optional durable workflow metadata persistence callback
    workflow_persistence_callback: Arc<StdRwLock<Option<Arc<WorkflowPersistenceCallback>>>>,
    /// ML model invoker for workflow execution
    model_invoker: Option<Arc<ModelInvoker>>,
    /// Rule executor for workflow execution
    rule_executor: Option<Arc<RuleExecutor>>,
    /// Optional RDF persistence callback
    rdf_persistence_callback:
        Option<Arc<dyn Fn(&str, &WorkflowResult) -> Result<()> + Send + Sync>>,
    /// Optional lineage tracker for field-level and row-level provenance
    lineage_tracker: Option<Arc<dyn LineageTracker>>,
    /// Optional transformer callback for ETL steps (injected by coordinator)
    transformer_callback: Option<Arc<TransformerCallback>>,
    /// Optional DB loader callback for database loading steps (injected by coordinator)
    db_loader_callback: Option<Arc<DbLoaderCallback>>,
    /// Optional DB extract callback for database extraction steps (injected by coordinator)
    db_extract_callback: Option<Arc<DbExtractCallback>>,
    /// Optional SoS validation callback for backend/API-driven validation steps.
    sos_validation_callback: Option<Arc<SosValidationCallback>>,
}

/// Registered workflow with metadata
#[derive(Debug, Clone)]
struct RegisteredWorkflow {
    id: String,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
    definition: WorkflowDefinition,
    version: String,
    created_at: chrono::DateTime<chrono::Utc>,
    /// Number of times this workflow has been executed
    execution_count: u64,
    /// Timestamp of the last execution (None if never executed)
    last_executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl RegisteredWorkflow {
    fn to_metadata(&self) -> WorkflowMetadata {
        WorkflowMetadata {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            tags: self.tags.clone(),
            definition: self.definition.clone(),
            version: self.version.clone(),
            created_at: self.created_at,
            execution_count: self.execution_count,
            last_executed_at: self.last_executed_at,
        }
    }

    fn from_metadata(metadata: WorkflowMetadata) -> Self {
        Self {
            id: metadata.id,
            name: metadata.name,
            description: metadata.description,
            tags: metadata.tags,
            definition: metadata.definition,
            version: metadata.version,
            created_at: metadata.created_at,
            execution_count: metadata.execution_count,
            last_executed_at: metadata.last_executed_at,
        }
    }
}

/// Public workflow metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowMetadata {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub definition: WorkflowDefinition,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub execution_count: u64,
    pub last_executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Durable workflow metadata change emitted by the workflow engine.
#[derive(Debug, Clone)]
pub enum WorkflowPersistenceEvent {
    Upsert(WorkflowMetadata),
    Delete(String),
}

type WorkflowPersistenceCallback = dyn Fn(WorkflowPersistenceEvent) -> Result<()> + Send + Sync;

impl WorkflowEngine {
    /// Create new workflow engine without execution capabilities
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_callback: Arc::new(StdRwLock::new(None)),
            model_invoker: None,
            rule_executor: None,
            rdf_persistence_callback: None,
            lineage_tracker: None,
            transformer_callback: None,
            db_loader_callback: None,
            db_extract_callback: None,
            sos_validation_callback: None,
        }
    }

    /// Create new workflow engine with execution capabilities
    pub fn new_with_execution(
        model_invoker: Arc<ModelInvoker>,
        rule_executor: Arc<RuleExecutor>,
    ) -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_callback: Arc::new(StdRwLock::new(None)),
            model_invoker: Some(model_invoker),
            rule_executor: Some(rule_executor),
            rdf_persistence_callback: None,
            lineage_tracker: None,
            transformer_callback: None,
            db_loader_callback: None,
            db_extract_callback: None,
            sos_validation_callback: None,
        }
    }

    /// Set RDF persistence callback (called after each execution)
    pub fn with_rdf_persistence<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str, &WorkflowResult) -> Result<()> + Send + Sync + 'static,
    {
        self.rdf_persistence_callback = Some(Arc::new(callback));
        self
    }

    /// Set durable workflow metadata persistence callback.
    pub fn with_workflow_persistence<F>(self, callback: F) -> Self
    where
        F: Fn(WorkflowPersistenceEvent) -> Result<()> + Send + Sync + 'static,
    {
        self.set_workflow_persistence_callback(callback);
        self
    }

    /// Set or replace the durable workflow metadata persistence callback.
    pub fn set_workflow_persistence_callback<F>(&self, callback: F)
    where
        F: Fn(WorkflowPersistenceEvent) -> Result<()> + Send + Sync + 'static,
    {
        *self
            .workflow_persistence_callback
            .write()
            .expect("workflow persistence callback lock poisoned") = Some(Arc::new(callback));
    }

    fn persist_workflow_metadata_change(&self, event: WorkflowPersistenceEvent) -> Result<()> {
        let callback = self
            .workflow_persistence_callback
            .read()
            .expect("workflow persistence callback lock poisoned")
            .clone();

        if let Some(callback) = callback {
            callback(event)?;
        }

        Ok(())
    }

    /// Set lineage tracker for field-level and row-level provenance tracking
    pub fn with_lineage_tracker(mut self, tracker: Arc<dyn LineageTracker>) -> Self {
        self.lineage_tracker = Some(tracker);
        self
    }

    /// Set transformer callback for ETL steps (injected by coordinator)
    pub fn with_transformer_callback(mut self, callback: Arc<TransformerCallback>) -> Self {
        self.transformer_callback = Some(callback);
        self
    }

    /// Set DB loader callback for database loading steps (injected by coordinator)
    pub fn with_db_loader_callback(mut self, callback: Arc<DbLoaderCallback>) -> Self {
        self.db_loader_callback = Some(callback);
        self
    }

    /// Set DB extract callback for database extraction steps (injected by coordinator)
    pub fn with_db_extract_callback(mut self, callback: Arc<DbExtractCallback>) -> Self {
        self.db_extract_callback = Some(callback);
        self
    }

    /// Set SoS validation callback for workflow execution.
    pub fn with_sos_validation_callback(mut self, callback: Arc<SosValidationCallback>) -> Self {
        self.sos_validation_callback = Some(callback);
        self
    }

    /// Get reference to model invoker (if configured)
    pub fn model_invoker(&self) -> &Option<Arc<ModelInvoker>> {
        &self.model_invoker
    }

    /// Get reference to rule executor (if configured)
    pub fn rule_executor(&self) -> &Option<Arc<RuleExecutor>> {
        &self.rule_executor
    }

    /// Register a new workflow with optional metadata
    pub async fn register_workflow(
        &self,
        workflow_id: String,
        name: String,
        definition: WorkflowDefinition,
        description: Option<String>,
        tags: Vec<String>,
    ) -> Result<String> {
        // Validate workflow definition
        definition
            .validate()
            .context("Invalid workflow definition")?;

        // Validate DAG structure
        DagExecutor::from_workflow(&definition).context("Failed to build DAG from workflow")?;

        let mut workflows = self.workflows.write().await;
        if workflows.contains_key(&workflow_id) {
            anyhow::bail!("Workflow already exists: {}", workflow_id);
        }

        let workflow = RegisteredWorkflow {
            id: workflow_id.clone(),
            name,
            description,
            tags,
            definition,
            version: "1.0.0".to_string(),
            created_at: chrono::Utc::now(),
            execution_count: 0,
            last_executed_at: None,
        };

        workflows.insert(workflow_id.clone(), workflow.clone());
        drop(workflows);

        if let Err(error) = self.persist_workflow_metadata_change(WorkflowPersistenceEvent::Upsert(
            workflow.to_metadata(),
        )) {
            let mut workflows = self.workflows.write().await;
            workflows.remove(&workflow_id);
            return Err(error.context("Failed to persist workflow metadata"));
        }

        Ok(workflow_id)
    }

    /// Restore a previously persisted workflow definition and metadata without
    /// re-emitting persistence writes.
    pub async fn hydrate_workflow(&self, metadata: WorkflowMetadata) -> Result<()> {
        metadata
            .definition
            .validate()
            .context("Invalid workflow definition")?;

        DagExecutor::from_workflow(&metadata.definition)
            .context("Failed to build DAG from persisted workflow")?;

        let mut workflows = self.workflows.write().await;
        if workflows.contains_key(&metadata.id) {
            anyhow::bail!("Workflow already exists: {}", metadata.id);
        }

        workflows.insert(
            metadata.id.clone(),
            RegisteredWorkflow::from_metadata(metadata),
        );

        Ok(())
    }

    /// Get workflow by ID (returns full metadata)
    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowMetadata>> {
        let workflows = self.workflows.read().await;
        Ok(workflows
            .get(workflow_id)
            .map(RegisteredWorkflow::to_metadata))
    }

    /// Get workflow definition only
    pub async fn get_workflow_definition(&self, workflow_id: &str) -> Result<WorkflowDefinition> {
        let workflows = self.workflows.read().await;
        workflows
            .get(workflow_id)
            .map(|w| w.definition.clone())
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))
    }

    /// List all registered workflows (returns full metadata)
    pub async fn list_workflows(&self) -> Result<Vec<(String, WorkflowMetadata)>> {
        let workflows = self.workflows.read().await;
        Ok(workflows
            .iter()
            .map(|(id, w)| (id.clone(), w.to_metadata()))
            .collect())
    }

    /// List workflows as summaries
    pub async fn list_workflow_summaries(&self) -> Vec<WorkflowSummary> {
        let workflows = self.workflows.read().await;
        workflows
            .values()
            .map(|w| WorkflowSummary {
                id: w.id.clone(),
                name: w.name.clone(),
                version: w.version.clone(),
                step_count: w.definition.steps.len(),
                created_at: w.created_at,
                execution_count: w.execution_count,
                last_executed_at: w.last_executed_at,
            })
            .collect()
    }

    /// Delete workflow
    pub async fn delete_workflow(&self, workflow_id: &str) -> Result<()> {
        let mut workflows = self.workflows.write().await;
        let deleted_workflow = workflows
            .remove(workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        drop(workflows);

        if let Err(error) = self.persist_workflow_metadata_change(WorkflowPersistenceEvent::Delete(
            workflow_id.to_string(),
        )) {
            let mut workflows = self.workflows.write().await;
            workflows.insert(workflow_id.to_string(), deleted_workflow);
            return Err(error.context("Failed to delete persisted workflow metadata"));
        }
        Ok(())
    }

    /// Update workflow (creates new version)
    pub async fn update_workflow(
        &self,
        workflow_id: &str,
        name: String,
        description: Option<String>,
        tags: Vec<String>,
        definition: WorkflowDefinition,
    ) -> Result<WorkflowMetadata> {
        // Validate new definition
        definition
            .validate()
            .context("Invalid workflow definition")?;

        DagExecutor::from_workflow(&definition).context("Failed to build DAG from workflow")?;

        let mut workflows = self.workflows.write().await;
        let workflow = workflows
            .get_mut(workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        let original_workflow = workflow.clone();

        // Increment version
        let version_parts: Vec<&str> = workflow.version.split('.').collect();
        let major: u32 = version_parts[0].parse().unwrap_or(1);
        let new_version = format!("{}.0.0", major + 1);

        workflow.name = name;
        workflow.description = description;
        workflow.tags = tags;
        workflow.definition = definition;
        workflow.version = new_version;
        let updated_metadata = workflow.to_metadata();
        drop(workflows);

        if let Err(error) = self.persist_workflow_metadata_change(WorkflowPersistenceEvent::Upsert(
            updated_metadata.clone(),
        )) {
            let mut workflows = self.workflows.write().await;
            workflows.insert(workflow_id.to_string(), original_workflow);
            return Err(error.context("Failed to persist updated workflow metadata"));
        }

        Ok(updated_metadata)
    }

    /// Record a workflow execution (increment count and update timestamp)
    pub async fn record_execution(&self, workflow_id: &str) -> Result<()> {
        let mut workflows = self.workflows.write().await;
        let workflow = workflows
            .get_mut(workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        let previous_execution_count = workflow.execution_count;
        let previous_last_executed_at = workflow.last_executed_at;
        workflow.execution_count += 1;
        workflow.last_executed_at = Some(chrono::Utc::now());
        let updated_metadata = workflow.to_metadata();
        drop(workflows);

        if let Err(error) = self
            .persist_workflow_metadata_change(WorkflowPersistenceEvent::Upsert(updated_metadata))
        {
            let mut workflows = self.workflows.write().await;
            if let Some(workflow) = workflows.get_mut(workflow_id) {
                workflow.execution_count = previous_execution_count;
                workflow.last_executed_at = previous_last_executed_at;
            }
            return Err(error.context("Failed to persist workflow execution metadata"));
        }

        Ok(())
    }

    /// Validate workflow without registering
    pub fn validate_workflow(&self, definition: &WorkflowDefinition) -> Result<()> {
        definition.validate()?;
        DagExecutor::from_workflow(definition)?;
        Ok(())
    }

    /// Execute a registered workflow
    pub async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
        context: &HashMap<String, String>,
    ) -> Result<WorkflowResult> {
        // Check execution capabilities
        let (model_invoker, rule_executor) = match (&self.model_invoker, &self.rule_executor) {
            (Some(m), Some(r)) => (m.clone(), r.clone()),
            _ => anyhow::bail!(
                "Workflow engine not configured for execution. Use new_with_execution()"
            ),
        };

        // Get workflow definition
        let definition = self.get_workflow_definition(workflow_id).await?;

        // Create executor with or without lineage tracking
        let mut executor = if let Some(ref tracker) = self.lineage_tracker {
            WorkflowExecutor::with_lineage(
                definition,
                model_invoker,
                rule_executor,
                tracker.clone(),
            )
            .context("Failed to create workflow executor with lineage")?
        } else {
            WorkflowExecutor::new(definition, model_invoker, rule_executor)
                .context("Failed to create workflow executor")?
        };

        // Inject transformer callback if available
        if let Some(callback) = &self.transformer_callback {
            executor = executor.with_transformer_callback(callback.clone());
        }

        // Inject DB loader callback if available
        if let Some(callback) = &self.db_loader_callback {
            executor = executor.with_db_loader_callback(callback.clone());
        }

        // Inject DB extract callback if available
        if let Some(callback) = &self.db_extract_callback {
            executor = executor.with_db_extract_callback(callback.clone());
        }

        if let Some(callback) = &self.sos_validation_callback {
            executor = executor.with_sos_validation_callback(callback.clone());
        }

        // Create execution context
        let mut exec_context = ExecutionContext::from_input_value(input)?;
        exec_context.metadata = context.clone();
        exec_context.workflow_id = Some(workflow_id.to_string());

        // Execute workflow
        let result = executor
            .execute(exec_context)
            .await
            .context("Workflow execution failed")?;

        // Record execution metadata. The workflow has already executed at this point,
        // so a metadata persistence miss should not flip a successful run into a
        // failed one.
        if let Err(error) = self.record_execution(workflow_id).await {
            tracing::warn!(
                workflow_id = workflow_id,
                "Failed to persist workflow execution metadata: {}",
                error
            );
        }

        // Persist to RDF if callback is set
        if let Some(ref callback) = self.rdf_persistence_callback {
            callback(workflow_id, &result).context("Failed to persist workflow result to RDF")?;
        }

        Ok(result)
    }

    /// Execute workflow with graph-native input (NEW - Phase 1)
    ///
    /// Accepts `WorkflowInput` which can be:
    /// - SPARQL query to select data from the graph
    /// - Entity filter to select by type/time range
    /// - Legacy JSON for backward compatibility
    ///
    /// Requires an `InputAdapter` to convert WorkflowInput → ExecutionContext.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use graphica_core::orchestration::workflow::{
    ///     WorkflowInput, SparqlInputAdapter,
    /// };
    ///
    /// let input = WorkflowInput::SparqlQuery {
    ///     query: "SELECT ?customer WHERE { ?customer a gph:Customer }".to_string(),
    ///     graph: Some("http://graphica.io/latest".to_string()),
    ///     batch_size: Some(100),
    ///     limit: None,
    /// };
    ///
    /// // Engine.execute_workflow_with_input(workflow_id, input, adapter, context).await
    /// ```
    pub async fn execute_workflow_with_input(
        &self,
        workflow_id: &str,
        input: super::input::WorkflowInput,
        adapter: Arc<dyn super::input::InputAdapter>,
        context: &HashMap<String, String>,
    ) -> Result<Vec<WorkflowResult>> {
        // Validate input
        input.validate().context("Invalid workflow input")?;

        // Check execution capabilities
        let (model_invoker, rule_executor) = match (&self.model_invoker, &self.rule_executor) {
            (Some(m), Some(r)) => (m.clone(), r.clone()),
            _ => anyhow::bail!(
                "Workflow engine not configured for execution. Use new_with_execution()"
            ),
        };

        // Get workflow definition
        let definition = self.get_workflow_definition(workflow_id).await?;

        // Create executor with or without lineage tracking
        let mut executor = if let Some(ref tracker) = self.lineage_tracker {
            WorkflowExecutor::with_lineage(
                definition,
                model_invoker,
                rule_executor,
                tracker.clone(),
            )
            .context("Failed to create workflow executor with lineage")?
        } else {
            WorkflowExecutor::new(definition, model_invoker, rule_executor)
                .context("Failed to create workflow executor")?
        };

        // Inject transformer callback if available
        if let Some(callback) = &self.transformer_callback {
            executor = executor.with_transformer_callback(callback.clone());
        }

        // Inject DB loader callback if available
        if let Some(callback) = &self.db_loader_callback {
            executor = executor.with_db_loader_callback(callback.clone());
        }

        // Inject DB extract callback if available
        if let Some(callback) = &self.db_extract_callback {
            executor = executor.with_db_extract_callback(callback.clone());
        }

        if let Some(callback) = &self.sos_validation_callback {
            executor = executor.with_sos_validation_callback(callback.clone());
        }

        // Prepare execution contexts using adapter
        let mut exec_contexts = adapter
            .prepare_context(&input)
            .await
            .context("Failed to prepare execution context from input")?;

        // Add metadata to all contexts and enable row lineage if tracker is present
        let has_lineage_tracker = self.lineage_tracker.is_some();
        for ctx in &mut exec_contexts {
            ctx.metadata = context.clone();

            // Enable row-level lineage tracking if we have a tracker
            if has_lineage_tracker && ctx.row_lineage.is_none() {
                // Generate execution context identifiers
                let execution_id = format!("exec_{}", uuid::Uuid::new_v4());
                let job_id = context
                    .get("job_id")
                    .cloned()
                    .unwrap_or_else(|| format!("job_{}", uuid::Uuid::new_v4()));
                let tenant_id = context
                    .get("tenant_id")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());

                ctx.row_lineage = Some(RowLineageContext::new(execution_id, job_id, tenant_id));
            }
        }

        // Execute workflow for each context (batched execution)
        let mut results = Vec::new();
        for exec_context in exec_contexts {
            let result = executor
                .execute(exec_context)
                .await
                .context("Workflow execution failed")?;

            // Persist to RDF if callback is set
            if let Some(ref callback) = self.rdf_persistence_callback {
                callback(workflow_id, &result)
                    .context("Failed to persist workflow result to RDF")?;
            }

            results.push(result);
        }

        // Record execution once (not per batch). See execute_workflow() for why
        // persistence failures are logged instead of bubbling after work completes.
        if let Err(error) = self.record_execution(workflow_id).await {
            tracing::warn!(
                workflow_id = workflow_id,
                "Failed to persist workflow execution metadata: {}",
                error
            );
        }

        Ok(results)
    }

    /// Execute a single workflow step (for testing)
    pub async fn execute_step(
        &self,
        step: &WorkflowStep,
        input: serde_json::Value,
        context: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        // Check execution capabilities
        let (model_invoker, rule_executor) = match (&self.model_invoker, &self.rule_executor) {
            (Some(m), Some(r)) => (m.clone(), r.clone()),
            _ => anyhow::bail!("Workflow engine not configured for execution"),
        };

        // Create minimal workflow with single step
        let workflow = WorkflowDefinition {
            steps: vec![step.clone()],
            fusion_threshold: 0.8,
            fallback: super::definition::FallbackStrategy::ManualReview,
        };

        // Create executor
        let executor = WorkflowExecutor::new(workflow, model_invoker, rule_executor)?;

        // Create execution context
        let mut exec_context = ExecutionContext::from_input_value(input)?;
        exec_context.metadata = context.clone();

        // Execute workflow (single step)
        let result = executor.execute(exec_context).await?;

        // Return the step output
        result
            .step_results
            .get(&step.id)
            .map(|step_result| step_result.output.clone())
            .ok_or_else(|| anyhow::anyhow!("Step result not found"))
    }

    // ============================================================================
    // Scheduling Methods (Stub - Not Yet Implemented)
    // ============================================================================

    /// Schedule workflow for recurring execution (STUB)
    #[allow(clippy::too_many_arguments)]
    pub async fn schedule_workflow(
        &self,
        _workflow_id: &str,
        _schedule_id: &str,
        _cron_expression: Option<String>,
        _interval_seconds: Option<u64>,
        _input: serde_json::Value,
        _context: HashMap<String, String>,
        _enabled: bool,
    ) -> Result<()> {
        anyhow::bail!("Workflow scheduling not yet implemented. Coming in next release.")
    }

    /// Get scheduled executions for a workflow (STUB)
    pub async fn get_workflow_schedules(
        &self,
        _workflow_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        // Return empty list for now
        Ok(vec![])
    }

    /// Cancel scheduled workflow execution (STUB)
    pub async fn unschedule_workflow(&self, _workflow_id: &str) -> Result<()> {
        Ok(()) // No-op since scheduling isn't implemented
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Workflow summary for listings
#[derive(Debug, Clone)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub step_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub execution_count: u64,
    pub last_executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::workflow::definition::{
        ConfidenceGateConfig, DataValidatorConfig, FallbackStrategy, FieldTransformation,
        FieldTransformerConfig, RuleType, Severity, SosValidationConfig, SosValidationSpec,
        StepConfig, StepType, TransformOperation, ValidationRule, WorkflowStep,
    };
    use crate::orchestration::workflow::input::{
        DatasetInputAdapter, DatasetResolver, WorkflowInput,
    };
    use serde_json::Value as JsonValue;

    fn create_test_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.8,
                    input_step: None,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    struct EngineDatasetResolver {
        rows: Vec<JsonValue>,
    }

    #[async_trait::async_trait]
    impl DatasetResolver for EngineDatasetResolver {
        async fn load_rows(
            &self,
            _dataset_id: &str,
            limit: Option<usize>,
        ) -> Result<Vec<JsonValue>> {
            Ok(match limit {
                Some(limit) => self.rows.iter().take(limit).cloned().collect(),
                None => self.rows.clone(),
            })
        }
    }

    fn create_batch_transform_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "transform1".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "status".to_string(),
                        operations: vec![TransformOperation::Lower],
                    }],
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    fn create_batch_transform_and_validate_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "transform1".to_string(),
                    step_type: StepType::FieldTransformer,
                    config: StepConfig::FieldTransformer(FieldTransformerConfig {
                        transformations: vec![FieldTransformation {
                            field: "status".to_string(),
                            operations: vec![TransformOperation::Lower],
                        }],
                    }),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "validate1".to_string(),
                    step_type: StepType::DataValidator,
                    config: StepConfig::DataValidator(DataValidatorConfig {
                        rules: vec![ValidationRule {
                            field: "status".to_string(),
                            rule_type: RuleType::InSet {
                                values: vec!["active".to_string(), "pending".to_string()],
                            },
                            params: None,
                            severity: Severity::Error,
                        }],
                        fail_on_error: false,
                    }),
                    depends_on: vec!["transform1".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    fn create_sos_validation_definition(blocking_severities: Vec<String>) -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "sos_validate".to_string(),
                step_type: StepType::SosValidation,
                config: StepConfig::SosValidation(SosValidationConfig {
                    validation: SosValidationSpec::ContractCompliance {
                        contract_id: "contract-1".to_string(),
                    },
                    blocking_severities,
                    persist_report: true,
                    emit_graph_lineage: true,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    #[tokio::test]
    async fn test_register_workflow() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        assert!(workflow_id.starts_with("wf_"));
    }

    #[tokio::test]
    async fn test_get_workflow() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition.clone(),
                None,
                vec![],
            )
            .await
            .unwrap();

        let retrieved = engine.get_workflow(&workflow_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().definition.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_list_workflows() {
        let engine = WorkflowEngine::new();

        let def1 = create_test_definition();
        let def2 = create_test_definition();

        engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "workflow1".to_string(),
                def1,
                None,
                vec![],
            )
            .await
            .unwrap();
        engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "workflow2".to_string(),
                def2,
                None,
                vec![],
            )
            .await
            .unwrap();

        let list = engine.list_workflows().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_workflow() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        engine.delete_workflow(&workflow_id).await.unwrap();

        let result = engine.get_workflow(&workflow_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_workflow() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        let mut new_definition = create_test_definition();
        new_definition.fusion_threshold = 0.9;

        let updated_metadata = engine
            .update_workflow(
                &workflow_id,
                "renamed_workflow".to_string(),
                Some("Updated description".to_string()),
                vec!["production".to_string()],
                new_definition,
            )
            .await
            .unwrap();

        let updated = engine.get_workflow(&workflow_id).await.unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.definition.fusion_threshold, 0.9);
        assert_eq!(updated.name, "renamed_workflow");
        assert_eq!(updated.description.as_deref(), Some("Updated description"));
        assert_eq!(updated.tags, vec!["production".to_string()]);
        assert_eq!(updated_metadata.name, updated.name);
        assert_eq!(updated_metadata.created_at, updated.created_at);
    }

    #[tokio::test]
    async fn test_hydrate_workflow_restores_metadata() {
        let engine = WorkflowEngine::new();
        let metadata = WorkflowMetadata {
            id: format!("wf_{}", Uuid::new_v4()),
            name: "restored_workflow".to_string(),
            description: Some("Recovered after restart".to_string()),
            tags: vec!["demo".to_string(), "restored".to_string()],
            definition: create_test_definition(),
            version: "3.0.0".to_string(),
            created_at: chrono::Utc::now(),
            execution_count: 7,
            last_executed_at: Some(chrono::Utc::now()),
        };

        engine.hydrate_workflow(metadata.clone()).await.unwrap();

        let restored = engine.get_workflow(&metadata.id).await.unwrap().unwrap();
        assert_eq!(restored.name, metadata.name);
        assert_eq!(restored.version, metadata.version);
        assert_eq!(restored.execution_count, metadata.execution_count);
        assert_eq!(restored.tags, metadata.tags);
    }

    #[tokio::test]
    async fn test_register_workflow_rolls_back_when_persistence_fails() {
        let engine = WorkflowEngine::new()
            .with_workflow_persistence(|_| anyhow::bail!("simulated persistence failure"));
        let workflow_id = format!("wf_{}", Uuid::new_v4());

        let result = engine
            .register_workflow(
                workflow_id.clone(),
                "persistent_workflow".to_string(),
                create_test_definition(),
                None,
                vec![],
            )
            .await;

        assert!(result.is_err());
        assert!(engine.get_workflow(&workflow_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_register_workflow_rejects_duplicate_id() {
        let engine = WorkflowEngine::new();
        let workflow_id = format!("wf_{}", Uuid::new_v4());

        engine
            .register_workflow(
                workflow_id.clone(),
                "original_workflow".to_string(),
                create_test_definition(),
                None,
                vec![],
            )
            .await
            .unwrap();

        let duplicate = engine
            .register_workflow(
                workflow_id.clone(),
                "replacement_workflow".to_string(),
                create_test_definition(),
                None,
                vec![],
            )
            .await;

        assert!(duplicate.is_err());
        assert!(duplicate
            .unwrap_err()
            .to_string()
            .contains("Workflow already exists"));
    }

    #[tokio::test]
    async fn test_validate_workflow() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let result = engine.validate_workflow(&definition);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reject_invalid_workflow() {
        let engine = WorkflowEngine::new();
        let invalid_definition = WorkflowDefinition {
            steps: vec![], // Empty steps - invalid
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let result = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "invalid".to_string(),
                invalid_definition,
                None,
                vec![],
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execution_tracking_initial_state() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        let workflow = engine.get_workflow(&workflow_id).await.unwrap().unwrap();

        // Initially, execution count should be 0 and last_executed_at should be None
        assert_eq!(workflow.execution_count, 0);
        assert!(workflow.last_executed_at.is_none());
    }

    #[tokio::test]
    async fn test_record_execution() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        // Record first execution
        engine.record_execution(&workflow_id).await.unwrap();

        let workflow = engine.get_workflow(&workflow_id).await.unwrap().unwrap();
        assert_eq!(workflow.execution_count, 1);
        assert!(workflow.last_executed_at.is_some());

        let first_execution_time = workflow.last_executed_at.unwrap();

        // Record second execution
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        engine.record_execution(&workflow_id).await.unwrap();

        let workflow = engine.get_workflow(&workflow_id).await.unwrap().unwrap();
        assert_eq!(workflow.execution_count, 2);
        assert!(workflow.last_executed_at.is_some());

        let second_execution_time = workflow.last_executed_at.unwrap();
        assert!(second_execution_time > first_execution_time);
    }

    #[tokio::test]
    async fn test_record_execution_nonexistent_workflow() {
        let engine = WorkflowEngine::new();

        let result = engine.record_execution("nonexistent_wf").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Workflow not found"));
    }

    #[tokio::test]
    async fn test_execute_workflow_with_input_preserves_dataset_batch_metadata_in_step_results() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let engine = WorkflowEngine::new_with_execution(invoker, rule_executor);

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "batch_transform_workflow".to_string(),
                create_batch_transform_definition(),
                None,
                vec![],
            )
            .await
            .unwrap();

        let adapter = Arc::new(DatasetInputAdapter::new(Arc::new(EngineDatasetResolver {
            rows: vec![
                serde_json::json!({"id": 1, "status": "ACTIVE"}),
                serde_json::json!({"id": 2, "status": "PENDING"}),
            ],
        })));

        let results = engine
            .execute_workflow_with_input(
                &workflow_id,
                WorkflowInput::Dataset {
                    dataset_id: "ds_input_123".to_string(),
                    batch_size: Some(1000),
                    limit: None,
                },
                adapter,
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        let step_result = results[0]
            .step_results
            .get("transform1")
            .expect("transform step result to be stored");

        assert_eq!(step_result.output["_rows"][0]["status"], "active");
        assert!(step_result.batch_frame.is_none());

        let batch_metadata = step_result
            .batch_metadata
            .as_ref()
            .expect("stored step result should preserve lightweight batch metadata");
        assert_eq!(batch_metadata.source_step_id, None);
        assert_eq!(batch_metadata.source_kind.as_deref(), Some("dataset_input"));
        assert_eq!(batch_metadata.source_id.as_deref(), Some("ds_input_123"));
    }

    #[tokio::test]
    async fn test_execute_workflow_with_input_preserves_ingress_metadata_across_second_batch_step()
    {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let engine = WorkflowEngine::new_with_execution(invoker, rule_executor);

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "batch_transform_validate_workflow".to_string(),
                create_batch_transform_and_validate_definition(),
                None,
                vec![],
            )
            .await
            .unwrap();

        let adapter = Arc::new(DatasetInputAdapter::new(Arc::new(EngineDatasetResolver {
            rows: vec![
                serde_json::json!({"id": 1, "status": "ACTIVE"}),
                serde_json::json!({"id": 2, "status": "PENDING"}),
            ],
        })));

        let results = engine
            .execute_workflow_with_input(
                &workflow_id,
                WorkflowInput::Dataset {
                    dataset_id: "ds_input_chain_456".to_string(),
                    batch_size: Some(1000),
                    limit: None,
                },
                adapter,
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);

        let transform_result = results[0]
            .step_results
            .get("transform1")
            .expect("transform step result to be stored");
        let validate_result = results[0]
            .step_results
            .get("validate1")
            .expect("validate step result to be stored");

        assert_eq!(transform_result.output["_rows"][0]["status"], "active");
        assert_eq!(validate_result.output["_error_count"], 0);

        let transform_metadata = transform_result
            .batch_metadata
            .as_ref()
            .expect("transform step should preserve lightweight ingress metadata");
        let validate_metadata = validate_result
            .batch_metadata
            .as_ref()
            .expect("validate step should preserve lightweight ingress metadata");

        assert_eq!(transform_metadata.source_step_id, None);
        assert_eq!(validate_metadata.source_step_id, None);
        assert_eq!(
            transform_metadata.source_kind.as_deref(),
            Some("dataset_input")
        );
        assert_eq!(
            validate_metadata.source_kind.as_deref(),
            Some("dataset_input")
        );
        assert_eq!(
            transform_metadata.source_id.as_deref(),
            Some("ds_input_chain_456")
        );
        assert_eq!(
            validate_metadata.source_id.as_deref(),
            Some("ds_input_chain_456")
        );
    }

    #[tokio::test]
    async fn test_sos_validation_step_fails_workflow_on_blocking_severity() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::{
            ExecutionContext, SosValidationCallback, SosValidationCheck, SosValidationStepResult,
        };
        use std::future::Future;
        use std::pin::Pin;

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let callback: Arc<SosValidationCallback> = Arc::new(Box::new(
            |_config: &SosValidationConfig, _context: &ExecutionContext| {
                Box::pin(async move {
                    Ok(SosValidationStepResult {
                        validation_id: "validation-1".to_string(),
                        passed: false,
                        checks: vec![SosValidationCheck {
                            check_name: "schema_compatibility".to_string(),
                            passed: false,
                            severity: "error".to_string(),
                            description: "Synthetic blocking failure".to_string(),
                            details: None,
                        }],
                        confidence: 0.0,
                        validated_at: "2026-04-13T00:00:00Z".to_string(),
                        report_id: Some("report-1".to_string()),
                    })
                })
                    as Pin<Box<dyn Future<Output = anyhow::Result<SosValidationStepResult>> + Send>>
            },
        ));

        let engine = WorkflowEngine::new_with_execution(invoker, rule_executor)
            .with_sos_validation_callback(callback);

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "sos_blocking_validation".to_string(),
                create_sos_validation_definition(vec!["error".to_string()]),
                None,
                vec![],
            )
            .await
            .unwrap();

        let result = engine
            .execute_workflow(&workflow_id, serde_json::json!({}), &HashMap::new())
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Step 'sos_validate' failed"));

        let step = result
            .step_results
            .get("sos_validate")
            .expect("sos validation step should be recorded");
        assert!(!step.success);
        assert_eq!(step.output["report_id"], "report-1");
        assert_eq!(step.output["step_passed"], false);
    }

    #[tokio::test]
    async fn test_sos_validation_step_allows_non_blocking_warning() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::{
            ExecutionContext, SosValidationCallback, SosValidationCheck, SosValidationStepResult,
        };
        use std::future::Future;
        use std::pin::Pin;

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let callback: Arc<SosValidationCallback> = Arc::new(Box::new(
            |_config: &SosValidationConfig, _context: &ExecutionContext| {
                Box::pin(async move {
                    Ok(SosValidationStepResult {
                        validation_id: "validation-2".to_string(),
                        passed: false,
                        checks: vec![SosValidationCheck {
                            check_name: "policy_warning".to_string(),
                            passed: false,
                            severity: "warning".to_string(),
                            description: "Synthetic non-blocking warning".to_string(),
                            details: None,
                        }],
                        confidence: 0.5,
                        validated_at: "2026-04-13T00:00:01Z".to_string(),
                        report_id: Some("report-2".to_string()),
                    })
                })
                    as Pin<Box<dyn Future<Output = anyhow::Result<SosValidationStepResult>> + Send>>
            },
        ));

        let engine = WorkflowEngine::new_with_execution(invoker, rule_executor)
            .with_sos_validation_callback(callback);

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "sos_warning_validation".to_string(),
                create_sos_validation_definition(vec!["error".to_string()]),
                None,
                vec![],
            )
            .await
            .unwrap();

        let result = engine
            .execute_workflow(&workflow_id, serde_json::json!({}), &HashMap::new())
            .await
            .unwrap();

        assert!(result.success);

        let step = result
            .step_results
            .get("sos_validate")
            .expect("sos validation step should be recorded");
        assert!(step.success);
        assert_eq!(step.output["report_id"], "report-2");
        assert_eq!(step.output["step_passed"], true);
        assert_eq!(
            step.output["blocking_severities"],
            serde_json::json!(["error"])
        );
    }

    #[tokio::test]
    async fn test_list_workflow_summaries_with_execution_tracking() {
        let engine = WorkflowEngine::new();
        let definition = create_test_definition();

        let workflow_id = engine
            .register_workflow(
                format!("wf_{}", Uuid::new_v4()),
                "test_workflow".to_string(),
                definition,
                None,
                vec![],
            )
            .await
            .unwrap();

        // Before execution
        let summaries = engine.list_workflow_summaries().await;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].execution_count, 0);
        assert!(summaries[0].last_executed_at.is_none());

        // After execution
        engine.record_execution(&workflow_id).await.unwrap();

        let summaries = engine.list_workflow_summaries().await;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].execution_count, 1);
        assert!(summaries[0].last_executed_at.is_some());
    }
}
