//! Workflow execution runtime
//!
//! ## Architecture Note
//!
//! This module provides a **basic workflow executor** intended for:
//! - Unit testing workflow definitions
//! - Integration testing in graphica-core
//! - Development/prototyping
//!
//! For **production workloads**, use `graphica-coordinator`'s implementations:
//! - `StreamingWorkflowExecutor` - High-throughput streaming with Timely/Differential
//! - `BatchWorkflowExecutor` - Parallel batch processing
//!
//! See [WORKFLOW_ARCHITECTURE.md](../../../WORKFLOW_ARCHITECTURE.md) for the separation of concerns.
//!
//! ## Basic Execution
//!
//! Executes workflow steps according to DAG dependencies

mod aggregator_step;
mod aggregator_support;
mod batch_context;
mod batch_results;
#[cfg(test)]
#[path = "executor/tests/batch_runtime_contracts.rs"]
mod batch_runtime_contracts_tests;
mod completion;
mod confidence_steps;
mod context_batch;
mod csv_export_support;
mod csv_exporter_step;
mod csv_source_step;
mod data_validator_step;
mod data_validator_support;
mod db_extract_step;
mod db_loader_step;
mod decisioning;
mod deduplicator_step;
mod deduplicator_support;
mod feature_resolution;
mod field_transformer_step;
mod field_transformer_support;
mod finalization;
#[cfg(test)]
#[path = "executor/tests/io_step_contracts.rs"]
mod io_step_contracts_tests;
mod lifecycle;
mod lineage;
mod ml_prediction_support;
mod modification_extraction;
mod orchestration;
#[cfg(test)]
#[path = "executor/tests/orchestration_contracts.rs"]
mod orchestration_contracts_tests;
mod prediction_extraction;
mod row_extraction;
mod rule_steps;
mod semantic_mapper_step;
mod state;
mod step_bookkeeping;
mod step_execution;
mod stub_steps;
mod utilities;
#[cfg(test)]
#[path = "executor/tests/workflow_core_contracts.rs"]
mod workflow_core_contracts_tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use self::batch_results::{
    build_batch_rows_step_result, build_batch_rows_success_result,
    build_materialized_rows_step_result, build_rows_output, BatchStepExecutionResult,
};
use self::utilities::extract_materializable_rows;
#[cfg(test)]
use self::utilities::parse_row_id_key;

