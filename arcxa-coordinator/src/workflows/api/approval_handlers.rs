//! Approval Request API Handlers
//!
//! REST API endpoints for managing workflow approval requests.
//! Supports listing, viewing, approving, and rejecting approval requests.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::api::ApiState;
use crate::workflows::domain::{ApprovalRequest, ApprovalStatus};
use crate::workflows::storage::ApprovalStore;

/// Query parameters for listing approval requests
#[derive(Debug, Deserialize)]
pub struct ListApprovalsQuery {
    /// Filter by approval status (pending, approved, rejected, expired, cancelled)
    pub status: Option<String>,

    /// Filter by workflow ID
    pub workflow_id: Option<String>,

    /// Filter by execution ID
    pub execution_id: Option<String>,

    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

/// Request body for approving an approval request
#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    /// User ID of the approver
    pub approved_by: String,
}

/// Request body for rejecting an approval request
#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    /// User ID of the rejector
    pub rejected_by: String,

    /// Reason for rejection
    pub reason: String,
}

/// Response for approval operations
#[derive(Debug, Serialize)]
pub struct ApprovalResponse {
    pub request: ApprovalRequest,
}

/// Response for list operations
#[derive(Debug, Serialize)]
pub struct ListApprovalsResponse {
    pub requests: Vec<ApprovalRequest>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// API error type
#[derive(Debug)]
pub enum ApprovalApiError {
    NotFound(String),
    InvalidStatus(String),
    StorageError(String),
    ValidationError(String),
}

impl IntoResponse for ApprovalApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApprovalApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApprovalApiError::InvalidStatus(msg) => (StatusCode::BAD_REQUEST, msg),
            ApprovalApiError::StorageError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApprovalApiError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = serde_json::json!({
            "error": message,
        });

        (status, Json(body)).into_response()
    }
}

impl From<anyhow::Error> for ApprovalApiError {
    fn from(err: anyhow::Error) -> Self {
        ApprovalApiError::StorageError(err.to_string())
    }
}

