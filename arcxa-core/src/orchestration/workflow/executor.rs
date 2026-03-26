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
mod prediction_extraction;
mod row_extraction;
mod rule_steps;
mod semantic_mapper_step;
mod state;
mod step_bookkeeping;
mod step_execution;
mod stub_steps;
mod utilities;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use self::batch_results::{
    build_batch_rows_step_result, build_batch_rows_success_result, build_rows_output,
    BatchStepExecutionResult,
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
///
/// # Returns
/// A pinned boxed future that resolves to a Result
pub type TransformerCallback = Box<
    dyn Fn(
            &str,
            &serde_json::Value,
            &mut serde_json::Value,
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
/// A pinned boxed future that resolves to Result<u64> (rows loaded)
pub type DbLoaderCallback = Box<
    dyn Fn(
            &str,
            &str,
            Vec<serde_json::Map<String, serde_json::Value>>,
            &str,
            Option<Vec<String>>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send>>
        + Send
        + Sync,
>;

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
        if let serde_json::Value::Object(ref mut working_obj) = self.working_data {
            if let serde_json::Value::Object(output_obj) = output {
                for (key, value) in output_obj {
                    working_obj.insert(key.clone(), value.clone());
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::workflow::definition::{
        ConfidenceAggregateConfig, ConfidenceGateConfig, DataJoinerConfig, FallbackStrategy,
        HeuristicConfig, JoinType, RdfLoaderConfig, StepConfig, StepType, WasmRuleConfig,
        WeightedVoteConfig, WorkflowDefinition, WorkflowStep,
    };
    use crate::orchestration::workflow::lineage_tracker::MLPredictionStepRecord;
    use std::sync::Mutex;

    fn create_test_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "gate1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.5,
                    input_step: None,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    #[derive(Default)]
    struct TestLineageTracker {
        workflow_starts: Mutex<Vec<WorkflowExecutionRecord>>,
        step_records: Mutex<Vec<StepExecutionRecord>>,
        ml_records: Mutex<Vec<MLPredictionStepRecord>>,
        workflow_completions: Mutex<Vec<(String, bool)>>,
    }

    #[async_trait::async_trait]
    impl LineageTracker for TestLineageTracker {
        async fn record_workflow_start(
            &self,
            _record: WorkflowExecutionRecord,
        ) -> anyhow::Result<()> {
            self.workflow_starts.lock().unwrap().push(_record);
            Ok(())
        }

        async fn record_step_execution(&self, record: StepExecutionRecord) -> anyhow::Result<()> {
            self.step_records.lock().unwrap().push(record);
            Ok(())
        }

        async fn record_ml_predictions(
            &self,
            record: MLPredictionStepRecord,
        ) -> anyhow::Result<()> {
            self.ml_records.lock().unwrap().push(record);
            Ok(())
        }

        async fn record_workflow_complete(
            &self,
            execution_id: String,
            success: bool,
            _completed_at: chrono::DateTime<chrono::Utc>,
        ) -> anyhow::Result<()> {
            self.workflow_completions
                .lock()
                .unwrap()
                .push((execution_id, success));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_executor_creation() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        // Create dependencies
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let _executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    }

    #[tokio::test]
    async fn test_execute_workflow() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        // Create dependencies
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        // Provide confidence value that passes the gate (threshold is 0.5)
        let context = ExecutionContext::new(serde_json::json!({"confidence": 0.6}));
        let result = executor.execute(context).await.unwrap();

        assert!(result.success);
        assert_eq!(result.step_results.len(), 1);
    }

    #[tokio::test]
    async fn test_confidence_gate() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        // Create dependencies
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let mut context = ExecutionContext::new(serde_json::json!({}));
        context.step_outputs.insert(
            "previous_step".to_string(),
            serde_json::json!({"confidence": 0.9}),
        );

        let result = executor.execute(context).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_execute_weighted_vote_combines_step_confidences() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let mut context = ExecutionContext::new(serde_json::json!({}));
        context.step_outputs.insert(
            "ml_step".to_string(),
            serde_json::json!({"confidence": 0.8}),
        );
        context.step_outputs.insert(
            "rules_step".to_string(),
            serde_json::json!({"confidence": 0.2}),
        );

        let config = WeightedVoteConfig {
            weights: HashMap::from([
                ("ml_step".to_string(), 0.75),
                ("rules_step".to_string(), 0.25),
            ]),
        };

        let (success, output, confidence) = executor
            .execute_weighted_vote(&config, &context)
            .await
            .unwrap();

        assert!(success);
        assert!((confidence - 0.65).abs() < f64::EPSILON);
        let weighted_confidence = output
            .get("weighted_confidence")
            .and_then(|value| value.as_f64())
            .unwrap();
        assert!((weighted_confidence - 0.65).abs() < 1e-12);
    }

    #[tokio::test]
    async fn test_execute_confidence_aggregate_uses_declared_inputs() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let mut context = ExecutionContext::new(serde_json::json!({}));
        context.step_outputs.insert(
            "source_a".to_string(),
            serde_json::json!({"confidence": 0.2}),
        );
        context.step_outputs.insert(
            "source_b".to_string(),
            serde_json::json!({"confidence": 0.8}),
        );
        context.step_outputs.insert(
            "ignored_source".to_string(),
            serde_json::json!({"confidence": 1.0}),
        );

        let config = ConfidenceAggregateConfig {
            method: "weighted_average".to_string(),
            inputs: vec!["source_a".to_string(), "source_b".to_string()],
        };

        let (success, output, confidence) = executor
            .execute_confidence_aggregate(&config, &context)
            .await
            .unwrap();

        assert!(success);
        assert!((confidence - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            output,
            serde_json::json!({
                "method": "weighted_average",
                "aggregated_confidence": 0.5,
                "input_count": 2
            })
        );
    }

    #[tokio::test]
    async fn test_execute_heuristic_returns_contextual_error_for_missing_rule() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let context = ExecutionContext::new(serde_json::json!({"field": "value"}));
        let config = HeuristicConfig {
            rule_id: "missing_rule".to_string(),
            min_confidence: 0.5,
        };

        let error = executor
            .execute_heuristic(&config, &context)
            .await
            .expect_err("missing heuristic rule should surface an execution error");

        assert!(
            error
                .to_string()
                .contains("Heuristic rule execution failed"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn test_execute_wasm_rule_returns_contextual_error_for_missing_rule() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let context = ExecutionContext::new(serde_json::json!({"field": "value"}));
        let config = WasmRuleConfig {
            rule_id: "missing_wasm_rule".to_string(),
        };

        let error = executor
            .execute_wasm_rule(&config, &context)
            .await
            .expect_err("missing wasm rule should surface an execution error");

        assert!(
            error.to_string().contains("WASM rule execution failed"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn test_execute_data_joiner_returns_stub_output_shape() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let context = ExecutionContext::new(serde_json::json!({}));
        let config = DataJoinerConfig {
            join_type: JoinType::Left,
            left_key: vec!["customer_id".to_string()],
            right_key: vec!["id".to_string()],
            output_columns: None,
        };

        let (success, output, confidence) = executor
            .execute_data_joiner(&config, &context)
            .await
            .expect("stub data joiner should execute successfully");

        assert!(success);
        assert_eq!(confidence, 1.0);
        assert_eq!(
            output,
            serde_json::json!({
                "_join_type": "Left",
                "_left_key": ["customer_id"],
                "_right_key": ["id"],
                "_status": "stub_implementation",
                "_rows": [],
                "_row_count": 0,
            })
        );
    }

    #[tokio::test]
    async fn test_execute_rdf_loader_returns_stub_output_shape() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let context = ExecutionContext::new(serde_json::json!({
            "_rows": [
                {"id": "cust-1", "name": "Alice"},
                {"id": "cust-2", "name": "Bob"}
            ]
        }));
        let config = RdfLoaderConfig {
            target_graph: Some("urn:arcxa:test".to_string()),
            entity_type: "Customer".to_string(),
            id_field: "id".to_string(),
            batch_size: 1000,
            capture_lineage: true,
        };

        let (success, output, confidence) = executor
            .execute_rdf_loader(&config, &context)
            .await
            .expect("stub RDF loader should execute successfully");

        assert!(success);
        assert_eq!(confidence, 1.0);
        assert_eq!(
            output,
            serde_json::json!({
                "_entity_type": "Customer",
                "_id_field": "id",
                "_target_graph": "urn:arcxa:test",
                "_status": "stub_implementation",
                "_rows_to_load": 2,
            })
        );
    }

    #[test]
    fn test_build_step_execution_record_extracts_modifications() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let step = WorkflowStep {
            id: "transform_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let step_result = StepResult {
            step_id: step.id.clone(),
            success: true,
            output: serde_json::json!({
                "_modifications": [
                    {
                        "field_name": "email",
                        "old_value": "A@Example.COM",
                        "new_value": "a@example.com",
                        "is_reversible": true,
                        "operations": 2
                    }
                ]
            }),
            confidence: 0.9,
            started_at,
            completed_at,
            batch_metadata: None,
            batch_frame: None,
        };

        let record = executor.build_step_execution_record("exec_1", &step, &step_result);

        assert_eq!(record.execution_id, "exec_1");
        assert_eq!(record.step_id, "transform_step");
        assert_eq!(record.step_type, StepType::ConfidenceGate.to_string());
        assert_eq!(record.started_at, started_at);
        assert_eq!(record.completed_at, completed_at);
        assert_eq!(record.modifications.len(), 1);
        assert_eq!(record.modifications[0].field_name, "email");
        assert_eq!(record.modifications[0].old_value, "A@Example.COM");
        assert_eq!(record.modifications[0].new_value, "a@example.com");
        assert!(record.modifications[0].is_reversible);
        assert_eq!(record.modifications[0].operation_count, 2);
    }

    #[test]
    fn test_extract_modifications_skips_entries_without_field_name() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let modifications = executor.extract_modifications(&serde_json::json!({
            "_modifications": [
                {
                    "old_value": "ignored",
                    "new_value": "still_ignored",
                    "operations": 4
                },
                {
                    "field_name": "status",
                    "old_value": "NEW",
                    "new_value": "normalized",
                    "is_reversible": false,
                    "operations": 1
                }
            ]
        }));

        assert_eq!(modifications.len(), 1);
        assert_eq!(modifications[0].field_name, "status");
        assert_eq!(modifications[0].old_value, "NEW");
        assert_eq!(modifications[0].new_value, "normalized");
        assert!(!modifications[0].is_reversible);
        assert_eq!(modifications[0].operation_count, 1);
    }

    #[test]
    fn test_build_ml_prediction_step_record_extracts_predictions() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{MLPredictionConfig, PredictionSpec};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let step = WorkflowStep {
            id: "ml_step".to_string(),
            step_type: StepType::MlPrediction,
            config: StepConfig::MLPrediction(MLPredictionConfig {
                model_id: "customer-segmentation".to_string(),
                model_version: "2026.03".to_string(),
                features: vec!["email".to_string()],
                feature_mappings: vec![],
                predictions: vec![PredictionSpec {
                    attribute_name: "segment".to_string(),
                    mock_value: "enterprise".to_string(),
                    mock_confidence: 0.92,
                }],
                confidence_threshold: Some(0.8),
                timeout_ms: 500,
                cache_ttl_secs: Some(60),
            }),
            depends_on: vec![],
        };
        let step_result = StepResult {
            step_id: step.id.clone(),
            success: true,
            output: serde_json::json!({
                "_predictions": [
                    {
                        "attribute_name": "segment",
                        "value": "enterprise",
                        "confidence": 0.92
                    }
                ]
            }),
            confidence: 0.92,
            started_at,
            completed_at,
            batch_metadata: None,
            batch_frame: None,
        };

        let record = executor
            .build_ml_prediction_step_record("exec_ml", &step, &step_result)
            .expect("ml prediction step should yield a lineage record");

        assert_eq!(record.execution_id, "exec_ml");
        assert_eq!(record.step_id, "ml_step");
        assert_eq!(record.model_id, "customer-segmentation");
        assert_eq!(record.model_version, "2026.03");
        assert_eq!(record.started_at, started_at);
        assert_eq!(record.completed_at, completed_at);
        assert_eq!(record.predictions.len(), 1);
        assert_eq!(record.predictions[0].attribute_name, "segment");
        assert_eq!(record.predictions[0].value, "enterprise");
        assert_eq!(record.predictions[0].confidence, 0.92);
    }

    #[test]
    fn test_extract_predictions_skips_entries_without_attribute_name() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{MLPredictionConfig, PredictionSpec};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let config = StepConfig::MLPrediction(MLPredictionConfig {
            model_id: "customer-segmentation".to_string(),
            model_version: "2026.03".to_string(),
            features: vec!["email".to_string()],
            feature_mappings: vec![],
            predictions: vec![PredictionSpec {
                attribute_name: "segment".to_string(),
                mock_value: "enterprise".to_string(),
                mock_confidence: 0.92,
            }],
            confidence_threshold: Some(0.8),
            timeout_ms: 500,
            cache_ttl_secs: Some(60),
        });

        let extracted = executor
            .extract_predictions(
                &serde_json::json!({
                    "_predictions": [
                        {
                            "value": "ignored",
                            "confidence": 0.13
                        },
                        {
                            "attribute_name": "segment",
                            "value": "enterprise",
                            "confidence": 0.92
                        }
                    ]
                }),
                &config,
            )
            .expect("at least one valid prediction should be extracted");

        assert_eq!(extracted.model_id, "customer-segmentation");
        assert_eq!(extracted.model_version, "2026.03");
        assert_eq!(extracted.predictions.len(), 1);
        assert_eq!(extracted.predictions[0].attribute_name, "segment");
        assert_eq!(extracted.predictions[0].value, "enterprise");
        assert_eq!(extracted.predictions[0].confidence, 0.92);
    }

    #[test]
    fn test_generate_mock_prediction_returns_configured_mock_value() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::PredictionSpec;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let prediction = executor
            .generate_mock_prediction(
                &PredictionSpec {
                    attribute_name: "segment".to_string(),
                    mock_value: "enterprise".to_string(),
                    mock_confidence: 0.92,
                },
                &HashMap::from([("email".to_string(), serde_json::json!("a@example.com"))]),
                &ExecutionContext::new(serde_json::json!({})),
            )
            .expect("configured mock value should be returned directly");

        assert_eq!(prediction, serde_json::json!("enterprise"));
    }

    #[tokio::test]
    async fn test_execute_ml_prediction_emits_features_and_prediction_metadata() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            FeatureMapping, MLPredictionConfig, PredictionSpec,
        };

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let config = MLPredictionConfig {
            model_id: "customer-segmentation".to_string(),
            model_version: "2026.03".to_string(),
            features: vec![],
            feature_mappings: vec![FeatureMapping {
                feature_name: "normalized_email".to_string(),
                field_name: "email".to_string(),
                transform: Some("trim".to_string()),
            }],
            predictions: vec![PredictionSpec {
                attribute_name: "segment".to_string(),
                mock_value: "enterprise".to_string(),
                mock_confidence: 0.92,
            }],
            confidence_threshold: Some(0.8),
            timeout_ms: 500,
            cache_ttl_secs: Some(60),
        };
        let context = ExecutionContext::new(serde_json::json!({
            "email": "  USER@EXAMPLE.COM  "
        }));