/// Type alias for transformer callback functions
///
/// This callback allows the coordinator to inject transformer execution logic
/// (e.g., OntologyMapperTransformer with column lineage support) into the
/// core's WorkflowExecutor without creating a dependency from core to coordinator.
///
/// # Arguments
/// * `name` - Transformer name (e.g., "ontology_map")
/// * `config` - Transformer configuration as JSON
/// * `data` - Mutable data to transform (input/output)
/// * `context` - Workflow execution context for lineage and dependency access
///
/// # Returns
/// A pinned boxed future that resolves to a Result
pub type TransformerCallback = Box<
    dyn Fn(
            &str,
            &serde_json::Value,
            &mut serde_json::Value,
            &ExecutionContext,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Type alias for DB loader callback functions
///
/// This callback allows the coordinator to inject database loading logic
/// into the core's WorkflowExecutor without creating a dependency from core to coordinator.
///
/// # Arguments
/// * `datasource_id` - Datasource ID to load into
/// * `table_name` - Target table name
/// * `rows` - Data rows to load (as JSON objects)
/// * `mode` - Load mode (insert, upsert, replace)
/// * `key_fields` - Key fields for upsert-capable targets
///
/// # Returns
/// A pinned boxed future that resolves to Result<DbLoadResult>.
pub type DbLoaderCallback = Box<
    dyn Fn(
            &str,
            &str,
            Vec<serde_json::Map<String, serde_json::Value>>,
            &str,
            Option<Vec<String>>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<DbLoadResult>> + Send>>
        + Send
        + Sync,
>;

/// Result of a DB load callback.
#[derive(Debug, Clone)]
pub struct DbLoadResult {
    pub rows_loaded: u64,
    pub output_row_ids: Vec<Option<crate::core::lineage::row_level::RowId>>,
}

/// Result of a DB extract callback.
#[derive(Debug, Clone)]
pub struct DbExtractResult {
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub row_count: usize,
    pub schema: Option<serde_json::Value>,
}

/// DB extract callback signature.
///
/// Allows the coordinator to provide database extraction logic without
/// introducing a dependency from graphica-core to graphica-coordinator.
pub type DbExtractCallback = Box<
    dyn Fn(
            &crate::orchestration::workflow::definition::DbExtractConfig,
            &ExecutionContext,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<DbExtractResult>> + Send>>
        + Send
        + Sync,
>;

#[cfg(test)]
use self::state::{
    ExecuteLoopOutcome, ExecuteLoopState, WorkflowExecutionSession, WorkflowRunState,
};
use super::dag::DagExecutor;
use super::definition::{StepConfig, StepType, WorkflowDefinition, WorkflowStep};
#[cfg(test)]
use super::lineage_tracker::StepExecutionRecord;
use super::lineage_tracker::{FieldModificationRecord, LineageTracker, WorkflowExecutionRecord};
use super::row_lineage_context::RowLineageContext;
use super::runtime::frame::{BatchFrame, BatchFrameMetadata};
use super::runtime::metrics::RuntimeStepMetrics;
use crate::orchestration::ml::ModelInvoker;
use crate::orchestration::rules::RuleExecutor;

/// Basic workflow executor for testing and development
///
/// This executor provides synchronous, in-process execution of workflow definitions.
/// It is **NOT suitable for production workloads** which require:
/// - High throughput (100K+ events/sec)
/// - Streaming dataflow with Timely/Differential
/// - RocksDB-based state management and checkpointing
/// - Distributed execution across multiple workers
///
/// For production use, see `graphica-coordinator`'s `StreamingWorkflowExecutor` and `BatchWorkflowExecutor`.
///
/// # Usage
///
/// This executor is primarily used for:
/// - Testing workflow definitions in unit tests
/// - Validating workflow logic during development
/// - Prototyping new workflow features
pub struct WorkflowExecutor {
    /// Workflow definition
    definition: WorkflowDefinition,
    /// DAG executor for step ordering
    dag: DagExecutor,
    /// ML model invoker
    model_invoker: Arc<ModelInvoker>,
    /// Rule executor (heuristics and WASM)
    rule_executor: Arc<RuleExecutor>,
    /// Optional lineage tracker for RDF triple generation
    lineage_tracker: Option<Arc<dyn LineageTracker>>,
    /// Optional transformer callback for ETL steps (injected by coordinator)
    ///
    /// This allows the coordinator to inject its TransformerRegistry execution
    /// into the core's WorkflowExecutor, enabling steps like SemanticMapper
    /// to use the real OntologyMapperTransformer with column lineage support.
    transformer_callback: Option<Arc<TransformerCallback>>,
    /// Optional DB loader callback for database loading (injected by coordinator)
    ///
    /// This allows the coordinator to inject its database connectivity logic
    /// into the core's WorkflowExecutor, enabling DB loader steps to actually
    /// load data without creating a dependency from core to coordinator.
    db_loader_callback: Option<Arc<DbLoaderCallback>>,
    /// Optional DB extract callback for database extraction (injected by coordinator)
    ///
    /// This allows the coordinator to inject its database extraction logic
    /// into the core's WorkflowExecutor, enabling DbExtract steps to run
    /// without creating a dependency from core to coordinator.
    db_extract_callback: Option<Arc<DbExtractCallback>>,
}

impl WorkflowExecutor {
    /// Create executor for workflow with dependencies
    pub fn new(
        definition: WorkflowDefinition,
        model_invoker: Arc<ModelInvoker>,
        rule_executor: Arc<RuleExecutor>,
    ) -> Result<Self> {
        let dag = DagExecutor::from_workflow(&definition).context("Failed to build DAG")?;

        Ok(Self {
            definition,
            dag,
            model_invoker,
            rule_executor,
            lineage_tracker: None,
            transformer_callback: None,
            db_loader_callback: None,
            db_extract_callback: None,
        })
    }

    /// Create executor with lineage tracking enabled
    pub fn with_lineage(
        definition: WorkflowDefinition,
        model_invoker: Arc<ModelInvoker>,
        rule_executor: Arc<RuleExecutor>,
        lineage_tracker: Arc<dyn LineageTracker>,
    ) -> Result<Self> {
        let dag = DagExecutor::from_workflow(&definition).context("Failed to build DAG")?;

        Ok(Self {
            definition,
            dag,
            model_invoker,
            rule_executor,
            lineage_tracker: Some(lineage_tracker),
            transformer_callback: None,
            db_loader_callback: None,
            db_extract_callback: None,
        })
    }

    /// Set a transformer callback for ETL step execution
    ///
    /// This allows the coordinator to inject its TransformerRegistry execution
    /// into the core's WorkflowExecutor, enabling steps like SemanticMapper
    /// to use the real OntologyMapperTransformer with column lineage support.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let executor = WorkflowExecutor::new(definition, invoker, rules)?
    ///     .with_transformer_callback(callback);
    /// ```
    pub fn with_transformer_callback(mut self, callback: Arc<TransformerCallback>) -> Self {
        self.transformer_callback = Some(callback);
        self
    }

    /// Set a DB loader callback for database loading
    ///
    /// This allows the coordinator to inject database connectivity logic
    /// into the core's WorkflowExecutor, enabling DB loader steps to actually
    /// load data without creating a dependency from core to coordinator.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let executor = WorkflowExecutor::new(definition, invoker, rules)?
    ///     .with_db_loader_callback(callback);
    /// ```
    pub fn with_db_loader_callback(mut self, callback: Arc<DbLoaderCallback>) -> Self {
        self.db_loader_callback = Some(callback);
        self
    }

    /// Set a DB extract callback for database extraction
    ///
    /// This allows the coordinator to inject database extraction logic
    /// into the core's WorkflowExecutor, enabling DbExtract steps to actually
    /// extract data without creating a dependency from core to coordinator.
    pub fn with_db_extract_callback(mut self, callback: Arc<DbExtractCallback>) -> Self {
        self.db_extract_callback = Some(callback);
        self
    }

    /// Execute workflow with given input context
    pub async fn execute(&self, input: ExecutionContext) -> Result<WorkflowResult> {
        let session = self.initialize_execution_session().await?;
        self.execute_session(session, input).await
    }

    // ============================================================================
    // ETL Step Executors
    // ============================================================================
}

/// Resource limits for workflow execution (Proposal 5 - Memory Management)
///
/// Prevents OOM crashes by enforcing memory and row count limits during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes (None = unlimited)
    ///
    /// Recommended values:
    /// - Small datasets (<100K rows): 5GB = 5_000_000_000
    /// - Medium datasets (100K-500K rows): 20GB = 20_000_000_000
    /// - Large datasets (500K-1M rows): 50GB = 50_000_000_000
    pub max_memory_bytes: Option<usize>,

    /// Maximum number of rows to process (None = unlimited)
    ///
    /// Recommended values:
    /// - Development: 10_000
    /// - Testing: 100_000
    /// - Production (with monitoring): 1_000_000
    pub max_rows: Option<usize>,

    /// Whether to enforce limits strictly (true) or just warn (false)
    pub enforce_limits: bool,

    /// Number of rows to process before yielding to other async tasks (Phase 2)
    ///
    /// Controls how frequently workflow execution yields control back to the
    /// async runtime, allowing other tasks (health checks, API requests) to run.
    ///
    /// Recommended values based on dataset size:
    /// - Small (<100K rows): 10,000 - 50,000 rows
    /// - Medium (100K-1M): 5,000 - 10,000 rows
    /// - Large (>1M): 1,000 - 5,000 rows
    ///
    /// Lower values = more responsive but slightly lower throughput
    /// Higher values = higher throughput but less responsive
    ///
    /// Default: 10,000 rows (~200ms of CPU time for typical operations)
    pub yield_interval: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(10_000_000_000), // 10GB default
            max_rows: Some(200_000),                // 200K rows default
            enforce_limits: true,
            yield_interval: 10_000, // Yield every 10K rows (Phase 2)
        }
    }
}

