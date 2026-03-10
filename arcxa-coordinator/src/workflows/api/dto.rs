//! API Data Transfer Objects (DTOs)
//!
//! Request and response types for the workflow REST API.

use crate::workflows::domain::{
    Action, ActionResult, BatchJob, BatchJobConfig, BatchJobProgress, BatchJobStatus, Condition,
    DataSource, ExecutionLog, ExecutionStatus, LogLevel, Route, TransactionMode, Workflow,
    WorkflowExecution, WorkflowExecutionRef, WorkflowSummary,
};
use crate::workflows::engine::{
    ResourceRequirements, ValidationCheck, ValidationError, ValidationWarning,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// === Create Workflow ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub routes: Vec<RouteDto>,
    #[serde(default)]
    pub default_route: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDto {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub condition: Box<Condition>,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflowResponse {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub created_at: DateTime<Utc>,
}

// === Update Workflow ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkflowRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub routes: Option<Vec<RouteDto>>,
    #[serde(default)]
    pub default_route: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

// === Execute Workflow ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteWorkflowRequest {
    /// Input data for workflow execution
    ///
    /// Supports both simple JSON and graph-native inputs (SPARQL, entity filters).
    /// For backward compatibility, plain JSON values are automatically wrapped.
    #[serde(default)]
    pub input: WorkflowInputWrapper,

    /// Optional execution context for tracing and ML tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ExecutionContextParams>,

    /// Enable dry-run mode (validate but don't execute)
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteWorkflowResponse {
    pub workflow_id: String,
    pub workflow_name: String,
    pub matched_route: Option<String>,
    pub actions_executed: Vec<ActionResult>,
    pub output: JsonValue,
    pub total_duration_ms: u64,
    pub evaluation_time_ms: u64,
    pub execution_time_ms: u64,

    /// ML feature values extracted or computed during execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ml_feature_values: Option<Vec<MlFeatureValue>>,

    /// Lineage references for data provenance tracking
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage_refs: Vec<LineageReference>,
}

// === List Workflows ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkflowsQuery {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub tags: Option<String>, // Comma-separated
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkflowsResponse {
    pub workflows: Vec<WorkflowSummary>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

// === Get Workflow ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetWorkflowResponse {
    pub workflow: Workflow,
}

// === Get Route Stats ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRouteStatsRequest {
    pub sample_data: Vec<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRouteStatsResponse {
    pub workflow_id: String,
    pub total_samples: usize,
    pub route_matches: std::collections::HashMap<String, RouteMatchStats>,
    pub no_match_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteMatchStats {
    pub route_id: String,
    pub route_name: String,
    pub match_count: usize,
    pub match_percentage: f64,
}

// === Execution Tracking ===

/// Execute workflow response with async execution tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteWorkflowAsyncResponse {
    pub execution_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub started_at: DateTime<Utc>,
}

/// Get execution details response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetExecutionResponse {
    pub execution_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub input: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub step_results: Vec<crate::workflows::domain::PersistedStepResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_route_name: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    pub actions_executed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_by: Option<String>,
    /// Progress percentage (0.0 - 100.0) calculated from actions_executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f64>,
    /// Total number of actions in the matched route
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_actions: Option<usize>,
    /// Next actions that will be executed (IDs)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub next_actions: Vec<String>,
    /// Per-action timing breakdown
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub action_results: Vec<crate::workflows::domain::ActionResult>,
}

impl From<WorkflowExecution> for GetExecutionResponse {
    fn from(exec: WorkflowExecution) -> Self {
        Self {
            execution_id: exec.execution_id,
            workflow_id: exec.workflow_id,
            workflow_name: exec.workflow_name,
            status: exec.status,
            input: exec.input,
            confidence: exec.confidence,
            step_results: exec.step_results,
            output: exec.output,
            matched_route: exec.matched_route,
            matched_route_name: exec.matched_route_name,
            started_at: exec.started_at,
            updated_at: exec.updated_at,
            completed_at: exec.completed_at,
            duration_ms: exec.duration_ms,
            error: exec.error,
            current_step: exec.current_step,
            actions_executed: exec.actions_executed,
            triggered_by: exec.triggered_by,
            progress_percent: None, // Will be calculated in handler if workflow info available
            total_actions: None,    // Will be set in handler if workflow info available
            next_actions: Vec::new(), // Will be set in handler if workflow info available
            action_results: exec.action_results,
        }
    }
}

/// Get execution logs response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetExecutionLogsResponse {
    pub execution_id: String,
    pub logs: Vec<ExecutionLogDto>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLogDto {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
}

impl From<ExecutionLog> for ExecutionLogDto {
    fn from(log: ExecutionLog) -> Self {
        Self {
            timestamp: log.timestamp,
            level: log.level,
            step_id: log.step_id,
            message: log.message,
            details: log.details,
        }
    }
}

/// List executions query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListExecutionsQuery {
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

/// List executions response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListExecutionsResponse {
    pub executions: Vec<ExecutionSummary>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub actions_executed: usize,
}