        let (success, output, confidence) = executor
            .execute_ml_prediction(&config, &context)
            .await
            .expect("ml prediction execution should succeed");

        assert!(success);
        assert_eq!(confidence, 0.92);
        assert_eq!(output["segment"], "enterprise");
        assert_eq!(output["_model_id"], "customer-segmentation");
        assert_eq!(output["_model_version"], "2026.03");
        assert_eq!(
            output["_features_used"]["normalized_email"],
            "USER@EXAMPLE.COM"
        );
        assert_eq!(output["_predictions"].as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn test_record_step_lineage_routes_standard_and_ml_records() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{MLPredictionConfig, PredictionSpec};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let tracker = Arc::new(TestLineageTracker::default());
        let executor =
            WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone())
                .unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);

        let standard_step = WorkflowStep {
            id: "transform_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let standard_result = StepResult {
            step_id: standard_step.id.clone(),
            success: true,
            output: serde_json::json!({
                "_modifications": [
                    {
                        "field_name": "status",
                        "old_value": "NEW",
                        "new_value": "normalized",
                        "is_reversible": false,
                        "operations": 1
                    }
                ]
            }),
            confidence: 1.0,
            started_at,
            completed_at,
            batch_metadata: None,
            batch_frame: None,
        };

        executor
            .record_step_lineage("exec_lineage", &standard_step, &standard_result)
            .await;

        let ml_step = WorkflowStep {
            id: "ml_step".to_string(),
            step_type: StepType::MlPrediction,
            config: StepConfig::MLPrediction(MLPredictionConfig {
                model_id: "customer-segmentation".to_string(),
                model_version: "2026.03".to_string(),
                features: vec!["email".to_string()],
                feature_mappings: vec![],
                predictions: vec![PredictionSpec {
                    attribute_name: "segment".to_string(),
                    mock_value: "enterprise".to_string(),
                    mock_confidence: 0.92,
                }],
                confidence_threshold: Some(0.8),
                timeout_ms: 500,
                cache_ttl_secs: Some(60),
            }),
            depends_on: vec![],
        };
        let ml_result = StepResult {
            step_id: ml_step.id.clone(),
            success: true,
            output: serde_json::json!({
                "_predictions": [
                    {
                        "attribute_name": "segment",
                        "value": "enterprise",
                        "confidence": 0.92
                    }
                ]
            }),
            confidence: 0.92,
            started_at,
            completed_at,
            batch_metadata: None,
            batch_frame: None,
        };

        executor
            .record_step_lineage("exec_lineage", &ml_step, &ml_result)
            .await;

        let step_records = tracker.step_records.lock().unwrap();
        assert_eq!(step_records.len(), 1);
        assert_eq!(step_records[0].step_id, "transform_step");
        assert_eq!(step_records[0].modifications.len(), 1);
        drop(step_records);

        let ml_records = tracker.ml_records.lock().unwrap();
        assert_eq!(ml_records.len(), 1);
        assert_eq!(ml_records[0].step_id, "ml_step");
        assert_eq!(ml_records[0].predictions.len(), 1);
        assert_eq!(ml_records[0].predictions[0].attribute_name, "segment");
    }

    #[tokio::test]
    async fn test_record_workflow_lifecycle_lineage_routes_tracker_records() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let tracker = Arc::new(TestLineageTracker::default());
        let executor =
            WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone())
                .unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(10);

        executor
            .record_workflow_start_lineage("exec_lifecycle", started_at)
            .await;
        executor
            .record_workflow_completion_lineage("exec_lifecycle", true, completed_at)
            .await;

        let starts = tracker.workflow_starts.lock().unwrap();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].execution_id, "exec_lifecycle");
        assert_eq!(starts[0].started_at, started_at);
        drop(starts);

        let completions = tracker.workflow_completions.lock().unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0], ("exec_lifecycle".to_string(), true));
    }

    #[tokio::test]
    async fn test_initialize_execution_session_records_start_lineage_and_order() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let tracker = Arc::new(TestLineageTracker::default());
        let executor =
            WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone())
                .unwrap();

        let session = executor
            .initialize_execution_session()
            .await
            .expect("execution session should initialize");

        assert!(session.run_state.execution_id.starts_with("exec_"));
        assert_eq!(session.execution_order.len(), 1);
        assert_eq!(session.execution_order[0].id, "gate1");

        let starts = tracker.workflow_starts.lock().unwrap();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].execution_id, session.run_state.execution_id);
        assert_eq!(starts[0].started_at, session.run_state.started_at);
    }

    #[tokio::test]
    async fn test_create_workflow_run_state_builds_prefixed_execution_id() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let before = chrono::Utc::now();
        let run_state = executor.create_workflow_run_state();
        let after = chrono::Utc::now();

        assert!(run_state.execution_id.starts_with("exec_"));
        assert!(run_state.started_at >= before);
        assert!(run_state.started_at <= after);
    }

    #[tokio::test]
    async fn test_compute_execution_order_matches_workflow_dag() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let execution_order = executor
            .compute_execution_order()
            .expect("execution order should resolve from the workflow DAG");

        assert_eq!(execution_order.len(), 1);
        assert_eq!(execution_order[0].id, "gate1");
    }

    #[tokio::test]
    async fn test_initialize_execution_session_without_lineage_tracker_still_builds_session() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let session = executor
            .initialize_execution_session()
            .await
            .expect("execution session should initialize without lineage");

        assert!(session.run_state.execution_id.starts_with("exec_"));
        assert_eq!(session.execution_order.len(), 1);
        assert_eq!(session.execution_order[0].id, "gate1");
    }

    #[test]
    fn test_build_failed_workflow_result_preserves_output_rows_and_error() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let final_output = serde_json::json!({
            "_rows": [
                {"id": 1, "status": "new"},
                {"id": 2, "status": "failed"}
            ],
            "_row_count": 2
        });

        let result = executor.build_failed_workflow_result(
            "exec_fail".to_string(),
            HashMap::new(),
            started_at,
            completed_at,
            0.42,
            "Step 'gate1' failed".to_string(),
            final_output.clone(),
        );

        assert!(!result.success);
        assert_eq!(result.final_decision, FinalDecision::Reject);
        assert_eq!(result.confidence, 0.42);
        assert_eq!(result.error.as_deref(), Some("Step 'gate1' failed"));
        assert_eq!(result.final_output, final_output);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(2));
        assert_eq!(result.output_rows.as_ref().unwrap()[1]["status"], "failed");
    }

    #[test]
    fn test_build_success_workflow_result_preserves_output_rows() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let final_output = serde_json::json!({
            "_rows": [
                {"id": 1, "status": "done"}
            ],
            "_row_count": 1
        });

        let result = executor.build_success_workflow_result(
            "exec_success".to_string(),
            FinalDecision::Accept,
            0.97,
            HashMap::new(),
            started_at,
            completed_at,
            final_output.clone(),
        );

        assert!(result.success);
        assert_eq!(result.final_decision, FinalDecision::Accept);
        assert_eq!(result.confidence, 0.97);
        assert_eq!(result.error, None);
        assert_eq!(result.final_output, final_output);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
        assert_eq!(result.output_rows.as_ref().unwrap()[0]["status"], "done");
    }

    #[test]
    fn test_prepare_step_execution_state_sets_current_row_lineage_step() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "prepare_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let mut context = ExecutionContext::new(serde_json::json!({})).with_row_lineage(
            "exec_lineage".to_string(),
            "job_123".to_string(),
            "tenant_abc".to_string(),
        );

        executor.prepare_step_execution_state(&step, &mut context);

        assert_eq!(
            context
                .row_lineage
                .as_ref()
                .and_then(|lineage| lineage.current_step_id.as_deref()),
            Some("prepare_step")
        );
    }

    #[tokio::test]
    async fn test_compute_final_decision_applies_threshold_and_manual_review_fallback() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step_results = HashMap::from([(
            "gate1".to_string(),
            StepResult {
                step_id: "gate1".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.7}),
                confidence: 0.7,
                started_at: chrono::Utc::now(),
                completed_at: chrono::Utc::now(),
                batch_metadata: None,
                batch_frame: None,
            },
        )]);

        let final_decision = executor.compute_final_decision(&step_results).unwrap();

        assert_eq!(final_decision, FinalDecision::ManualReview);
    }

    #[tokio::test]
    async fn test_compute_final_confidence_averages_step_results() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step_results = HashMap::from([
            (
                "gate1".to_string(),
                StepResult {
                    step_id: "gate1".to_string(),
                    success: true,
                    output: serde_json::json!({"confidence": 0.4}),
                    confidence: 0.4,
                    started_at: chrono::Utc::now(),
                    completed_at: chrono::Utc::now(),
                    batch_metadata: None,
                    batch_frame: None,
                },
            ),
            (
                "gate2".to_string(),
                StepResult {
                    step_id: "gate2".to_string(),
                    success: true,
                    output: serde_json::json!({"confidence": 0.8}),
                    confidence: 0.8,
                    started_at: chrono::Utc::now(),
                    completed_at: chrono::Utc::now(),
                    batch_metadata: None,
                    batch_frame: None,
                },
            ),
        ]);

        let final_confidence = executor.compute_final_confidence(&step_results);

        assert!((final_confidence - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ensure_step_can_start_rejects_cancelled_context() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::CancellationToken;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "cancelled_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let token = CancellationToken::new();
        token.cancel();
        let context = ExecutionContext::new(serde_json::json!({})).with_cancellation_token(token);

        let error = executor
            .ensure_step_can_start(&step, &context)
            .expect_err("cancelled execution should be rejected before step start");

        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn test_mark_step_execution_progress_updates_tracker() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::progress::ProgressTracker;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "progress_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let tracker = Arc::new(ProgressTracker::new(
            "exec_progress".to_string(),
            "workflow_progress".to_string(),
            1,
        ));
        let context =
            ExecutionContext::new(serde_json::json!({})).with_progress_tracker(tracker.clone());

        executor.mark_step_execution_started(&step, &context);
        let started_snapshot = tracker.snapshot();
        assert_eq!(
            started_snapshot
                .current_step
                .as_ref()
                .map(|current| current.step_name.as_str()),
            Some("progress_step")
        );
        assert_eq!(
            started_snapshot
                .current_step
                .as_ref()
                .map(|current| current.step_type.as_str()),
            Some("ConfidenceGate")
        );

        executor.mark_step_execution_completed(&context);
        let completed_snapshot = tracker.snapshot();
        assert_eq!(completed_snapshot.steps_completed, 1);
    }

    #[tokio::test]
    async fn test_execute_step_rejects_cancelled_context_before_dispatch() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::CancellationToken;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "cancelled_execute_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let token = CancellationToken::new();
        token.cancel();
        let context = ExecutionContext::new(serde_json::json!({ "confidence": 0.9 }))
            .with_cancellation_token(token);

        let error = executor
            .execute_step(&step, &context)
            .await
            .expect_err("cancelled execution should fail before step dispatch");

        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn test_execute_step_updates_progress_tracker_for_simple_step() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::progress::ProgressTracker;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "progress_execute_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let tracker = Arc::new(ProgressTracker::new(
            "exec_progress_step".to_string(),
            "workflow_progress_step".to_string(),
            1,
        ));
        let context = ExecutionContext::new(serde_json::json!({ "confidence": 0.9 }))
            .with_progress_tracker(tracker.clone());

        let result = executor
            .execute_step(&step, &context)
            .await
            .expect("simple confidence gate step should execute");

        assert!(result.success);
        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.steps_completed, 1);
        assert_eq!(
            snapshot
                .current_step
                .as_ref()
                .map(|current| current.step_name.as_str()),
            Some("progress_execute_step")
        );
    }

    #[tokio::test]
    async fn test_finalize_step_execution_records_lineage_and_stores_step_result() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let tracker = Arc::new(TestLineageTracker::default());
        let executor =
            WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone())
                .unwrap();

        let step = WorkflowStep {
            id: "finalize_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let frame = BatchFrame::from_json_values(&[serde_json::json!({"id": 1, "status": "ok"})])
            .unwrap()
            .with_metadata(BatchFrameMetadata {
                source_step_id: Some("source_step".to_string()),
                source_kind: Some("dataset".to_string()),
                source_id: Some("dataset_123".to_string()),
            });
        let step_result = StepResult {
            step_id: step.id.clone(),
            success: true,
            output: serde_json::json!({
                "_rows": [{"id": 1, "status": "ok"}],
                "_row_count": 1,
                "_modifications": [
                    {
                        "field_name": "status",
                        "old_value": "new",
                        "new_value": "ok",
                        "is_reversible": false,
                        "operations": 1
                    }
                ]
            }),
            confidence: 0.88,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            batch_metadata: Some(frame.metadata().clone()),
            batch_frame: Some(frame),
        };
        let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));

        executor
            .finalize_step_execution("exec_finalize", &step, &step_result, &mut state)
            .await
            .unwrap();

        let step_records = tracker.step_records.lock().unwrap();
        assert_eq!(step_records.len(), 1);
        assert_eq!(step_records[0].step_id, "finalize_step");
        assert_eq!(step_records[0].modifications.len(), 1);
        drop(step_records);

        let stored = state
            .step_results
            .get("finalize_step")
            .expect("finalized step result should be stored");
        assert_eq!(stored.output["_row_count"], 1);
        assert!(stored.batch_frame.is_none());
        assert_eq!(
            stored
                .batch_metadata
                .as_ref()
                .and_then(|m| m.source_id.as_deref()),
            Some("dataset_123")
        );
        assert_eq!(
            state
                .context
                .get_batch_frame()
                .unwrap()
                .metadata()
                .source_step_id
                .as_deref(),
            Some("source_step")
        );
    }

    #[test]
    fn test_complete_failed_step_execution_builds_reject_result() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let run_state = WorkflowRunState {
            execution_id: "exec_fail".to_string(),
            started_at,
        };
        let step = WorkflowStep {
            id: "failed_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let step_result = StepResult {
            step_id: step.id.clone(),
            success: false,
            output: serde_json::json!({
                "_rows": [{"id": 1, "status": "failed"}],
                "_row_count": 1
            }),
            confidence: 0.25,
            started_at,
            completed_at: started_at,
            batch_metadata: None,
            batch_frame: None,
        };
        let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));
        state
            .step_results
            .insert(step.id.clone(), step_result.clone());
        state.context.working_data = step_result.output.clone();

        let result = executor
            .complete_failed_step_execution(&run_state, &step, &step_result, &state)
            .expect("failed steps should produce a workflow result");

        assert!(!result.success);
        assert_eq!(result.execution_id, run_state.execution_id);
        assert_eq!(result.final_decision, FinalDecision::Reject);
        assert_eq!(result.confidence, 0.25);
        assert_eq!(result.started_at, run_state.started_at);
        assert_eq!(result.error.as_deref(), Some("Step 'failed_step' failed"));
        assert_eq!(result.step_results.len(), 1);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn test_build_failed_workflow_completion_preserves_run_state_and_output_rows() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let run_state = WorkflowRunState {
            execution_id: "exec_failure_build".to_string(),
            started_at,
        };
        let step = WorkflowStep {
            id: "failed_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        };
        let step_result = StepResult {
            step_id: step.id.clone(),
            success: false,
            output: serde_json::json!({
                "_rows": [{"id": 1, "status": "failed"}],
                "_row_count": 1
            }),
            confidence: 0.25,
            started_at,
            completed_at: started_at,
            batch_metadata: None,
            batch_frame: None,
        };
        let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));
        state
            .step_results
            .insert(step.id.clone(), step_result.clone());
        state.context.working_data = step_result.output.clone();

        let result = executor.build_failed_workflow_completion(
            &run_state,
            &step,
            &step_result,
            &state,
            completed_at,
        );

        assert!(!result.success);
        assert_eq!(result.execution_id, run_state.execution_id);
        assert_eq!(result.started_at, run_state.started_at);
        assert_eq!(result.completed_at, completed_at);
        assert_eq!(result.final_decision, FinalDecision::Reject);
        assert_eq!(result.confidence, 0.25);
        assert_eq!(result.error.as_deref(), Some("Step 'failed_step' failed"));
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn test_complete_successful_workflow_execution_records_completion_lineage() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let tracker = Arc::new(TestLineageTracker::default());
        let executor =
            WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone())
                .unwrap();

        let started_at = chrono::Utc::now();
        let run_state = WorkflowRunState {
            execution_id: "exec_success".to_string(),
            started_at,
        };
        let mut step_results = HashMap::new();
        step_results.insert(
            "gate1".to_string(),
            StepResult {
                step_id: "gate1".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.95}),
                confidence: 0.95,
                started_at,
                completed_at: started_at,
                batch_metadata: None,
                batch_frame: None,
            },
        );

        let result = executor
            .complete_successful_workflow_execution(
                &run_state,
                step_results,
                serde_json::json!({
                    "_rows": [{"id": 1, "status": "done"}],
                    "_row_count": 1
                }),
            )
            .await
            .expect("successful execution should finalize cleanly");

        assert!(result.success);
        assert_eq!(result.execution_id, run_state.execution_id);
        assert_eq!(result.final_decision, FinalDecision::Accept);
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.started_at, run_state.started_at);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));

        let completions = tracker.workflow_completions.lock().unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0], (run_state.execution_id, true));
    }

    #[test]
    fn test_build_successful_workflow_completion_preserves_run_state_and_output_rows() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let started_at = chrono::Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(5);
        let run_state = WorkflowRunState {
            execution_id: "exec_success_build".to_string(),
            started_at,
        };
        let mut step_results = HashMap::new();
        step_results.insert(
            "gate1".to_string(),
            StepResult {
                step_id: "gate1".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.95}),
                confidence: 0.95,
                started_at,
                completed_at: started_at,
                batch_metadata: None,
                batch_frame: None,
            },
        );
        let final_output = serde_json::json!({
            "_rows": [{"id": 1, "status": "done"}],
            "_row_count": 1
        });

        let result = executor
            .build_successful_workflow_completion(
                &run_state,
                step_results,
                final_output.clone(),
                completed_at,
            )
            .expect("success completion should build a workflow result");

        assert!(result.success);
        assert_eq!(result.execution_id, run_state.execution_id);
        assert_eq!(result.started_at, run_state.started_at);
        assert_eq!(result.completed_at, completed_at);
        assert_eq!(result.final_decision, FinalDecision::Accept);
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.final_output, final_output);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn test_execute_ordered_steps_returns_completed_state_for_successful_run() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let execution_order = executor.dag.execution_order().unwrap();
        let run_state = WorkflowRunState {
            execution_id: "exec_loop_success".to_string(),
            started_at: chrono::Utc::now(),
        };

        let outcome = executor
            .execute_ordered_steps(
                &run_state,
                execution_order,
                ExecutionContext::new(serde_json::json!({"confidence": 0.9})),
            )
            .await
            .expect("successful ordered execution should complete");

        match outcome {
            ExecuteLoopOutcome::Completed {
                context,
                step_results,
            } => {
                assert_eq!(step_results.len(), 1);
                assert!(step_results.get("gate1").unwrap().success);
                assert_eq!(context.working_data["confidence"], 0.9);
            }
            ExecuteLoopOutcome::Failed(_) => panic!("expected completed ordered execution"),
        }
    }

    #[tokio::test]
    async fn test_execute_ordered_steps_returns_failed_result_for_failing_step() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let execution_order = executor.dag.execution_order().unwrap();
        let run_state = WorkflowRunState {
            execution_id: "exec_loop_failure".to_string(),
            started_at: chrono::Utc::now(),
        };

        let outcome = executor
            .execute_ordered_steps(
                &run_state,
                execution_order,
                ExecutionContext::new(serde_json::json!({"confidence": 0.1})),
            )
            .await
            .expect("failing ordered execution should still produce a workflow result");

        match outcome {
            ExecuteLoopOutcome::Failed(result) => {
                assert!(!result.success);
                assert_eq!(result.execution_id, run_state.execution_id);
                assert_eq!(result.final_decision, FinalDecision::Reject);
                assert_eq!(result.started_at, run_state.started_at);
                assert_eq!(result.error.as_deref(), Some("Step 'gate1' failed"));
                assert_eq!(result.step_results.len(), 1);
            }
            ExecuteLoopOutcome::Completed { .. } => panic!("expected failed ordered execution"),
        }
    }

    #[tokio::test]
    async fn test_execute_ordered_step_updates_state_for_successful_step() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let step = executor.dag.execution_order().unwrap().remove(0);
        let run_state = WorkflowRunState {
            execution_id: "exec_single_success".to_string(),
            started_at: chrono::Utc::now(),
        };
        let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({
            "confidence": 0.9
        })));

        let result = executor
            .execute_ordered_step(&run_state, &step, &mut state)
            .await
            .expect("single ordered step should execute");

        assert!(result.is_none());
        assert_eq!(state.step_results.len(), 1);
        assert!(state.step_results.get("gate1").unwrap().success);
        assert_eq!(state.context.working_data["confidence"], 0.9);
    }

    #[tokio::test]
    async fn test_execute_ordered_step_returns_failed_result_for_rejecting_step() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let step = executor.dag.execution_order().unwrap().remove(0);
        let run_state = WorkflowRunState {
            execution_id: "exec_single_failure".to_string(),
            started_at: chrono::Utc::now(),
        };
        let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({
            "confidence": 0.1
        })));

        let result = executor
            .execute_ordered_step(&run_state, &step, &mut state)
            .await
            .expect("single ordered step should return a workflow result");

        let workflow_result = result.expect("rejecting step should fail workflow execution");
        assert!(!workflow_result.success);
        assert_eq!(workflow_result.execution_id, run_state.execution_id);
        assert_eq!(workflow_result.started_at, run_state.started_at);
        assert_eq!(workflow_result.final_decision, FinalDecision::Reject);
        assert_eq!(state.step_results.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_session_returns_failed_result_with_session_run_state() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let session = WorkflowExecutionSession {
            run_state: WorkflowRunState {
                execution_id: "exec_session_failure".to_string(),
                started_at: chrono::Utc::now(),
            },
            execution_order: executor.dag.execution_order().unwrap(),
        };

        let result = executor
            .execute_session(
                session.clone(),
                ExecutionContext::new(serde_json::json!({"confidence": 0.1})),
            )
            .await
            .expect("session execution should return a workflow result");

        assert!(!result.success);
        assert_eq!(result.execution_id, session.run_state.execution_id);
        assert_eq!(result.started_at, session.run_state.started_at);
        assert_eq!(result.final_decision, FinalDecision::Reject);
    }

    #[tokio::test]
    async fn test_complete_session_outcome_returns_failed_result_unchanged() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let session = WorkflowExecutionSession {
            run_state: WorkflowRunState {
                execution_id: "exec_session_outcome_failure".to_string(),
                started_at: chrono::Utc::now(),
            },
            execution_order: vec![],
        };
        let expected = WorkflowResult {
            success: false,
            execution_id: session.run_state.execution_id.clone(),
            final_decision: FinalDecision::Reject,
            confidence: 0.2,
            step_results: HashMap::new(),
            started_at: session.run_state.started_at,
            completed_at: session.run_state.started_at,
            error: Some("failed".to_string()),
            final_output: serde_json::json!({"status": "failed"}),
            output_rows: Some(vec![serde_json::json!({"id": 1})]),
        };

        let result = executor
            .complete_session_outcome(
                &session.run_state,
                ExecuteLoopOutcome::Failed(expected.clone()),
            )
            .await
            .expect("failed session outcome should return the workflow result unchanged");

        assert_eq!(result.success, expected.success);
        assert_eq!(result.execution_id, expected.execution_id);
        assert_eq!(result.final_decision, expected.final_decision);
        assert_eq!(result.confidence, expected.confidence);
        assert_eq!(result.started_at, expected.started_at);
        assert_eq!(result.error, expected.error);
        assert_eq!(result.final_output, expected.final_output);
        assert_eq!(result.output_rows, expected.output_rows);
    }

    #[tokio::test]
    async fn test_complete_session_outcome_finalizes_completed_result() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let session = WorkflowExecutionSession {
            run_state: WorkflowRunState {
                execution_id: "exec_session_outcome_success".to_string(),
                started_at: chrono::Utc::now(),
            },
            execution_order: vec![],
        };
        let mut step_results = HashMap::new();
        step_results.insert(
            "gate1".to_string(),
            StepResult {
                step_id: "gate1".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.95}),
                confidence: 0.95,
                started_at: session.run_state.started_at,
                completed_at: session.run_state.started_at,
                batch_metadata: None,
                batch_frame: None,
            },
        );

        let result = executor
            .complete_session_outcome(
                &session.run_state,
                ExecuteLoopOutcome::Completed {
                    context: ExecutionContext::new(serde_json::json!({
                        "_rows": [{"id": 1, "status": "done"}],
                        "_row_count": 1
                    })),
                    step_results,
                },
            )
            .await
            .expect("completed session outcome should finalize successfully");

        assert!(result.success);
        assert_eq!(result.execution_id, session.run_state.execution_id);
        assert_eq!(result.started_at, session.run_state.started_at);
        assert_eq!(result.final_decision, FinalDecision::Accept);
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn test_execute_session_returns_success_result_with_session_run_state() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
        let session = WorkflowExecutionSession {
            run_state: WorkflowRunState {
                execution_id: "exec_session_success".to_string(),
                started_at: chrono::Utc::now(),
            },
            execution_order: executor.dag.execution_order().unwrap(),
        };

        let result = executor
            .execute_session(
                session.clone(),
                ExecutionContext::new(serde_json::json!({"confidence": 0.9})),
            )
            .await
            .expect("session execution should complete successfully");

        assert!(result.success);
        assert_eq!(result.execution_id, session.run_state.execution_id);
        assert_eq!(result.started_at, session.run_state.started_at);
        assert_eq!(result.final_decision, FinalDecision::Accept);
    }

    #[test]
    fn test_resolve_feature_reads_nested_step_output_paths() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let mut context = ExecutionContext::new(serde_json::json!({
            "email": "person@example.com"
        }));
        context.step_outputs.insert(
            "score_step".to_string(),
            serde_json::json!({
                "output": {
                    "score": 0.91,
                    "band": "gold"
                }
            }),
        );

        let value = executor
            .resolve_feature("score_step.output.score", &context)
            .expect("nested step output path should resolve");

        assert_eq!(value, serde_json::json!(0.91));
    }

    #[test]
    fn test_extract_features_from_mappings_applies_transform_after_resolution() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::FeatureMapping;

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let context = ExecutionContext::new(serde_json::json!({
            "email": "  USER@EXAMPLE.COM  "
        }));
        let mappings = vec![FeatureMapping {
            feature_name: "normalized_email".to_string(),
            field_name: "email".to_string(),
            transform: Some("trim".to_string()),
        }];

        let features = executor
            .extract_features_from_mappings(&mappings, &context)
            .expect("feature mappings should resolve and transform");

        assert_eq!(
            features.get("normalized_email"),
            Some(&serde_json::json!("USER@EXAMPLE.COM"))
        );
    }

    #[tokio::test]
    async fn test_execute_field_transformer_falls_back_to_legacy_object_transform() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            FieldTransformation, FieldTransformerConfig, TransformOperation,
        };

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "email".to_string(),
                operations: vec![TransformOperation::Trim, TransformOperation::Lower],
            }],
        };
        let context = ExecutionContext::new(serde_json::json!({"email": "  TEST@EXAMPLE.COM  "}));

        let result = executor
            .execute_field_transformer(&config, &context)
            .await
            .expect("legacy object transform path should execute");

        assert!(result.success);
        assert_eq!(result.confidence, 1.0);
        assert!(result.batch_frame.is_none());
        assert_eq!(
            result.output["email"],
            serde_json::json!("test@example.com")
        );
        assert_eq!(result.output["_fields_modified"], serde_json::json!(1));
        assert_eq!(result.output["_modifications"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_execute_step_field_transformer_attaches_batch_frame_sidecar() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            FieldTransformation, FieldTransformerConfig, StepConfig, StepType, TransformOperation,
            WorkflowStep,
        };
        use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "transform_step".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![FieldTransformation {
                    field: "status".to_string(),
                    operations: vec![TransformOperation::Lower],
                }],
            }),
            depends_on: vec![],
        };
        let frame = BatchFrame::from_json_values(&[
            serde_json::json!({"id": 1, "status": "ACTIVE"}),
            serde_json::json!({"id": 2, "status": "PENDING"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_step".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });
        let context = ExecutionContext::from_batch_frame(frame).unwrap();

        let step_result = executor.execute_step(&step, &context).await.unwrap();

        assert!(step_result.success);
        assert_eq!(step_result.output["_rows"][0]["status"], "active");
        let batch_frame = step_result
            .batch_frame
            .expect("field transformer batch path should attach a frame sidecar");
        assert_eq!(
            batch_frame.metadata().source_step_id.as_deref(),
            Some("extract_step")
        );
        assert_eq!(
            batch_frame.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
    }

    #[tokio::test]
    async fn test_execute_step_data_validator_attaches_batch_frame_sidecar() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            DataValidatorConfig, RuleType, Severity, StepConfig, StepType, ValidationRule,
            WorkflowStep,
        };
        use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "validate_step".to_string(),
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
            depends_on: vec![],
        };
        let frame = BatchFrame::from_json_values(&[
            serde_json::json!({"id": 1, "status": "active"}),
            serde_json::json!({"id": 2, "status": "archived"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_step".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });
        let context = ExecutionContext::from_batch_frame(frame).unwrap();

        let step_result = executor.execute_step(&step, &context).await.unwrap();

        assert!(step_result.success);
        assert_eq!(step_result.output["_error_count"], 1);
        let batch_frame = step_result
            .batch_frame
            .expect("data validator batch path should attach a frame sidecar");
        assert_eq!(
            batch_frame.metadata().source_step_id.as_deref(),
            Some("extract_step")
        );
        assert_eq!(
            batch_frame.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
    }

    #[tokio::test]
    async fn test_execute_data_validator_falls_back_to_legacy_output_contract() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            DataValidatorConfig, RuleType, Severity, ValidationRule,
        };

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let context = ExecutionContext::new(serde_json::json!({
            "_rows": [123, 456]
        }));
        let config = DataValidatorConfig {
            rules: vec![ValidationRule {
                field: "name".to_string(),
                rule_type: RuleType::NotNull,
                params: None,
                severity: Severity::Error,
            }],
            fail_on_error: true,
        };

        let result = executor
            .execute_data_validator(&config, &context)
            .await
            .expect("legacy validator path should execute");

        assert!(!result.success);
        assert!(result.batch_frame.is_none());
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.output["_row_count"], serde_json::json!(2));
        assert_eq!(result.output["_error_count"], serde_json::json!(2));
        assert_eq!(result.output["_warning_count"], serde_json::json!(0));
        assert_eq!(result.output["_rows"], serde_json::json!([123, 456]));
    }

    #[tokio::test]
    async fn test_execute_step_aggregator_attaches_batch_frame_sidecar() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            AggFunction, Aggregation, AggregatorConfig, StepConfig, StepType, WorkflowStep,
        };
        use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "aggregate_step".to_string(),
            step_type: StepType::Aggregator,
            config: StepConfig::Aggregator(AggregatorConfig {
                group_by: vec!["region".to_string()],
                aggregations: vec![Aggregation {
                    field: "amount".to_string(),
                    function: AggFunction::Sum,
                    alias: Some("total_amount".to_string()),
                }],
            }),
            depends_on: vec![],
        };
        let frame = BatchFrame::from_json_values(&[
            serde_json::json!({"region": "east", "amount": 10.0}),
            serde_json::json!({"region": "east", "amount": 15.0}),
            serde_json::json!({"region": "west", "amount": 7.0}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_step".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });
        let context = ExecutionContext::from_batch_frame(frame).unwrap();

        let step_result = executor.execute_step(&step, &context).await.unwrap();

        assert!(step_result.success);
        assert_eq!(step_result.output["_row_count"], 2);
        let batch_frame = step_result
            .batch_frame
            .expect("aggregator batch path should attach a frame sidecar");
        assert_eq!(
            batch_frame.metadata().source_step_id.as_deref(),
            Some("extract_step")
        );
        assert_eq!(
            batch_frame.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
    }

    #[tokio::test]
    async fn test_execute_aggregator_falls_back_to_legacy_output_contract() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            AggFunction, Aggregation, AggregatorConfig,
        };

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let context = ExecutionContext::new(serde_json::json!({
            "_rows": [123, 456]
        }));
        let config = AggregatorConfig {
            group_by: vec!["region".to_string()],
            aggregations: vec![Aggregation {
                field: "amount".to_string(),
                function: AggFunction::Sum,
                alias: Some("total_amount".to_string()),
            }],
        };

        let result = executor
            .execute_aggregator(&config, &context)
            .await
            .expect("legacy aggregator path should execute");

        assert!(result.success);
        assert!(result.batch_frame.is_none());
        assert_eq!(result.confidence, 1.0);
        assert_eq!(result.output["_row_count"], serde_json::json!(1));
        assert_eq!(result.output["_original_count"], serde_json::json!(2));
        assert_eq!(result.output["_rows"][0]["region"], "");
        assert_eq!(result.output["_rows"][0]["total_amount"], 0.0);
    }

    #[tokio::test]
    async fn test_execute_step_deduplicator_attaches_batch_frame_sidecar() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            DedupMethod, DeduplicatorConfig, KeepStrategy, StepConfig, StepType, WorkflowStep,
        };
        use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let step = WorkflowStep {
            id: "dedup_step".to_string(),
            step_type: StepType::Deduplicator,
            config: StepConfig::Deduplicator(DeduplicatorConfig {
                method: DedupMethod::Exact,
                key_fields: vec!["id".to_string()],
                threshold: None,
                keep: KeepStrategy::First,
            }),
            depends_on: vec![],
        };
        let frame = BatchFrame::from_json_values(&[
            serde_json::json!({"id": 1, "name": "Alice"}),
            serde_json::json!({"id": 1, "name": "Alice Updated"}),
            serde_json::json!({"id": 2, "name": "Bob"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_step".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });
        let context = ExecutionContext::from_batch_frame(frame).unwrap();

        let step_result = executor.execute_step(&step, &context).await.unwrap();

        assert!(step_result.success);
        assert_eq!(step_result.output["_duplicates_removed"], 1);
        let batch_frame = step_result
            .batch_frame
            .expect("deduplicator batch path should attach a frame sidecar");
        assert_eq!(
            batch_frame.metadata().source_step_id.as_deref(),
            Some("extract_step")
        );
        assert_eq!(
            batch_frame.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
    }

    #[tokio::test]
    async fn test_execute_deduplicator_falls_back_to_legacy_output_contract() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;
        use crate::orchestration::workflow::definition::{
            DedupMethod, DeduplicatorConfig, KeepStrategy,
        };

        let workflow = create_test_workflow();
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());
        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        let context = ExecutionContext::new(serde_json::json!({
            "_rows": [
                {"id": 1, "name": "Alice"},
                {"id": 1, "name": "Alice Updated"},
                {"id": 2, "name": "Bob"}
            ]
        }));
        let config = DeduplicatorConfig {
            method: DedupMethod::Exact,
            key_fields: vec!["id".to_string()],
            threshold: None,
            keep: KeepStrategy::Last,
        };

        let result = executor
            .execute_deduplicator(&config, &context)
            .await
            .expect("legacy deduplicator path should execute");

        assert!(result.success);
        assert!(result.batch_frame.is_none());
        assert_eq!(result.confidence, 1.0);
        assert_eq!(result.output["_row_count"], serde_json::json!(2));
        assert_eq!(result.output["_original_count"], serde_json::json!(3));
        assert_eq!(result.output["_duplicates_removed"], serde_json::json!(1));
        let rows = result.output["_rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| {
            row["id"] == serde_json::json!(1) && row["name"] == serde_json::json!("Alice Updated")
        }));
        assert!(rows.iter().any(
            |row| row["id"] == serde_json::json!(2) && row["name"] == serde_json::json!("Bob")
        ));
    }

    #[tokio::test]
    async fn test_data_flow_between_steps() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        // Create workflow with 2 confidence gates to test data flow
        let workflow = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.5,
                        input_step: None,
                    }),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "step2".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.6,
                        input_step: Some("step1".to_string()),
                    }),
                    depends_on: vec!["step1".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        // Create dependencies
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        // Create context with initial confidence
        let context = ExecutionContext::new(serde_json::json!({
            "confidence": 0.75
        }));

        let result = executor.execute(context).await.unwrap();

        // Verify workflow succeeded
        assert!(result.success);
        assert_eq!(result.step_results.len(), 2);

        // Verify step1 passed (0.75 >= 0.5)
        let step1_result = result.step_results.get("step1").unwrap();
        assert!(step1_result.success);
        assert_eq!(step1_result.confidence, 0.75);

        // Verify step2 received step1's confidence and passed (0.75 >= 0.6)
        let step2_result = result.step_results.get("step2").unwrap();
        assert!(step2_result.success);
        assert_eq!(step2_result.confidence, 0.75);
    }

    #[tokio::test]
    async fn test_working_data_propagation() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
        use crate::orchestration::rules::RuleExecutor;

        // Create workflow with 2 confidence gates
        let workflow = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "gate1".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.5,
                        input_step: None,
                    }),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "gate2".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.7,
                        input_step: Some("gate1".to_string()),
                    }),
                    depends_on: vec!["gate1".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        // Create dependencies
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

        // Start with confidence in input_data
        let context = ExecutionContext::new(serde_json::json!({
            "confidence": 0.85,
            "entity_id": "test_123"
        }));

        let result = executor.execute(context).await.unwrap();

        // Both steps should succeed
        assert!(result.success);

        // gate1 should have passed with 0.85
        let gate1 = result.step_results.get("gate1").unwrap();
        assert!(gate1.success);
        assert_eq!(gate1.confidence, 0.85);

        // gate2 should have received gate1's confidence (0.85) and passed (>= 0.7)
        let gate2 = result.step_results.get("gate2").unwrap();
        assert!(gate2.success);
        assert_eq!(gate2.confidence, 0.85);

        // Verify working_data propagation by checking outputs contain expected fields
        assert!(gate1.output.get("confidence").is_some());
        assert!(gate2.output.get("confidence").is_some());
    }
}
