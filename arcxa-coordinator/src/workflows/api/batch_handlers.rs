//! Batch Job API Handlers
//!
//! HTTP handlers for batch job orchestration and monitoring.

use super::dto::*;
use super::handlers::ApiError;
use crate::api::auth::Claims;
use crate::api::file_library::storage::FileLibraryStorage;
use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::workflows::domain::{BatchJob, WorkflowExecutionRef};
use crate::workflows::engine::BatchJobExecutor;
use crate::workflows::storage::{BatchJobStore, ExecutionStore, WorkflowStore};
use axum::{
    extract::{Path, Query, State},
    http::{Extensions, StatusCode},
    Json,
};
use std::sync::Arc;
use tracing::{error, info};

/// Batch job API state
#[derive(Clone)]
pub struct BatchJobApiState {
    pub batch_store: Arc<BatchJobStore>,
    pub workflow_store: Arc<WorkflowStore>,
    pub execution_store: Arc<ExecutionStore>,
    pub file_store: Arc<dyn FileLibraryStore>,
    pub executor: Arc<BatchJobExecutor>,
}

impl BatchJobApiState {
    pub fn new(
        batch_store: BatchJobStore,
        workflow_store: WorkflowStore,
        execution_store: ExecutionStore,
        file_store: Arc<dyn FileLibraryStore>,
    ) -> Self {
        let batch_store = Arc::new(batch_store);
        let workflow_store = Arc::new(workflow_store);
        let execution_store = Arc::new(execution_store);

        let executor = Arc::new(BatchJobExecutor::new(
            batch_store.clone(),
            workflow_store.clone(),
            execution_store.clone(),
            file_store.clone(),
        ));

        Self {
            batch_store,
            workflow_store,
            execution_store,
            file_store,
            executor,
        }
    }

    /// Create API state with default in-memory file store
    pub fn new_with_default_file_store(
        batch_store: BatchJobStore,
        workflow_store: WorkflowStore,
        execution_store: ExecutionStore,
    ) -> Self {
        Self::new(
            batch_store,
            workflow_store,
            execution_store,
            Arc::new(FileLibraryStorage::new()),
        )
    }
}

// === API Handlers ===

/// Create a new batch job
///
/// POST /api/v1/batch-jobs
pub async fn create_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    extensions: Extensions,
    Json(req): Json<CreateBatchJobRequest>,
) -> Result<Json<CreateBatchJobResponse>, ApiError> {
    info!(
        "Creating batch job: {} ({} files)",
        req.name,
        req.file_ids.len()
    );

    // Extract authenticated user from request extensions
    let created_by = extensions
        .get::<Claims>()
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "system".to_string());

    // Validate workflow exists
    let workflow = state
        .workflow_store
        .get(&req.workflow_id)?
        .ok_or_else(|| ApiError::BadRequest(format!("Workflow not found: {}", req.workflow_id)))?;

    if !workflow.enabled {
        return Err(ApiError::BadRequest(format!(
            "Workflow '{}' is disabled",
            workflow.id
        )));
    }

    // Validate that at least one source is provided (new or old format)
    if req.sources.is_empty() && req.file_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "Batch job must have at least one source or file".to_string(),
        ));
    }

    // Create batch job with config
    let config = req.config.unwrap_or_default();
    let mut batch_job = BatchJob::new(
        req.name.clone(),
        req.workflow_id.clone(),
        config,
        created_by,
    );

    batch_job.description = req.description;
    batch_job.metadata = req.metadata;

    // Add workflow executions - prioritize new format (sources)
    if !req.sources.is_empty() {
        // New format: Use SourceExecutionRef with DataSource
        for source_ref in req.sources {
            let mut exec_ref =
                WorkflowExecutionRef::new(source_ref.source, source_ref.target_table);
            exec_ref.dependencies = source_ref.dependencies;
            batch_job.add_execution(exec_ref);
        }
    } else {
        // Old format: Convert FileRef to DataSource for backward compatibility
        #[allow(deprecated)]
        for file_ref in req.file_ids {
            let target_table = file_ref
                .file_name
                .strip_suffix(".csv")
                .unwrap_or(&file_ref.file_name)
                .to_string();

            #[allow(deprecated)]
            let mut exec_ref =
                WorkflowExecutionRef::from_file(file_ref.file_id, file_ref.file_name, target_table);
            exec_ref.dependencies = file_ref.dependencies;
            batch_job.add_execution(exec_ref);
        }
    }

    // Validate batch job (checks dependencies, etc.)
    batch_job
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Batch job validation failed: {}", e)))?;

    // Store batch job
    state.batch_store.create(batch_job.clone())?;

    info!("Batch job '{}' created successfully", batch_job.job_id);

    Ok(Json(batch_job.into()))
}

/// Get a batch job by ID
///
/// GET /api/v1/batch-jobs/{id}
pub async fn get_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<GetBatchJobResponse>, ApiError> {
    info!("Getting batch job: {}", job_id);

    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    Ok(Json(batch_job.into()))
}

