//! Batch Job Executor
//!
//! Orchestrates parallel execution of multiple workflow instances as a batch job.
//!
//! ## Architecture
//!
//! ```text
//! BatchJobExecutor
//!   ├─ Dependency Resolver (DAG → execution waves)
//!   ├─ Semaphore Pool (max_parallel limit)
//!   ├─ Retry Manager (exponential backoff)
//!   └─ Progress Tracker (real-time updates)
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use graphica_coordinator::api::file_library::storage::FileLibraryStorage;
//! use graphica_coordinator::api::file_library::storage_trait::FileLibraryStore;
//! use graphica_coordinator::workflows::engine::BatchJobExecutor;
//! use graphica_coordinator::workflows::storage::{BatchJobStore, WorkflowStore, ExecutionStore};
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! # let batch_store = Arc::new(BatchJobStore::open("/tmp/batch")?);
//! # let workflow_store = Arc::new(WorkflowStore::new());
//! # let execution_store = Arc::new(ExecutionStore::new());
//! let file_store: Arc<dyn FileLibraryStore> = Arc::new(FileLibraryStorage::new());
//! let executor = BatchJobExecutor::new(
//!     batch_store,
//!     workflow_store,
//!     execution_store,
//!     file_store,
//! );
//!
//! executor.execute("batch_123".to_string()).await?;
//! # Ok(())
//! # }
//! ```

use crate::api::file_library::storage::FileLibraryStorage;
use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::etl::loaders::database::LoadMode;
use crate::workflows::domain::{
    create_reader, Action, ActionResult, BatchJob, BatchJobStatus, DataSource, DatabaseType,
    ExecutionStatus, WorkflowExecution, WorkflowExecutionRef, WorkflowExecutionStatus,
};
use crate::workflows::engine::{
    ActionExecutor, DataLoader, ExecutionContext, LoadConfig, PreflightResult, PreflightValidator,
    WorkflowRouter,
};
use crate::workflows::storage::{BatchJobStore, ExecutionStore, WorkflowStore};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Batch job executor with parallel execution and dependency management
pub struct BatchJobExecutor {
    /// Batch job storage
    batch_store: Arc<BatchJobStore>,

    /// Workflow definitions
    workflow_store: Arc<WorkflowStore>,

    /// Workflow execution tracking
    execution_store: Arc<ExecutionStore>,

    /// File library storage for preflight validation
    file_store: Arc<dyn FileLibraryStore>,

    /// Maximum concurrent workflow executions (default limit)
    default_max_concurrent: usize,

    /// Optional production rule executor for real rule execution
    rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
}

impl BatchJobExecutor {
    /// Create a new batch job executor
    pub fn new(
        batch_store: Arc<BatchJobStore>,
        workflow_store: Arc<WorkflowStore>,
        execution_store: Arc<ExecutionStore>,
        file_store: Arc<dyn FileLibraryStore>,
    ) -> Self {
        Self {
            batch_store,
            workflow_store,
            execution_store,
            file_store,
            default_max_concurrent: 16, // Increased from 4 to 16 for I/O-bound workloads
            rule_executor: None,
        }
    }

    /// Create batch job executor with production components
    pub fn with_rule_executor(
        batch_store: Arc<BatchJobStore>,
        workflow_store: Arc<WorkflowStore>,
        execution_store: Arc<ExecutionStore>,
        file_store: Arc<dyn FileLibraryStore>,
        rule_executor: Arc<graphica_core::orchestration::rules::RuleExecutor>,
    ) -> Self {
        Self {
            batch_store,
            workflow_store,
            execution_store,
            file_store,
            default_max_concurrent: 16, // Increased from 4 to 16 for I/O-bound workloads
            rule_executor: Some(rule_executor),
        }
    }

    /// Run preflight validation on a batch job
    ///
    /// Validates a batch job without executing it. This should be called before
    /// execution to catch errors early.
    ///
    /// Returns a PreflightResult with validation errors, warnings, and estimates.
    pub async fn preflight_validate(&self, job_id: &str) -> Result<PreflightResult> {
        info!("Running preflight validation for batch job: {}", job_id);

        // Load batch job
        let batch_job = self
            .batch_store
            .get(job_id)?
            .ok_or_else(|| anyhow!("Batch job not found: {}", job_id))?;

        // Run validation with file library integration
        let validator = PreflightValidator::new(self.file_store.clone());
        let result = validator.validate(&batch_job).await?;

        // Update batch job status if validation failed
        if !result.is_valid() {
            info!(
                "Preflight validation failed for batch job {}: {} errors",
                job_id,
                result.errors.len()
            );
        } else {
            info!("Preflight validation passed for batch job {}", job_id);
        }

        Ok(result)
    }

