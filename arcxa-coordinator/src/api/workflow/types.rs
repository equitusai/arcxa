//! Workflow API request/response types

use chrono::{DateTime, Utc};
use graphica_core::orchestration::workflow::runtime::metrics::RuntimeStepMetrics;
use graphica_core::orchestration::workflow::{WorkflowDefinition, WorkflowInput, WorkflowStep};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Re-export graphica-core types for OpenAPI schema generation
pub use graphica_core::orchestration::workflow::definition::{
    AggFunction,
    Aggregation,
    AggregatorConfig,
    ConfidenceAggregateConfig,
    ConfidenceGateConfig,
    CsvExporterConfig,
    CsvSourceConfig,
    DataJoinerConfig,
    DataValidatorConfig,
    DbExtractConfig,
    DbLoaderConfig,
    DedupMethod,
    DeduplicatorConfig,
    FallbackStrategy,
    // Re-export nested types referenced by configs
    FeatureMapping,
    FieldTransformation,
    FieldTransformerConfig,
    FuzzyAlgorithm,
    HeuristicConfig,
    JoinType,
    KeepStrategy,
    LoadMode,
    // Re-export all the config types that StepConfig references
    MLPredictionConfig,
    MappingMode,
    PredictionSpec,
    RdfLoaderConfig,
    RuleType,
    SemanticMapperConfig,
    Severity,
    StepConfig,
    StepType,
    TransformOperation,
    ValidationRule,
    WasmRuleConfig,
    WeightedVoteConfig,
};

// ============================================================================
// Registration Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterWorkflowRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub definition: WorkflowDefinition,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterWorkflowResponse {
    pub workflow_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowDetailsResponse {
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub version: String,
    pub execution_count: u64,
    pub last_executed_at: Option<DateTime<Utc>>,
    pub definition: WorkflowDefinition,
}

// ============================================================================
// Execution Types
// ============================================================================

/// Workflow execution request with graph-native input support
///
/// Supports both new graph-native input (SPARQL, entity filters) and
/// legacy JSON input for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteWorkflowRequest {
    /// Workflow input specification
    ///
    /// Can be:
    /// - WorkflowInput object (new - graph-native)
    /// - Raw JSON value (legacy - backward compatible)
    #[serde(flatten)]
    pub input: WorkflowInputWrapper,

    #[serde(default)]
    pub context: ExecutionContextParams,

    /// Optional request to materialize the final workflow rows as a catalog dataset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dataset: Option<WorkflowOutputDatasetRequest>,
}

/// Wrapper to support both old and new input formats
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum WorkflowInputWrapper {
    /// New graph-native input
    GraphNative(WorkflowInput),

    /// Legacy JSON input (for backward compatibility)
    ///
    /// Automatically wraps in WorkflowInput::Json
    Legacy { input: serde_json::Value },
}