impl ResourceLimits {
    /// Create unlimited resource limits (use with caution!)
    pub fn unlimited() -> Self {
        Self {
            max_memory_bytes: None,
            max_rows: None,
            enforce_limits: false,
            yield_interval: 10_000, // Still yield for responsiveness
        }
    }

    /// Create strict limits for development/testing
    pub fn strict() -> Self {
        Self {
            max_memory_bytes: Some(5_000_000_000), // 5GB
            max_rows: Some(100_000),               // 100K rows
            enforce_limits: true,
            yield_interval: 5_000, // More frequent yields for testing
        }
    }

    /// Create production limits with monitoring
    pub fn production() -> Self {
        Self {
            max_memory_bytes: Some(50_000_000_000), // 50GB
            max_rows: Some(1_000_000),              // 1M rows
            enforce_limits: true,
            yield_interval: 10_000, // Balanced for production
        }
    }
}

/// Execution context passed to workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Original input data for workflow (immutable)
    pub input_data: serde_json::Value,
    /// Working data that gets transformed by each step (mutable pipeline)
    pub working_data: serde_json::Value,
    /// Step outputs (populated during execution)
    pub step_outputs: HashMap<String, serde_json::Value>,
    /// User-provided metadata
    pub metadata: HashMap<String, String>,
    /// Row-level lineage context for ETL tracking
    pub row_lineage: Option<RowLineageContext>,
    /// Optional workflow identifier for lineage and logging
    pub workflow_id: Option<String>,
    /// Resource limits for execution (Proposal 5 - Memory Management)
    ///
    /// Prevents OOM crashes by enforcing memory and row count limits.
    /// Defaults to 10GB/200K rows if not specified.
    pub resource_limits: ResourceLimits,
    /// Progress tracker for real-time monitoring (Phase 3)
    #[serde(skip)]
    pub progress_tracker: Option<Arc<super::progress::ProgressTracker>>,
    /// Cancellation token for graceful shutdown (Phase 3)
    #[serde(skip)]
    pub cancellation_token: Option<super::cancellation::CancellationToken>,
    /// Cached batch-oriented view of the current working rows.
    #[serde(skip)]
    pub batch_frame: Option<BatchFrame>,
}