    /// Execute a batch job by ID
    ///
    /// This is the main entry point. It:
    /// 1. Loads the batch job from storage
    /// 2. Validates the workflow exists
    /// 3. Resolves dependencies into execution waves
    /// 4. Executes workflows in parallel (respecting semaphore)
    /// 5. Handles retries and error recovery
    /// 6. Updates progress in real-time
    pub async fn execute(&self, job_id: String) -> Result<()> {
        info!("Starting batch job execution: {}", job_id);

        // Load batch job
        let mut batch_job = self
            .batch_store
            .get(&job_id)?
            .ok_or_else(|| anyhow!("Batch job not found: {}", job_id))?;

        // Validate workflow exists
        let workflow = self
            .workflow_store
            .get(&batch_job.workflow_id)?
            .ok_or_else(|| anyhow!("Workflow not found: {}", batch_job.workflow_id))?;

        if !workflow.enabled {
            return Err(anyhow!("Workflow '{}' is disabled", workflow.id));
        }

        // Validate batch job configuration
        batch_job
            .validate()
            .map_err(|e| anyhow!("Batch job validation failed: {}", e))?;

        info!(
            "Executing batch job '{}' with {} workflows (max_parallel={})",
            batch_job.name,
            batch_job.workflow_executions.len(),
            batch_job.config.max_parallel
        );

        // Update status to Running
        batch_job.update_status(BatchJobStatus::Running);
        self.batch_store.update(batch_job.clone())?;

        // Create semaphore for concurrency control
        let semaphore = Arc::new(Semaphore::new(batch_job.config.max_parallel));

        // Resolve dependencies into execution waves
        let waves = self.resolve_dependencies(&batch_job.workflow_executions)?;

        info!("Resolved {} execution waves", waves.len());

        // Execute waves sequentially, workflows within each wave in parallel
        let mut completed_ids = HashSet::new();
        let mut failed_ids = HashSet::new();

        for (wave_idx, wave) in waves.iter().enumerate() {
            info!(
                "Executing wave {} with {} workflows",
                wave_idx + 1,
                wave.len()
            );

            let wave_results = self
                .execute_wave(&batch_job, wave, semaphore.clone(), &workflow)
                .await;

            // Process wave results
            for (exec_id, result) in wave_results {
                match result {
                    Ok(_) => {
                        completed_ids.insert(exec_id.clone());
                    }
                    Err(e) => {
                        error!("Workflow execution {} failed: {}", exec_id, e);
                        failed_ids.insert(exec_id.clone());

                        // Stop on error if configured
                        if batch_job.config.stop_on_error {
                            error!("Stopping batch job due to error");
                            batch_job.update_status(BatchJobStatus::Failed);
                            self.batch_store.update(batch_job.clone())?;
                            return Err(anyhow!("Batch job stopped due to error: {}", e));
                        }
                    }
                }
            }

            // Update progress after each wave
            self.update_batch_progress(&mut batch_job).await?;
        }

        // Determine final status
        let final_status = if failed_ids.is_empty() {
            BatchJobStatus::Completed
        } else if completed_ids.is_empty() {
            BatchJobStatus::Failed
        } else {
            BatchJobStatus::PartiallyCompleted
        };

        batch_job.update_status(final_status);
        batch_job.recalculate_progress();
        self.batch_store.update(batch_job.clone())?;

        info!(
            "Batch job '{}' completed with status {:?} (completed={}, failed={})",
            batch_job.name,
            final_status,
            completed_ids.len(),
            failed_ids.len()
        );

        Ok(())
    }