impl From<WorkflowExecution> for ExecutionSummary {
    fn from(exec: WorkflowExecution) -> Self {
        Self {
            execution_id: exec.execution_id,
            workflow_id: exec.workflow_id,
            workflow_name: exec.workflow_name,
            status: exec.status,
            started_at: exec.started_at,
            updated_at: exec.updated_at,
            completed_at: exec.completed_at,
            duration_ms: exec.duration_ms,
            error: exec.error,
            actions_executed: exec.actions_executed,
        }
    }
}

/// Stop/Pause/Resume/Abort execution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLifecycleResponse {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub message: String,
}

/// Update schedule request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScheduleRequest {
    pub cron_expression: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Update schedule response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScheduleResponse {
    pub workflow_id: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub timezone: String,
    pub next_run: Option<DateTime<Utc>>,
}

/// Schedule preview request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulePreviewRequest {
    pub cron_expression: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default = "default_preview_count")]
    pub count: usize,
}

fn default_preview_count() -> usize {
    10
}

/// Schedule preview response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulePreviewResponse {
    pub cron_expression: String,
    pub timezone: String,
    pub next_runs: Vec<DateTime<Utc>>,
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
}

// === Error Response ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: JsonValue) -> Self {
        self.details = Some(details);
        self
    }
}

// === Batch Job Operations ===

/// Create batch job request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBatchJobRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub workflow_id: String,
    /// Data source executions (new format)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceExecutionRef>,
    /// File references (deprecated - use sources instead)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[deprecated(since = "0.2.0", note = "Use sources field instead")]
    pub file_ids: Vec<FileRef>,
    #[serde(default)]
    pub config: Option<BatchJobConfig>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Data source execution reference (new format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceExecutionRef {
    /// Data source to read from
    pub source: DataSource,
    /// Target table/entity to load into
    pub target_table: String,
    /// Execution dependencies (IDs of other executions that must complete first)
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// File reference for batch job (deprecated - use SourceExecutionRef)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.2.0", note = "Use SourceExecutionRef instead")]
pub struct FileRef {
    pub file_id: String,
    pub file_name: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Create batch job response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBatchJobResponse {
    pub job_id: String,
    pub name: String,
    pub workflow_id: String,
    pub total_files: usize,
    pub status: BatchJobStatus,
    pub created_at: DateTime<Utc>,
}

impl From<BatchJob> for CreateBatchJobResponse {
    fn from(batch: BatchJob) -> Self {
        Self {
            job_id: batch.job_id,
            name: batch.name,
            workflow_id: batch.workflow_id,
            total_files: batch.workflow_executions.len(),
            status: batch.status,
            created_at: batch.created_at,
        }
    }
}

/// Get batch job response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBatchJobResponse {
    pub job_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub workflow_id: String,
    pub status: BatchJobStatus,
    pub progress: BatchJobProgress,
    pub config: BatchJobConfig,
    pub workflow_executions: Vec<WorkflowExecutionRef>,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub created_by: String,
}

impl From<BatchJob> for GetBatchJobResponse {
    fn from(batch: BatchJob) -> Self {
        let duration_ms = batch.duration_ms();

        Self {
            job_id: batch.job_id,
            name: batch.name,
            description: batch.description,
            workflow_id: batch.workflow_id,
            status: batch.status,
            progress: batch.progress,
            config: batch.config,
            workflow_executions: batch.workflow_executions,
            metadata: batch.metadata,
            created_at: batch.created_at,
            updated_at: batch.updated_at,
            started_at: batch.started_at,
            completed_at: batch.completed_at,
            duration_ms,
            created_by: batch.created_by,
        }
    }
}

/// List batch jobs query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBatchJobsQuery {
    #[serde(default)]
    pub status: Option<BatchJobStatus>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

/// List batch jobs response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBatchJobsResponse {
    pub batch_jobs: Vec<BatchJobSummary>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Batch job summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobSummary {
    pub job_id: String,
    pub name: String,
    pub workflow_id: String,
    pub status: BatchJobStatus,
    pub progress: BatchJobProgress,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub total_files: usize,
    pub created_by: String,
}

impl From<BatchJob> for BatchJobSummary {
    fn from(batch: BatchJob) -> Self {
        Self {
            job_id: batch.job_id,
            name: batch.name,
            workflow_id: batch.workflow_id,
            status: batch.status,
            progress: batch.progress,
            created_at: batch.created_at,
            updated_at: batch.updated_at,
            started_at: batch.started_at,
            completed_at: batch.completed_at,
            total_files: batch.workflow_executions.len(),
            created_by: batch.created_by,
        }
    }
}

/// Execute batch job response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteBatchJobResponse {
    pub job_id: String,
    pub status: BatchJobStatus,
    pub message: String,
}

/// Cancel batch job response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelBatchJobResponse {
    pub job_id: String,
    pub status: BatchJobStatus,
    pub message: String,
}