/// List approval requests
///
/// GET /approvals
///
/// Query parameters:
/// - status: Filter by status (pending, approved, rejected, expired, cancelled)
/// - workflow_id: Filter by workflow ID
/// - execution_id: Filter by execution ID
/// - limit: Maximum results (default: 50)
/// - offset: Pagination offset (default: 0)
pub async fn list_approvals(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListApprovalsQuery>,
) -> Result<Json<ListApprovalsResponse>, ApprovalApiError> {
    info!(
        "Listing approval requests: status={:?}, workflow_id={:?}, execution_id={:?}, limit={}, offset={}",
        query.status, query.workflow_id, query.execution_id, query.limit, query.offset
    );

    // Parse status filter if provided
    let status_filter = if let Some(ref status_str) = query.status {
        Some(parse_approval_status(status_str)?)
    } else {
        None
    };

    // Fetch requests based on filters
    let requests = if let Some(execution_id) = query.execution_id {
        // Filter by execution ID
        state
            .approval_store
            .as_ref()
            .expect("approval_store not initialized")
            .list_by_execution(&execution_id)
            .await?
    } else if let Some(workflow_id) = query.workflow_id {
        // Filter by workflow ID
        state
            .approval_store
            .as_ref()
            .expect("approval_store not initialized")
            .list_by_workflow(&workflow_id, query.limit, query.offset)
            .await?
    } else if let Some(status) = status_filter {
        // Filter by status
        state
            .approval_store
            .as_ref()
            .expect("approval_store not initialized")
            .list_by_status(status, query.limit, query.offset)
            .await?
    } else {
        // No filters - list pending by default
        state
            .approval_store
            .as_ref()
            .expect("approval_store not initialized")
            .list_pending(query.limit)
            .await?
    };

    let total = requests.len();

    Ok(Json(ListApprovalsResponse {
        requests,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Get a specific approval request by ID
///
/// GET /approvals/:request_id
pub async fn get_approval(
    State(state): State<Arc<ApiState>>,
    Path(request_id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApprovalApiError> {
    info!("Getting approval request: {}", request_id);

    let request = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .get(&request_id)
        .await?
        .ok_or_else(|| {
            ApprovalApiError::NotFound(format!("Approval request not found: {}", request_id))
        })?;

    Ok(Json(ApprovalResponse { request }))
}

/// Approve an approval request
///
/// POST /approvals/:request_id/approve
///
/// Body: { "approved_by": "user@example.com" }
pub async fn approve_approval(
    State(state): State<Arc<ApiState>>,
    Path(request_id): Path<String>,
    Json(body): Json<ApproveRequest>,
) -> Result<Json<ApprovalResponse>, ApprovalApiError> {
    info!(
        "Approving request: {} by user: {}",
        request_id, body.approved_by
    );

    // Validate approved_by is not empty
    if body.approved_by.trim().is_empty() {
        return Err(ApprovalApiError::ValidationError(
            "approved_by cannot be empty".to_string(),
        ));
    }

    // Approve the request (this handles state validation internally)
    state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .approve(&request_id, body.approved_by)
        .await
        .map_err(|e| {
            warn!("Failed to approve request {}: {}", request_id, e);
            ApprovalApiError::InvalidStatus(e.to_string())
        })?;

    // Fetch updated request
    let request = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .get_required(&request_id)
        .await?;

    info!("Successfully approved request: {}", request_id);

    // Trigger workflow resume if execution_id is present
    if !request.execution_id.is_empty() {
        let execution_id = request.execution_id.clone();
        info!("Triggering workflow resume for execution: {}", execution_id);

        // Resume execution asynchronously (don't block the API response)
        let workflow_store = state.workflow_store.clone();
        let execution_store = state.execution_store.clone();
        let file_library = state.file_library.clone();
        let rule_executor = state.rule_executor.clone();

        tokio::spawn(async move {
            match resume_workflow_execution(
                execution_id.clone(),
                workflow_store,
                execution_store,
                file_library,
                rule_executor,
            )
            .await
            {
                Ok(_) => info!("Workflow execution resumed successfully: {}", execution_id),
                Err(e) => error!(
                    "Failed to resume workflow execution {}: {}",
                    execution_id, e
                ),
            }
        });
    }

    Ok(Json(ApprovalResponse { request }))
}

/// Reject an approval request
///
/// POST /approvals/:request_id/reject
///
/// Body: { "rejected_by": "user@example.com", "reason": "Does not meet criteria" }
pub async fn reject_approval(
    State(state): State<Arc<ApiState>>,
    Path(request_id): Path<String>,
    Json(body): Json<RejectRequest>,
) -> Result<Json<ApprovalResponse>, ApprovalApiError> {
    info!(
        "Rejecting request: {} by user: {} with reason: {}",
        request_id, body.rejected_by, body.reason
    );

    // Validate inputs
    if body.rejected_by.trim().is_empty() {
        return Err(ApprovalApiError::ValidationError(
            "rejected_by cannot be empty".to_string(),
        ));
    }

    if body.reason.trim().is_empty() {
        return Err(ApprovalApiError::ValidationError(
            "reason cannot be empty".to_string(),
        ));
    }

    // Reject the request (this handles state validation internally)
    state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .reject(&request_id, body.rejected_by, Some(body.reason))
        .await
        .map_err(|e| {
            warn!("Failed to reject request {}: {}", request_id, e);
            ApprovalApiError::InvalidStatus(e.to_string())
        })?;

    // Fetch updated request
    let request = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .get_required(&request_id)
        .await?;

    info!("Successfully rejected request: {}", request_id);

    Ok(Json(ApprovalResponse { request }))
}

/// Cancel an approval request
///
/// POST /approvals/:request_id/cancel
pub async fn cancel_approval(
    State(state): State<Arc<ApiState>>,
    Path(request_id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApprovalApiError> {
    info!("Cancelling request: {}", request_id);

    // Cancel the request
    state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .cancel(&request_id)
        .await
        .map_err(|e| {
            warn!("Failed to cancel request {}: {}", request_id, e);
            ApprovalApiError::InvalidStatus(e.to_string())
        })?;

    // Fetch updated request
    let request = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .get_required(&request_id)
        .await?;

    info!("Successfully cancelled request: {}", request_id);

    Ok(Json(ApprovalResponse { request }))
}

/// Get statistics about approval requests
///
/// GET /approvals/stats
#[derive(Debug, Serialize)]
pub struct ApprovalStatsResponse {
    pub total: usize,
    pub pending: usize,
    pub approved: usize,
    pub rejected: usize,
    pub expired: usize,
    pub cancelled: usize,
}

pub async fn get_approval_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ApprovalStatsResponse>, ApprovalApiError> {
    info!("Getting approval statistics");

    let total = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_total()
        .await?;
    let pending = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_by_status(ApprovalStatus::Pending)
        .await?;
    let approved = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_by_status(ApprovalStatus::Approved)
        .await?;
    let rejected = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_by_status(ApprovalStatus::Rejected)
        .await?;
    let expired = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_by_status(ApprovalStatus::Expired)
        .await?;
    let cancelled = state
        .approval_store
        .as_ref()
        .expect("approval_store not initialized")
        .count_by_status(ApprovalStatus::Cancelled)
        .await?;

    Ok(Json(ApprovalStatsResponse {
        total,
        pending,
        approved,
        rejected,
        expired,
        cancelled,
    }))
}

/// Helper: Parse approval status from string
fn parse_approval_status(status: &str) -> Result<ApprovalStatus, ApprovalApiError> {
    match status.to_lowercase().as_str() {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "rejected" => Ok(ApprovalStatus::Rejected),
        "expired" => Ok(ApprovalStatus::Expired),
        "cancelled" => Ok(ApprovalStatus::Cancelled),
        _ => Err(ApprovalApiError::ValidationError(format!(
            "Invalid status: '{}'. Must be one of: pending, approved, rejected, expired, cancelled",
            status
        ))),
    }
}

/// Helper: Resume a paused workflow execution
///
/// This function is called asynchronously after an approval is granted.
/// It mirrors the logic in BatchJobExecutor::resume_execution but works
/// standalone without requiring a batch job context.
async fn resume_workflow_execution(
    execution_id: String,
    workflow_store: Option<Arc<crate::workflows::storage::WorkflowStore>>,
    execution_store: Option<Arc<crate::workflows::storage::ExecutionStore>>,
    file_library: Option<Arc<dyn crate::api::file_library::storage_trait::FileLibraryStore>>,
    rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
) -> anyhow::Result<()> {
    use crate::workflows::domain::{ActionStatus, ExecutionStatus};
    use crate::workflows::engine::{ActionExecutor, ExecutionContext};
    use anyhow::{anyhow, Context};

    let workflow_store = workflow_store.ok_or_else(|| anyhow!("workflow_store not initialized"))?;
    let execution_store =
        execution_store.ok_or_else(|| anyhow!("execution_store not initialized"))?;

    info!("Resuming workflow execution: {}", execution_id);

    // Load execution record
    let mut execution = execution_store
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
    let workflow = workflow_store
        .get(&execution.workflow_id)?
        .ok_or_else(|| anyhow!("Workflow not found: {}", execution.workflow_id))?;

    // Get the route (using first route for now)
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
        execution_store.update(execution).await?;
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
        rule_executor,
        transformer_registry: None,
        kafka_producer: None,
        http_client: None,
        lineage_generator: None,
        manual_mapping_store: None,
        execution_id: Some(execution_id.clone()),
        action_index: resume_from_index,
        metrics: None,
        approval_store: None,
        execution_store: Some(execution_store.clone()),
        column_lineage_store: None,
        tenant_id: "default".to_string(),
        timeout_config: graphica_core::orchestration::workflow::ExecutionTimeout::default(),
        workflow_start_time: std::time::Instant::now(),
        stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        db2_pool: None,
        postgres_pool: None,
        memory_monitor: None,
    };

    // Update execution status to Running
    execution.update_status(ExecutionStatus::Running);
    execution.clear_checkpoint();
    execution_store.update(execution.clone()).await?;

    // Execute remaining actions
    let results =
        ActionExecutor::execute_actions(remaining_actions, &mut intermediate_data, &context)
            .await?;

    // Check if workflow paused again
    let paused_action = results.iter().find(|r| r.status == ActionStatus::Paused);

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

        let mut execution = execution_store
            .get(&execution_id)
            .await?
            .ok_or_else(|| anyhow!("Execution not found during checkpoint save"))?;

        execution.checkpoint(new_checkpoint_index, checkpoint_data);
        execution.update_status(ExecutionStatus::Paused);
        execution_store.update(execution).await?;

        return Ok(());
    }

    // Check for failures
    let failed_actions: Vec<_> = results
        .iter()
        .filter(|r| r.status == ActionStatus::Failed)
        .collect();

    if !failed_actions.is_empty() {
        execution.update_status(ExecutionStatus::Failed);
        execution_store.update(execution).await?;
        return Err(anyhow!(
            "Workflow execution failed after resume: {} actions failed",
            failed_actions.len()
        ));
    }

    // All actions completed successfully
    execution.update_status(ExecutionStatus::Completed);
    execution_store.update(execution).await?;

    info!("Workflow execution resumed and completed: {}", execution_id);

    Ok(())
}