    /// Resume a paused workflow execution from its checkpoint
    ///
    /// This method is called when an approval is granted or a wait condition is met.
    /// It:
    /// 1. Loads the paused execution from storage
    /// 2. Validates it's in Paused status with a valid checkpoint
    /// 3. Loads the workflow definition
    /// 4. Restores intermediate data from checkpoint
    /// 5. Resumes execution from the next action after the pause point
    ///
    /// ## Error Handling
    /// - Returns error if execution not found or not in Paused status
    /// - Returns error if checkpoint data is missing or invalid
    /// - Returns error if workflow definition not found
    /// - Propagates execution errors from resumed actions
    pub async fn resume_execution(&self, execution_id: String) -> Result<()> {
        info!("Resuming workflow execution: {}", execution_id);

        // Load execution record
        let mut execution = self
            .execution_store
            .get(&execution_id)
            .await?
            .ok_or_else(|| anyhow!("Execution not found: {}", execution_id))?;

        // Validate execution is paused
        if execution.status != ExecutionStatus::Paused {
            return Err(anyhow!(
                "Cannot resume execution {} - status is {:?}, expected Paused",
                execution_id,
                execution.status
            ));
        }

        // Extract checkpoint data
        let checkpoint_action_index = execution
            .checkpoint_action_index()
            .ok_or_else(|| anyhow!("No checkpoint found for paused execution {}", execution_id))?;

        let checkpoint_data = execution
            .checkpoint_data()
            .ok_or_else(|| anyhow!("No checkpoint data found for execution {}", execution_id))?;

        // Extract intermediate data from checkpoint
        let mut intermediate_data = checkpoint_data
            .get("intermediate_data")
            .ok_or_else(|| anyhow!("Checkpoint missing intermediate_data field"))?
            .clone();

        info!(
            "Resuming from checkpoint: action_index={}, workflow_id={}",
            checkpoint_action_index, execution.workflow_id
        );

        // Load workflow definition
        let workflow = self
            .workflow_store
            .get(&execution.workflow_id)?
            .ok_or_else(|| anyhow!("Workflow not found: {}", execution.workflow_id))?;

        // Get the route (using first route for now - TODO: support route selection)
        let route = workflow
            .routes
            .first()
            .ok_or_else(|| anyhow!("Workflow has no routes"))?;

        // Calculate which actions to execute (all actions after the paused one)
        let resume_from_index = checkpoint_action_index + 1;
        if resume_from_index >= route.actions.len() {
            info!(
                "No remaining actions to execute for {} (paused at last action)",
                execution_id
            );
            execution.update_status(ExecutionStatus::Completed);
            self.execution_store.update(execution).await?;
            return Ok(());
        }

        let remaining_actions = &route.actions[resume_from_index..];
        info!(
            "Executing {} remaining actions (out of {} total)",
            remaining_actions.len(),
            route.actions.len()
        );

        // Create execution context for resume
        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route.id.clone(),
            input_data: intermediate_data.clone(),
            rule_executor: self.rule_executor.clone(),
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            manual_mapping_store: None,
            execution_id: Some(execution_id.clone()),
            action_index: resume_from_index,
            metrics: None,
            approval_store: None,
            execution_store: Some(self.execution_store.clone()),
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        // Update execution status to Running
        execution.update_status(ExecutionStatus::Running);
        execution.clear_checkpoint(); // Clear old checkpoint
        self.execution_store.update(execution.clone()).await?;

        // Execute remaining actions
        let results =
            ActionExecutor::execute_actions(remaining_actions, &mut intermediate_data, &context)
                .await?;

        // Check if workflow paused again
        let paused_action = results
            .iter()
            .find(|r| r.status == crate::workflows::domain::ActionStatus::Paused);

        if let Some(paused_result) = paused_action {
            // Paused again - save new checkpoint
            info!(
                "Workflow paused again at action {} ({})",
                resume_from_index + results.len(),
                paused_result.action_type
            );

            let new_checkpoint_index = resume_from_index + results.len() - 1;
            let checkpoint_data = serde_json::json!({
                "paused_action_type": paused_result.action_type,
                "paused_action_output": paused_result.output,
                "intermediate_data": intermediate_data,
                "total_actions": route.actions.len(),
                "executed_actions": new_checkpoint_index + 1,
            });

            let mut execution = self
                .execution_store
                .get(&execution_id)
                .await?
                .ok_or_else(|| anyhow!("Execution not found during checkpoint save"))?;

            execution.checkpoint(new_checkpoint_index, checkpoint_data);
            execution.update_status(ExecutionStatus::Paused);

            // Append new action results to existing ones
            execution.action_results.extend(results);

            self.execution_store.update(execution).await?;

            return Ok(());
        }

        // Check for failures
        let failed_actions: Vec<&ActionResult> = results
            .iter()
            .filter(|r| r.status == crate::workflows::domain::ActionStatus::Failed)
            .collect();

        if !failed_actions.is_empty() {
            execution.update_status(ExecutionStatus::Failed);
            self.execution_store.update(execution).await?;
            return Err(anyhow!(
                "Workflow execution failed after resume: {} actions failed",
                failed_actions.len()
            ));
        }

        // All actions completed successfully
        execution.update_status(ExecutionStatus::Completed);

        // Append new action results to existing ones
        execution.action_results.extend(results);

        self.execution_store.update(execution).await?;

        info!("Workflow execution resumed and completed: {}", execution_id);

        Ok(())
    }