/// List batch jobs
///
/// GET /api/v1/batch-jobs
pub async fn list_batch_jobs(
    State(state): State<Arc<BatchJobApiState>>,
    extensions: Extensions,
    Query(query): Query<ListBatchJobsQuery>,
) -> Result<Json<ListBatchJobsResponse>, ApiError> {
    info!(
        "Listing batch jobs (limit={}, offset={})",
        query.limit, query.offset
    );

    // Extract authenticated user from request extensions
    let user_id = extensions
        .get::<Claims>()
        .map(|claims| claims.sub.as_str())
        .unwrap_or("system");

    let all_jobs = state.batch_store.list_by_user(user_id, 1000, 0)?;

    // Filter by status if provided
    let filtered: Vec<BatchJob> = if let Some(status) = query.status {
        all_jobs
            .into_iter()
            .filter(|j| j.status == status)
            .collect()
    } else {
        all_jobs
    };

    // Filter by workflow_id if provided
    let filtered: Vec<BatchJob> = if let Some(workflow_id) = query.workflow_id {
        filtered
            .into_iter()
            .filter(|j| j.workflow_id == workflow_id)
            .collect()
    } else {
        filtered
    };

    let total = filtered.len();

    // Apply pagination
    let batch_jobs: Vec<BatchJobSummary> = filtered
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(BatchJobSummary::from)
        .collect();

    Ok(Json(ListBatchJobsResponse {
        batch_jobs,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Execute a batch job
///
/// POST /api/v1/batch-jobs/{id}/execute
pub async fn execute_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<ExecuteBatchJobResponse>, ApiError> {
    info!("Executing batch job: {}", job_id);

    // Verify batch job exists
    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    // Check if already running or completed
    if batch_job.status.is_terminal() {
        return Err(ApiError::BadRequest(format!(
            "Batch job already completed (status: {:?})",
            batch_job.status
        )));
    }

    // Spawn executor task in background
    let executor = state.executor.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        match executor.execute(job_id_clone.clone()).await {
            Ok(_) => {
                info!("Batch job {} completed successfully", job_id_clone);
            }
            Err(e) => {
                error!("Batch job {} failed: {}", job_id_clone, e);
            }
        }
    });

    Ok(Json(ExecuteBatchJobResponse {
        job_id: job_id.clone(),
        status: crate::workflows::domain::BatchJobStatus::Running,
        message: format!("Batch job {} execution started", job_id),
    }))
}

/// Cancel a batch job
///
/// POST /api/v1/batch-jobs/{id}/cancel
pub async fn cancel_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<CancelBatchJobResponse>, ApiError> {
    info!("Cancelling batch job: {}", job_id);

    // Cancel the batch job
    state.executor.cancel(job_id.clone()).await?;

    // Reload to get updated status
    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    Ok(Json(CancelBatchJobResponse {
        job_id: job_id.clone(),
        status: batch_job.status,
        message: format!("Batch job {} cancelled", job_id),
    }))
}

/// Validate a batch job before execution (preflight check)
///
/// POST /api/v1/batch-jobs/{id}/validate
pub async fn validate_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<PreflightValidationResponse>, ApiError> {
    info!("Running preflight validation for batch job: {}", job_id);

    // Run preflight validation
    let result = state.executor.preflight_validate(&job_id).await?;

    Ok(Json(PreflightValidationResponse {
        job_id: job_id.clone(),
        is_valid: result.is_valid(),
        errors: result.errors,
        warnings: result.warnings,
        checks: result.checks,
        estimated_duration_minutes: result.estimated_duration_minutes,
        resource_requirements: result.resource_requirements,
    }))
}

/// Delete a batch job
///
/// DELETE /api/v1/batch-jobs/{id}
pub async fn delete_batch_job(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("Deleting batch job: {}", job_id);

    // Verify exists
    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    // Only allow deletion of terminal jobs
    if !batch_job.is_terminal() {
        return Err(ApiError::BadRequest(format!(
            "Cannot delete running batch job (status: {:?})",
            batch_job.status
        )));
    }

    state.batch_store.delete(&job_id)?;

    info!("Batch job '{}' deleted successfully", job_id);

    Ok(StatusCode::NO_CONTENT)
}

/// Get dead letter queue statistics for a batch job
///
/// GET /api/v1/batch-jobs/{id}/dlq
pub async fn get_batch_job_dlq(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<DlqInfoResponse>, ApiError> {
    info!("Getting DLQ information for batch job: {}", job_id);

    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    Ok(Json(DlqInfoResponse {
        job_id: job_id.clone(),
        dlq_enabled: batch_job.config.enable_dlq,
        dlq_files: batch_job.dlq_files.clone(),
        total_failed_rows: batch_job.dlq_row_count,
        total_failed_workflows: batch_job.progress.failed,
    }))
}

/// Get transaction information for a batch job
///
/// GET /api/v1/batch-jobs/{id}/transactions
pub async fn get_batch_job_transactions(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<TransactionInfoResponse>, ApiError> {
    info!("Getting transaction information for batch job: {}", job_id);

    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    Ok(Json(TransactionInfoResponse {
        job_id: job_id.clone(),
        transaction_mode: batch_job.config.transaction_mode,
        transaction_summary: batch_job.transaction_summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::BatchJobConfig;
    use rocksdb::DB;
    use tempfile::TempDir;

    fn create_test_state() -> Arc<BatchJobApiState> {
        let temp_dir = TempDir::new().unwrap();
        let batch_store_path = temp_dir.path().join("batch_jobs");

        let batch_store = BatchJobStore::open(&batch_store_path).unwrap();
        let workflow_store = WorkflowStore::new();
        let execution_store = ExecutionStore::new();

        Arc::new(BatchJobApiState::new_with_default_file_store(
            batch_store,
            workflow_store,
            execution_store,
        ))
    }

    #[tokio::test]
    async fn test_batch_job_api_state_creation() {
        let state = create_test_state();
        assert!(Arc::strong_count(&state.batch_store) >= 2); // Referenced by state and executor
    }
}