impl WorkflowInputWrapper {
    /// Convert to WorkflowInput
    pub fn into_workflow_input(self) -> WorkflowInput {
        match self {
            WorkflowInputWrapper::GraphNative(input) => input,
            WorkflowInputWrapper::Legacy { input } => WorkflowInput::Json { data: input },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ExecutionContextParams {
    pub request_id: Option<String>,
    pub initiator: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ExecutionContextParams {
    /// Convert to HashMap for workflow engine
    pub fn to_hashmap(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();

        if let Some(ref request_id) = self.request_id {
            map.insert("request_id".to_string(), request_id.clone());
        }

        if let Some(ref initiator) = self.initiator {
            map.insert("initiator".to_string(), initiator.clone());
        }

        // Convert metadata JSON to string entries
        if let Some(obj) = self.metadata.as_object() {
            for (key, value) in obj {
                if let Some(s) = value.as_str() {
                    map.insert(key.clone(), s.to_string());
                } else {
                    map.insert(key.clone(), value.to_string());
                }
            }
        }

        map
    }
}

/// Workflow execution response
///
/// For batched execution (e.g., SPARQL queries returning multiple batches),
/// contains multiple results. For single execution, contains one result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteWorkflowResponse {
    pub workflow_id: String,

    /// Execution results (one per batch)
    pub results: Vec<ExecutionResultDto>,

    /// Total number of batches processed
    pub batch_count: usize,

    /// Overall success (true if all batches succeeded)
    pub overall_success: bool,

    /// Average confidence across all batches
    pub average_confidence: f64,

    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

impl ExecuteWorkflowResponse {
    /// Create response from single execution result (legacy format)
    pub fn single(
        workflow_id: String,
        result: ExecutionResultDto,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Self {
        let success = result.success;
        let confidence = result.confidence;

        Self {
            workflow_id,
            results: vec![result],
            batch_count: 1,
            overall_success: success,
            average_confidence: confidence,
            started_at,
            completed_at,
        }
    }

    /// Create response from multiple batch results
    pub fn batched(
        workflow_id: String,
        results: Vec<ExecutionResultDto>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Self {
        let batch_count = results.len();
        let overall_success = results.iter().all(|r| r.success);
        let average_confidence = if !results.is_empty() {
            results.iter().map(|r| r.confidence).sum::<f64>() / results.len() as f64
        } else {
            0.0
        };

        Self {
            workflow_id,
            results,
            batch_count,
            overall_success,
            average_confidence,
            started_at,
            completed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionResultDto {
    pub execution_id: String,
    pub success: bool,
    pub step_results: Vec<StepResultDto>,
    pub final_output: serde_json::Value,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_metrics: Option<ExecutionRuntimeMetricsDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialized_dataset: Option<WorkflowOutputDatasetRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepResultDto {
    pub step_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub confidence: f64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_metrics: Option<RuntimeStepMetricsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuntimeStepMetricsDto {
    pub input_rows: usize,
    pub output_rows: usize,
    pub materialization_count: usize,
    pub spill_events: usize,
    pub spill_bytes: usize,
    pub memory_high_water_mark: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_decision_reason: Option<String>,
    pub reserved_spill_bytes: usize,
    pub execution_reserved_spill_bytes: usize,
    pub total_reserved_spill_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_location: Option<String>,
    pub pushdown_applied: bool,
}

impl From<&RuntimeStepMetrics> for RuntimeStepMetricsDto {
    fn from(metrics: &RuntimeStepMetrics) -> Self {
        Self {
            input_rows: metrics.input_rows,
            output_rows: metrics.output_rows,
            materialization_count: metrics.materialization_count,
            spill_events: metrics.spill_events,
            spill_bytes: metrics.spill_bytes,
            memory_high_water_mark: metrics.memory_high_water_mark,
            storage_type: metrics.storage_type.clone(),
            storage_operation: metrics.storage_operation.clone(),
            planned_tier: metrics.planned_tier.clone(),
            storage_decision_reason: metrics.storage_decision_reason.clone(),
            reserved_spill_bytes: metrics.reserved_spill_bytes,
            execution_reserved_spill_bytes: metrics.execution_reserved_spill_bytes,
            total_reserved_spill_bytes: metrics.total_reserved_spill_bytes,
            storage_location: metrics.storage_location.clone(),
            pushdown_applied: metrics.pushdown_applied,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionRuntimeMetricsDto {
    pub steps_with_runtime_metrics: usize,
    pub steps_with_disk_storage: usize,
    pub total_spill_events: usize,
    pub total_spill_bytes: usize,
    pub max_memory_high_water_mark: usize,
    pub max_reserved_spill_bytes: usize,
    pub max_execution_reserved_spill_bytes: usize,
    pub max_total_reserved_spill_bytes: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub storage_backends: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub planned_tiers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub storage_decision_reasons: Vec<String>,
}

impl From<&crate::workflows::domain::ExecutionRuntimeMetricsSummary>
    for ExecutionRuntimeMetricsDto
{
    fn from(metrics: &crate::workflows::domain::ExecutionRuntimeMetricsSummary) -> Self {
        Self {
            steps_with_runtime_metrics: metrics.steps_with_runtime_metrics,
            steps_with_disk_storage: metrics.steps_with_disk_storage,
            total_spill_events: metrics.total_spill_events,
            total_spill_bytes: metrics.total_spill_bytes,
            max_memory_high_water_mark: metrics.max_memory_high_water_mark,
            max_reserved_spill_bytes: metrics.max_reserved_spill_bytes,
            max_execution_reserved_spill_bytes: metrics.max_execution_reserved_spill_bytes,
            max_total_reserved_spill_bytes: metrics.max_total_reserved_spill_bytes,
            storage_backends: metrics.storage_backends.clone(),
            planned_tiers: metrics.planned_tiers.clone(),
            storage_decision_reasons: metrics.storage_decision_reasons.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowOutputDatasetRequest {
    /// Optional catalog name override for the materialized dataset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowOutputDatasetRef {
    pub dataset_id: String,
    pub name: String,
    pub dataset_type: String,
    pub asset_kind: String,
    pub record_count: u64,
    pub file_size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Query Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowSummaryDto {
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Testing/Validation Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestWorkflowStepRequest {
    pub step: WorkflowStep,
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: ExecutionContextParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowValidationIssueLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowValidationIssue {
    pub level: WorkflowValidationIssueLevel,
    pub step_id: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateWorkflowResponse {
    pub valid: bool,
    pub message: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub step_count: usize,
    pub has_conditional_logic: bool,
    pub has_error_handling: bool,
    #[serde(default)]
    pub issues: Vec<WorkflowValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestWorkflowStepResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
    pub step_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DryRunWorkflowRequest {
    pub definition: WorkflowDefinition,
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: ExecutionContextParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DryRunWorkflowResponse {
    pub success: bool,
    pub steps_executed: Vec<StepExecutionResult>,
    pub final_output: Option<serde_json::Value>,
    pub total_execution_time_ms: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepExecutionResult {
    pub step_id: String,
    pub step_type: String,
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

// ============================================================================
// Scheduling Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScheduleWorkflowRequest {
    pub schedule_id: Option<String>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<u64>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>, // IANA timezone (e.g., "America/New_York", "UTC")
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: ExecutionContextParams,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateScheduleRequest {
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<u64>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>, // Can update timezone
    pub input: Option<serde_json::Value>,
    pub context: Option<ExecutionContextParams>,
    pub enabled: Option<bool>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScheduleWorkflowResponse {
    pub schedule_id: String,
    pub workflow_id: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<u64>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
    pub next_execution: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowScheduleInfo {
    pub schedule_id: String,
    pub workflow_id: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<u64>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>, // IANA timezone for schedule execution
    pub next_execution: Option<DateTime<Utc>>,
    pub last_execution: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub execution_count: i32,
}

// ============================================================================
// Execution History Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowExecutionSummary {
    pub execution_id: String,
    pub workflow_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub success: bool,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_metrics: Option<ExecutionRuntimeMetricsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowStepProgressDto {
    pub step_name: String,
    pub step_type: String,
    pub rows_processed: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl From<graphica_core::orchestration::workflow::StepProgress> for WorkflowStepProgressDto {
    fn from(value: graphica_core::orchestration::workflow::StepProgress) -> Self {
        Self {
            step_name: value.step_name,
            step_type: value.step_type,
            rows_processed: value.rows_processed,
            started_at: value.started_at,
            completed_at: value.completed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowExecutionProgressDto {
    pub execution_id: String,
    pub workflow_id: String,
    pub status: graphica_core::orchestration::workflow::ExecutionStatus,
    pub current_step: Option<WorkflowStepProgressDto>,
    pub total_steps: usize,
    pub steps_completed: u64,
    pub rows_processed: u64,
    pub total_rows: Option<u64>,
    pub percent_complete: Option<f64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_updated: DateTime<Utc>,
    pub error: Option<String>,
    pub eta_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_metrics: Option<ExecutionRuntimeMetricsDto>,
}

impl WorkflowExecutionProgressDto {
    pub fn from_progress(
        progress: graphica_core::orchestration::workflow::WorkflowProgress,
        runtime_metrics: Option<ExecutionRuntimeMetricsDto>,
    ) -> Self {
        Self {
            execution_id: progress.execution_id,
            workflow_id: progress.workflow_id,
            status: progress.status,
            current_step: progress.current_step.map(WorkflowStepProgressDto::from),
            total_steps: progress.total_steps,
            steps_completed: progress.steps_completed,
            rows_processed: progress.rows_processed,
            total_rows: progress.total_rows,
            percent_complete: progress.percent_complete,
            started_at: progress.started_at,
            completed_at: progress.completed_at,
            last_updated: progress.last_updated,
            error: progress.error,
            eta_seconds: progress.eta_seconds,
            runtime_metrics,
        }
    }
}