    /// Execute a single wave of workflows in parallel
    async fn execute_wave(
        &self,
        batch_job: &BatchJob,
        wave: &[WorkflowExecutionRef],
        semaphore: Arc<Semaphore>,
        workflow: &crate::workflows::domain::Workflow,
    ) -> HashMap<String, Result<()>> {
        let mut results = HashMap::new();
        let mut handles = Vec::new();

        // Spawn tasks for all workflows in this wave
        for exec_ref in wave {
            let exec_ref = exec_ref.clone();
            let batch_job = batch_job.clone();
            let workflow = workflow.clone();
            let semaphore = semaphore.clone();
            let execution_store = self.execution_store.clone();
            let batch_store = self.batch_store.clone();
            let rule_executor = self.rule_executor.clone();

            let handle = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = semaphore.acquire().await.unwrap();

                info!("Executing workflow for: {}", exec_ref.display_name());

                // Execute workflow with retry logic
                let result = Self::execute_with_retry(
                    &exec_ref,
                    &batch_job,
                    &workflow,
                    &execution_store,
                    &batch_store,
                    rule_executor,
                    None, // transformer_registry not available in batch executor
                )
                .await;

                (exec_ref.execution_id.clone(), result)
            });

            handles.push(handle);
        }

        // Wait for all workflows in this wave to complete
        for handle in handles {
            match handle.await {
                Ok((exec_id, result)) => {
                    results.insert(exec_id, result);
                }
                Err(e) => {
                    error!("Task panicked: {}", e);
                }
            }
        }

        results
    }

    /// Execute a single workflow with retry logic
    async fn execute_with_retry(
        exec_ref: &WorkflowExecutionRef,
        batch_job: &BatchJob,
        workflow: &crate::workflows::domain::Workflow,
        execution_store: &ExecutionStore,
        batch_store: &BatchJobStore,
        rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
        transformer_registry: Option<
            Arc<crate::workflows::engine::transformers::TransformerRegistry>,
        >,
    ) -> Result<()> {
        let max_retries = batch_job.config.max_retries;
        let mut attempt = 0;

        loop {
            attempt += 1;

            debug!(
                "Attempt {} of {} for workflow execution {}",
                attempt,
                max_retries + 1,
                exec_ref.execution_id
            );

            match Self::execute_workflow_internal(
                exec_ref,
                workflow,
                execution_store,
                rule_executor.clone(),
                transformer_registry.clone(),
            )
            .await
            {
                Ok(_) => {
                    info!(
                        "Workflow execution {} succeeded on attempt {}",
                        exec_ref.execution_id, attempt
                    );
                    return Ok(());
                }
                Err(e) => {
                    error!(
                        "Workflow execution {} failed on attempt {}: {}",
                        exec_ref.execution_id, attempt, e
                    );

                    // Check if we should retry
                    if attempt > max_retries || !batch_job.config.retry_failed {
                        return Err(e);
                    }

                    // Exponential backoff
                    let backoff_ms = 1000 * 2_u64.pow((attempt - 1) as u32);
                    let backoff = Duration::from_millis(backoff_ms.min(30000)); // Max 30s

                    warn!(
                        "Retrying workflow execution {} after {:?}",
                        exec_ref.execution_id, backoff
                    );

                    sleep(backoff).await;
                }
            }
        }
    }

    /// Execute a single workflow (internal implementation)
    async fn execute_workflow_internal(
        exec_ref: &WorkflowExecutionRef,
        workflow: &crate::workflows::domain::Workflow,
        execution_store: &ExecutionStore,
        rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
        transformer_registry: Option<
            Arc<crate::workflows::engine::transformers::TransformerRegistry>,
        >,
    ) -> Result<()> {
        // Create execution record
        let execution = WorkflowExecution::new(
            exec_ref.execution_id.clone(),
            workflow.id.clone(),
            workflow.name.clone(),
            serde_json::json!({
                "source": exec_ref.source,
                "target_table": exec_ref.target_table,
                "batch_execution": true
            }),
            None, // TODO: Extract from batch job context
        );

        execution_store.save(execution.clone()).await?;

        // Prepare input data for workflow
        let mut input_data =
            serde_json::to_value(&exec_ref.source).unwrap_or_else(|_| serde_json::json!({}));

        // Select matching route (use first route for batch jobs)
        // TODO: Support condition-based routing in batch jobs
        let route = workflow
            .routes
            .first()
            .ok_or_else(|| anyhow!("Workflow has no routes"))?;

        info!(
            "Executing route '{}' for {}",
            route.name,
            exec_ref.display_name()
        );

        // Create execution context with production components
        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route.id.clone(),
            input_data: input_data.clone(),
            rule_executor,
            transformer_registry,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            manual_mapping_store: None,
            execution_id: None,
            action_index: 0,
            metrics: None,
            approval_store: None,
            execution_store: None,
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        // Execute actions
        let results =
            ActionExecutor::execute_actions(&route.actions, &mut input_data, &context).await?;

        // Check if workflow paused (Phase 2.3: Checkpoint logic)
        let paused_action = results
            .iter()
            .find(|r| r.status == crate::workflows::domain::ActionStatus::Paused);

        if let Some(paused_result) = paused_action {
            // Workflow paused - save checkpoint and update status
            info!(
                "Workflow paused at action {} ({}). Saving checkpoint...",
                results.len(),
                paused_result.action_type
            );

            let mut execution = execution_store
                .get(&exec_ref.execution_id)
                .await?
                .ok_or_else(|| anyhow!("Execution record not found"))?;

            // Save checkpoint: action index where pause occurred + intermediate data
            let checkpoint_action_index = results.len() - 1; // Last executed action (the one that paused)
            let checkpoint_data = serde_json::json!({
                "paused_action_type": paused_result.action_type,
                "paused_action_output": paused_result.output,
                "intermediate_data": input_data,
                "total_actions": route.actions.len(),
                "executed_actions": results.len(),
            });

            execution.checkpoint(checkpoint_action_index, checkpoint_data);
            execution.update_status(ExecutionStatus::Paused);

            // Store action results for timing analysis (up to the pause point)
            execution.action_results = results.clone();

            execution_store.update(execution).await?;

            info!(
                "Workflow execution paused: {} (checkpoint saved at action {})",
                exec_ref.execution_id, checkpoint_action_index
            );

            return Ok(()); // Successfully paused
        }

        // Check for action failures (non-paused failures)
        let failed_actions: Vec<&ActionResult> = results
            .iter()
            .filter(|r| r.status == crate::workflows::domain::ActionStatus::Failed)
            .collect();

        if !failed_actions.is_empty() {
            return Err(anyhow!(
                "Workflow execution failed: {} actions failed",
                failed_actions.len()
            ));
        }

        // Update execution status to completed (no pause, no failures)
        let mut execution = execution_store
            .get(&exec_ref.execution_id)
            .await?
            .ok_or_else(|| anyhow!("Execution record not found"))?;

        execution.update_status(ExecutionStatus::Completed);

        // Store action results for timing analysis
        execution.action_results = results;

        execution_store.update(execution).await?;

        Ok(())
    }

    /// Execute data loading workflow (optimized path for ETL)
    ///
    /// This method provides a direct path from source to database without
    /// going through the generic workflow engine. It's used when the target
    /// is a database and we want maximum performance.
    async fn execute_data_loading(
        exec_ref: &WorkflowExecutionRef,
        execution_store: &ExecutionStore,
    ) -> Result<()> {
        info!("Executing data loading: {}", exec_ref.display_name());

        // Create execution record
        let execution = WorkflowExecution::new(
            exec_ref.execution_id.clone(),
            "data_loading".to_string(), // Special workflow ID for direct loading
            "Direct Data Loading".to_string(),
            serde_json::json!({
                "source": exec_ref.source,
                "target_table": exec_ref.target_table,
                "batch_execution": true,
                "direct_load": true
            }),
            None,
        );

        execution_store.save(execution.clone()).await?;

        // Determine target database from metadata or use default
        // For now, check if source is a DatabaseQuery - if so, extract connection info
        let (db_type, db_config, load_mode) = match &exec_ref.source {
            DataSource::DatabaseQuery {
                database_type,
                connection_config,
                ..
            } => {
                // Source is a database query - use same database as target
                (*database_type, connection_config.clone(), LoadMode::Insert)
            }
            DataSource::CsvFile { .. } | DataSource::S3Object { .. } => {
                // TODO: In production, this would come from workflow metadata or config
                // For now, return error indicating database target must be specified
                return Err(anyhow!(
                    "Direct data loading from CSV/S3 requires target database configuration in workflow metadata"
                ));
            }
        };

        // Create load configuration
        let load_config = LoadConfig {
            table_name: exec_ref.target_table.clone(),
            load_mode,
            key_fields: None, // TODO: Extract from workflow metadata
            batch_size: 10000,
            max_errors: Some(100),
        };

        // Create data loader
        let data_loader = DataLoader::new(db_type, db_config, load_config);

        // Create data source reader
        let reader = create_reader(exec_ref.source.clone())
            .context("Failed to create data source reader")?;

        // Perform the load
        let stats = data_loader
            .load_from_reader(reader)
            .await
            .context("Data loading failed")?;

        info!(
            "Data load complete: {} rows loaded, {} failed, duration={}ms",
            stats.rows_loaded, stats.rows_failed, stats.duration_ms
        );

        // Update execution status
        let mut execution = execution_store
            .get(&exec_ref.execution_id)
            .await?
            .ok_or_else(|| anyhow!("Execution record not found"))?;

        execution.status = ExecutionStatus::Completed;
        execution.completed_at = Some(Utc::now());
        execution_store.save(execution).await?;

        Ok(())
    }

    /// Resolve workflow dependencies into execution waves (topological sort)
    ///
    /// Returns a Vec of waves, where each wave contains workflows that can run in parallel.
    /// Workflows in wave N depend only on workflows in waves 0..N-1.
    fn resolve_dependencies(
        &self,
        executions: &[WorkflowExecutionRef],
    ) -> Result<Vec<Vec<WorkflowExecutionRef>>> {
        let mut waves: Vec<Vec<WorkflowExecutionRef>> = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();
        let mut remaining: Vec<WorkflowExecutionRef> = executions.to_vec();

        // Build execution ID index for fast lookup
        let exec_by_id: HashMap<String, WorkflowExecutionRef> = executions
            .iter()
            .map(|e| (e.execution_id.clone(), e.clone()))
            .collect();

        // Validate all dependencies exist
        for exec in executions {
            for dep_id in &exec.dependencies {
                if !exec_by_id.contains_key(dep_id) {
                    return Err(anyhow!(
                        "Invalid dependency: {} references non-existent execution {}",
                        exec.execution_id,
                        dep_id
                    ));
                }
            }
        }

        // Detect circular dependencies
        self.detect_circular_dependencies(&exec_by_id)?;

        // Build waves
        let max_iterations = executions.len() + 1;
        let mut iterations = 0;

        while !remaining.is_empty() {
            iterations += 1;
            if iterations > max_iterations {
                return Err(anyhow!("Circular dependency detected in batch job"));
            }

            // Find all workflows whose dependencies are satisfied
            let mut current_wave = Vec::new();
            let mut still_waiting = Vec::new();

            for exec in remaining {
                if exec.dependencies_satisfied(&completed) {
                    current_wave.push(exec.clone());
                } else {
                    still_waiting.push(exec);
                }
            }

            if current_wave.is_empty() && !still_waiting.is_empty() {
                return Err(anyhow!(
                    "Deadlock detected: {} workflows have unsatisfied dependencies",
                    still_waiting.len()
                ));
            }

            if !current_wave.is_empty() {
                // Mark all executions in current wave as completed
                for exec in &current_wave {
                    completed.insert(exec.execution_id.clone());
                }
                waves.push(current_wave);
            }

            remaining = still_waiting;
        }

        Ok(waves)
    }

    /// Detect circular dependencies using DFS
    fn detect_circular_dependencies(
        &self,
        exec_by_id: &HashMap<String, WorkflowExecutionRef>,
    ) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for exec_id in exec_by_id.keys() {
            if !visited.contains(exec_id) {
                if self.has_cycle(exec_id, exec_by_id, &mut visited, &mut rec_stack) {
                    return Err(anyhow!(
                        "Circular dependency detected involving {}",
                        exec_id
                    ));
                }
            }
        }

        Ok(())
    }

    /// DFS helper for cycle detection
    fn has_cycle(
        &self,
        exec_id: &str,
        exec_by_id: &HashMap<String, WorkflowExecutionRef>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(exec_id.to_string());
        rec_stack.insert(exec_id.to_string());

        if let Some(exec) = exec_by_id.get(exec_id) {
            for dep_id in &exec.dependencies {
                if !visited.contains(dep_id) {
                    if self.has_cycle(dep_id, exec_by_id, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep_id) {
                    return true;
                }
            }
        }

        rec_stack.remove(exec_id);
        false
    }

    /// Update batch job progress from current execution states
    async fn update_batch_progress(&self, batch_job: &mut BatchJob) -> Result<()> {
        // Reload execution refs from storage to get latest status
        // For now, recalculate from in-memory state
        batch_job.recalculate_progress();
        self.batch_store.update(batch_job.clone())?;

        debug!(
            "Batch job progress: {:.1}% ({}/{})",
            batch_job.progress.progress_percent,
            batch_job.progress.completed + batch_job.progress.failed,
            batch_job.progress.total_files
        );

        Ok(())
    }

    /// Cancel a running batch job
    pub async fn cancel(&self, job_id: String) -> Result<()> {
        info!("Cancelling batch job: {}", job_id);

        let mut batch_job = self
            .batch_store
            .get(&job_id)?
            .ok_or_else(|| anyhow!("Batch job not found: {}", job_id))?;

        if !batch_job.can_cancel() {
            return Err(anyhow!(
                "Batch job cannot be cancelled (status: {:?})",
                batch_job.status
            ));
        }

        batch_job.update_status(BatchJobStatus::Cancelled);
        self.batch_store.update(batch_job)?;

        info!("Batch job {} cancelled", job_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{BatchJobConfig, Route, Workflow};
    use rocksdb::DB;
    use tempfile::TempDir;

    fn create_test_stores() -> (Arc<BatchJobStore>, Arc<WorkflowStore>, Arc<ExecutionStore>) {
        let temp_dir = TempDir::new().unwrap();
        let batch_store_path = temp_dir.path().join("batch_jobs");

        let batch_store = Arc::new(BatchJobStore::open(&batch_store_path).unwrap());
        let workflow_store = Arc::new(WorkflowStore::new());
        let execution_store = Arc::new(ExecutionStore::new());

        (batch_store, workflow_store, execution_store)
    }

    #[tokio::test]
    async fn test_dependency_resolution_no_deps() {
        use crate::api::file_library::storage::FileLibraryStorage;
        let (batch_store, workflow_store, execution_store) = create_test_stores();
        let file_store = Arc::new(FileLibraryStorage::new());
        let executor =
            BatchJobExecutor::new(batch_store, workflow_store, execution_store, file_store);

        let executions = vec![
            WorkflowExecutionRef::new(
                DataSource::CsvFile {
                    file_id: "file_1".to_string(),
                    file_path: PathBuf::from("data1.csv"),
                    encoding: None,
                    delimiter: None,
                    has_header: true,
                },
                "table1".to_string(),
            ),
            WorkflowExecutionRef::new(
                DataSource::CsvFile {
                    file_id: "file_2".to_string(),
                    file_path: PathBuf::from("data2.csv"),
                    encoding: None,
                    delimiter: None,
                    has_header: true,
                },
                "table2".to_string(),
            ),
            WorkflowExecutionRef::new(
                DataSource::CsvFile {
                    file_id: "file_3".to_string(),
                    file_path: PathBuf::from("data3.csv"),
                    encoding: None,
                    delimiter: None,
                    has_header: true,
                },
                "table3".to_string(),
            ),
        ];

        let waves = executor.resolve_dependencies(&executions).unwrap();

        // All should be in one wave (no dependencies)
        assert_eq!(waves.len(), 1);
        assert_eq!(waves[0].len(), 3);
    }

    #[tokio::test]
    async fn test_dependency_resolution_linear_chain() {
        use crate::api::file_library::storage::FileLibraryStorage;
        let (batch_store, workflow_store, execution_store) = create_test_stores();
        let file_store = Arc::new(FileLibraryStorage::new());
        let executor =
            BatchJobExecutor::new(batch_store, workflow_store, execution_store, file_store);

        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("data1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table1".to_string(),
        );
        let exec2 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_2".to_string(),
                file_path: PathBuf::from("data2.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table2".to_string(),
        )
        .with_dependency(exec1.execution_id.clone());
        let exec3 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_3".to_string(),
                file_path: PathBuf::from("data3.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table3".to_string(),
        )
        .with_dependency(exec2.execution_id.clone());

        let executions = vec![exec1, exec2, exec3];
        let waves = executor.resolve_dependencies(&executions).unwrap();

        // Should be 3 waves (linear dependency chain)
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0].len(), 1);
        assert_eq!(waves[1].len(), 1);
        assert_eq!(waves[2].len(), 1);
    }

    #[tokio::test]
    async fn test_dependency_resolution_diamond() {
        use crate::api::file_library::storage::FileLibraryStorage;
        let (batch_store, workflow_store, execution_store) = create_test_stores();
        let file_store = Arc::new(FileLibraryStorage::new());
        let executor =
            BatchJobExecutor::new(batch_store, workflow_store, execution_store, file_store);

        // Diamond dependency: 1 → (2, 3) → 4
        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("base.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "base_table".to_string(),
        );
        let exec2 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_2".to_string(),
                file_path: PathBuf::from("derived1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "derived1_table".to_string(),
        )
        .with_dependency(exec1.execution_id.clone());
        let exec3 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_3".to_string(),
                file_path: PathBuf::from("derived2.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "derived2_table".to_string(),
        )
        .with_dependency(exec1.execution_id.clone());
        let exec4 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_4".to_string(),
                file_path: PathBuf::from("final.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "final_table".to_string(),
        )
        .with_dependency(exec2.execution_id.clone())
        .with_dependency(exec3.execution_id.clone());

        let executions = vec![exec1, exec2, exec3, exec4];
        let waves = executor.resolve_dependencies(&executions).unwrap();

        // Should be 3 waves
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0].len(), 1); // exec1
        assert_eq!(waves[1].len(), 2); // exec2, exec3 (parallel)
        assert_eq!(waves[2].len(), 1); // exec4
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        use crate::api::file_library::storage::FileLibraryStorage;
        let (batch_store, workflow_store, execution_store) = create_test_stores();
        let file_store = Arc::new(FileLibraryStorage::new());
        let executor =
            BatchJobExecutor::new(batch_store, workflow_store, execution_store, file_store);

        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("data1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table1".to_string(),
        );
        let exec2 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_2".to_string(),
                file_path: PathBuf::from("data2.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table2".to_string(),
        )
        .with_dependency(exec1.execution_id.clone());

        // Create circular dependency
        let mut exec1_circular = exec1.clone();
        exec1_circular.dependencies.push(exec2.execution_id.clone());

        let executions = vec![exec1_circular, exec2];
        let result = executor.resolve_dependencies(&executions);

        // Should detect circular dependency
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circular dependency"));
    }

    #[tokio::test]
    async fn test_invalid_dependency_reference() {
        use crate::api::file_library::storage::FileLibraryStorage;
        let (batch_store, workflow_store, execution_store) = create_test_stores();
        let file_store = Arc::new(FileLibraryStorage::new());
        let executor =
            BatchJobExecutor::new(batch_store, workflow_store, execution_store, file_store);

        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("data1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "table1".to_string(),
        )
        .with_dependency("nonexistent_id".into());

        let executions = vec![exec1];
        let result = executor.resolve_dependencies(&executions);

        // Should detect invalid dependency
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid dependency"));
    }
}