pub(super) const LARGE_ROW_PAYLOAD_THRESHOLD: u64 = 10_000;

impl ExecutionContext {
    pub fn new(input_data: serde_json::Value) -> Self {
        // Initialize working_data as a copy of input_data
        let working_data = input_data.clone();
        let batch_frame = Self::infer_batch_frame_from_value(&working_data);
        Self {
            input_data,
            working_data,
            step_outputs: HashMap::new(),
            metadata: HashMap::new(),
            row_lineage: None,
            workflow_id: None,
            resource_limits: ResourceLimits::default(),
            progress_tracker: None,
            cancellation_token: None,
            batch_frame,
        }
    }

    /// Create a legacy execution context from arbitrary input data while
    /// normalizing row-oriented payloads through the batch-frame bridge.
    pub fn from_input_value(input_data: serde_json::Value) -> Result<Self> {
        if let Some(rows) = input_data.as_array() {
            return Self::from_batch_frame(BatchFrame::from_json_values(rows)?);
        }

        if let Some(rows) = input_data.get("_rows").and_then(|value| value.as_array()) {
            // Validate that embedded row payloads are compatible with the
            // batch-frame bridge while preserving surrounding metadata.
            BatchFrame::from_json_values(rows)?;
        }

        Ok(Self::new(input_data))
    }

    /// Create a legacy execution context from a batch-oriented frame.
    pub fn from_batch_frame(frame: BatchFrame) -> Result<Self> {
        let input_data = serde_json::Value::Array(frame.to_json_values()?);
        let working_data = input_data.clone();

        Ok(Self {
            input_data,
            working_data,
            step_outputs: HashMap::new(),
            metadata: HashMap::new(),
            row_lineage: None,
            workflow_id: None,
            resource_limits: ResourceLimits::default(),
            progress_tracker: None,
            cancellation_token: None,
            batch_frame: Some(frame),
        })
    }

    /// Merge a step output into the working data while keeping the cached
    /// batch view synchronized with the current row payload.
    pub fn merge_step_output(&mut self, output: &serde_json::Value) -> Result<()> {
        self.merge_step_output_with_batch(output, None)
    }

    pub fn merge_step_output_with_batch(
        &mut self,
        output: &serde_json::Value,
        batch_frame: Option<BatchFrame>,
    ) -> Result<()> {
        let working_output = Self::build_working_output_with_batch(output, batch_frame.as_ref());

        if let serde_json::Value::Object(ref mut working_obj) = self.working_data {
            if let serde_json::Value::Object(output_obj) = &working_output {
                for (key, value) in output_obj {
                    working_obj.insert(key.clone(), value.clone());
                }
            }
        } else if matches!(working_output, serde_json::Value::Object(_)) {
            self.working_data = working_output;
        }

        if let Some(frame) = batch_frame {
            self.batch_frame = Some(frame);
            Ok(())
        } else {
            self.refresh_batch_frame_from_working_data()
        }
    }

    /// Create context with row lineage tracking enabled
    pub fn with_row_lineage(
        mut self,
        execution_id: String,
        job_id: String,
        tenant_id: String,
    ) -> Self {
        self.row_lineage = Some(RowLineageContext::new(execution_id, job_id, tenant_id));
        self
    }

    /// Set workflow ID for lineage and logging
    pub fn with_workflow_id(mut self, workflow_id: String) -> Self {
        self.workflow_id = Some(workflow_id);
        self
    }

    /// Set resource limits for execution (Proposal 5 - Memory Management)
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Set progress tracker for real-time monitoring (Phase 3)
    pub fn with_progress_tracker(mut self, tracker: Arc<super::progress::ProgressTracker>) -> Self {
        self.progress_tracker = Some(tracker);
        self
    }

    /// Set cancellation token for graceful shutdown (Phase 3)
    pub fn with_cancellation_token(
        mut self,
        token: super::cancellation::CancellationToken,
    ) -> Self {
        self.cancellation_token = Some(token);
        self
    }
}

/// Result of workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub execution_id: String,
    pub success: bool,
    pub final_decision: FinalDecision,
    pub confidence: f64,
    pub step_results: HashMap<String, StepResult>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    pub error: Option<String>,
    #[serde(skip)]
    pub final_output: serde_json::Value,
    #[serde(skip)]
    pub output_rows: Option<Vec<serde_json::Value>>,
}

/// Result of single step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub confidence: f64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_metadata: Option<BatchFrameMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_metrics: Option<RuntimeStepMetrics>,
    #[serde(skip)]
    pub batch_frame: Option<BatchFrame>,
}

/// Final decision from workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FinalDecision {
    Accept,
    Reject,
    ManualReview,
}