/// Preflight validation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightValidationResponse {
    pub job_id: String,
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub checks: Vec<ValidationCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration_minutes: Option<usize>,
    pub resource_requirements: ResourceRequirements,
}

/// Dead letter queue information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqInfoResponse {
    pub job_id: String,
    pub dlq_enabled: bool,
    pub dlq_files: Vec<std::path::PathBuf>,
    pub total_failed_rows: u64,
    pub total_failed_workflows: usize,
}

/// Transaction information response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfoResponse {
    pub job_id: String,
    pub transaction_mode: TransactionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_summary: Option<crate::workflows::domain::TransactionSummaryInfo>,
}

// === Graph-Native & ML Support (from Implementation #2) ===

pub use crate::api::workflow::types::{
    ExecutionContextParams, RegisterWorkflowRequest, RegisterWorkflowResponse, StepExecutionResult,
    TestWorkflowStepRequest, TestWorkflowStepResponse, WorkflowSummaryDto,
};

/// Workflow input wrapper supporting both JSON and graph-native inputs
///
/// Provides backward compatibility with JSON-only workflows while
/// enabling graph-native workflows with SPARQL queries and entity filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkflowInputWrapper {
    /// Graph-native input using graphica-core's WorkflowInput
    ///
    /// Supports SPARQL queries, entity filters, and RDF operations.
    /// Requires graphica-core orchestration module.
    GraphNative(graphica_core::orchestration::workflow::WorkflowInput),

    /// Standard JSON input (most common)
    Json(JsonValue),
}

impl WorkflowInputWrapper {
    /// Extract the JSON value, converting from graph-native if needed
    pub fn into_json(self) -> JsonValue {
        match self {
            WorkflowInputWrapper::Json(value) => value,
            WorkflowInputWrapper::GraphNative(input) => {
                // Convert WorkflowInput to JSON representation
                serde_json::to_value(&input).unwrap_or(JsonValue::Null)
            }
        }
    }

    /// Convert to graphica-core WorkflowInput type
    ///
    /// This method provides compatibility with graph-native workflow handlers
    /// that need the underlying WorkflowInput type from graphica-core.
    pub fn into_workflow_input(self) -> graphica_core::orchestration::workflow::WorkflowInput {
        match self {
            WorkflowInputWrapper::Json(value) => {
                graphica_core::orchestration::workflow::WorkflowInput::Json { data: value }
            }
            WorkflowInputWrapper::GraphNative(input) => input,
        }
    }
}

impl Default for WorkflowInputWrapper {
    fn default() -> Self {
        WorkflowInputWrapper::Json(JsonValue::Null)
    }
}

/// ML feature value tracked during workflow execution
///
/// Captures feature values extracted or computed during workflow
/// execution for ML model training, inference, or feature store population.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlFeatureValue {
    /// Feature name (e.g., "customer_lifetime_value")
    pub feature_name: String,

    /// Feature value (can be numeric, string, boolean, etc.)
    pub value: JsonValue,

    /// Optional model ID that computed this feature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// Feature computation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<DateTime<Utc>>,
}

/// Lineage reference for data provenance tracking
///
/// References to upstream data sources, transformations, or models
/// that contributed to workflow execution results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageReference {
    /// Type of lineage reference (e.g., "source", "transformation", "model")
    pub ref_type: String,

    /// Unique identifier for the referenced entity
    pub ref_id: String,

    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// === Workflow Testing DTOs ===

/// Request to perform a dry-run execution of a workflow
///
/// Executes the workflow without persisting any changes or side effects,
/// allowing validation of workflow logic before actual execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunWorkflowRequest {
    pub input: JsonValue,
    #[serde(default)]
    pub context: ExecutionContextParams,
}

/// Response from dry-run execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunWorkflowResponse {
    pub success: bool,
    pub steps_executed: Vec<StepExecutionResult>,
    pub final_output: Option<JsonValue>,
    pub total_execution_time_ms: u64,
    pub failed_step: Option<String>,
}

// === Conversion Helpers ===

impl From<RouteDto> for Route {
    fn from(dto: RouteDto) -> Self {
        Route {
            id: uuid::Uuid::new_v4().to_string(),
            name: dto.name,
            description: dto.description,
            condition: dto.condition,
            actions: Box::new(dto.actions),
            priority: dto.priority,
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_workflow_request_serde() {
        let req = CreateWorkflowRequest {
            name: "test_workflow".to_string(),
            description: "Test description".to_string(),
            routes: vec![RouteDto {
                name: "test_route".to_string(),
                description: String::new(),
                condition: Box::new(Condition::Always),
                actions: vec![Action::Log {
                    level: "info".to_string(),
                    message: "test".to_string(),
                }],
                priority: 10,
            }],
            default_route: None,
            tags: vec!["test".to_string()],
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CreateWorkflowRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test_workflow");
        assert_eq!(deserialized.routes.len(), 1);
    }

    #[test]
    fn test_execute_workflow_request_serde() {
        let req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"test": "data"})),
            context: None,
            dry_run: false,
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ExecuteWorkflowRequest = serde_json::from_str(&json).unwrap();

        // Extract JSON from wrapper and check
        let input_json = deserialized.input.into_json();
        assert_eq!(input_json["test"], "data");
    }

    #[test]
    fn test_workflow_input_wrapper_deserializes_graph_native_first() {
        let value = json!({
            "type": "dataset",
            "dataset_id": "ds_datasource_123",
            "batch_size": 100
        });

        let wrapper: WorkflowInputWrapper = serde_json::from_value(value).unwrap();
        match wrapper {
            WorkflowInputWrapper::GraphNative(
                graphica_core::orchestration::workflow::WorkflowInput::Dataset {
                    dataset_id,
                    batch_size,
                    limit,
                },
            ) => {
                assert_eq!(dataset_id, "ds_datasource_123");
                assert_eq!(batch_size, Some(100));
                assert_eq!(limit, None);
            }
            _ => panic!("Expected graph-native dataset workflow input"),
        }
    }

    #[test]
    fn test_error_response() {
        let err = ErrorResponse::new("NotFound", "Workflow not found")
            .with_details(json!({"workflow_id": "wf_001"}));

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("NotFound"));
        assert!(json.contains("workflow_id"));
    }

    #[test]
    fn test_execution_async_response_serde() {
        let resp = ExecuteWorkflowAsyncResponse {
            execution_id: "exec_001".to_string(),
            workflow_id: "wf_001".to_string(),
            workflow_name: "Test Workflow".to_string(),
            status: ExecutionStatus::Running,
            started_at: Utc::now(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ExecuteWorkflowAsyncResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.execution_id, "exec_001");
        assert_eq!(deserialized.workflow_id, "wf_001");
        assert_eq!(deserialized.status, ExecutionStatus::Running);
    }

    #[test]
    fn test_get_execution_response_from_workflow_execution() {
        use crate::workflows::domain::WorkflowExecution;

        let execution = WorkflowExecution::new(
            "exec_001".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            json!({"input": "data"}),
            Some("user@example.com".to_string()),
        );

        let resp: GetExecutionResponse = execution.into();

        assert_eq!(resp.execution_id, "exec_001");
        assert_eq!(resp.workflow_id, "wf_001");
        assert_eq!(resp.status, ExecutionStatus::Pending);
        assert_eq!(resp.triggered_by, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_execution_log_dto_conversion() {
        let log = ExecutionLog::info("Test log message")
            .with_step("step_001")
            .with_details(json!({"key": "value"}));

        let dto: ExecutionLogDto = log.into();

        assert_eq!(dto.message, "Test log message");
        assert_eq!(dto.level, LogLevel::Info);
        assert_eq!(dto.step_id, Some("step_001".to_string()));
        assert_eq!(dto.details, Some(json!({"key": "value"})));
    }

    #[test]
    fn test_list_executions_query_defaults() {
        let json = r#"{}"#;
        let query: ListExecutionsQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 0);
        assert!(query.workflow_id.is_none());
        assert!(query.status.is_none());
    }

    #[test]
    fn test_execution_summary_from_workflow_execution() {
        use crate::workflows::domain::WorkflowExecution;

        let mut execution = WorkflowExecution::new(
            "exec_001".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Completed);
        execution.actions_executed = 5;

        let summary: ExecutionSummary = execution.into();

        assert_eq!(summary.execution_id, "exec_001");
        assert_eq!(summary.status, ExecutionStatus::Completed);
        assert_eq!(summary.actions_executed, 5);
        assert!(summary.duration_ms.is_some());
    }

    #[test]
    fn test_execution_lifecycle_response_serde() {
        let resp = ExecutionLifecycleResponse {
            execution_id: "exec_001".to_string(),
            status: ExecutionStatus::Stopped,
            message: "Execution stopped successfully".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ExecutionLifecycleResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.execution_id, "exec_001");
        assert_eq!(deserialized.status, ExecutionStatus::Stopped);
        assert!(deserialized.message.contains("stopped"));
    }

    #[test]
    fn test_update_schedule_request_serde() {
        let req = UpdateScheduleRequest {
            cron_expression: "0 0 * * *".to_string(),
            enabled: true,
            timezone: Some("America/New_York".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: UpdateScheduleRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.cron_expression, "0 0 * * *");
        assert_eq!(deserialized.enabled, true);
        assert_eq!(deserialized.timezone, Some("America/New_York".to_string()));
    }

    #[test]
    fn test_schedule_preview_request_defaults() {
        let json = r#"{"cron_expression": "0 0 * * *"}"#;
        let req: SchedulePreviewRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.cron_expression, "0 0 * * *");
        assert_eq!(req.count, 10); // default
        assert!(req.timezone.is_none());
    }

    #[test]
    fn test_schedule_preview_response_serde() {
        let resp = SchedulePreviewResponse {
            cron_expression: "0 0 * * *".to_string(),
            timezone: "UTC".to_string(),
            next_runs: vec![Utc::now(), Utc::now()],
            is_valid: true,
            validation_error: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: SchedulePreviewResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.cron_expression, "0 0 * * *");
        assert_eq!(deserialized.is_valid, true);
        assert_eq!(deserialized.next_runs.len(), 2);
    }

    #[test]
    fn test_get_execution_logs_response_serde() {
        let resp = GetExecutionLogsResponse {
            execution_id: "exec_001".to_string(),
            logs: vec![
                ExecutionLogDto {
                    timestamp: Utc::now(),
                    level: LogLevel::Info,
                    step_id: None,
                    message: "Log 1".to_string(),
                    details: None,
                },
                ExecutionLogDto {
                    timestamp: Utc::now(),
                    level: LogLevel::Error,
                    step_id: Some("step_001".to_string()),
                    message: "Log 2".to_string(),
                    details: Some(json!({"error_code": 500})),
                },
            ],
            total: 2,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: GetExecutionLogsResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.execution_id, "exec_001");
        assert_eq!(deserialized.logs.len(), 2);
        assert_eq!(deserialized.total, 2);
        assert_eq!(deserialized.logs[0].level, LogLevel::Info);
        assert_eq!(deserialized.logs[1].level, LogLevel::Error);
    }
}
