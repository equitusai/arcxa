//! # Workflow Engine
//!
//! Provides DAG-based workflow execution for orchestrating complex multi-step
//! governance operations.
//!
//! ## Architecture
//!
//! This module follows a clear separation of concerns:
//!
//! - **Definitions** (`definition.rs`): Workflow data structures (YAML/JSON parsing, validation)
//! - **Traits** (`traits.rs`): Core abstractions for execution (implemented by graphica-coordinator)
//! - **Executor** (`executor.rs`): Basic execution for testing/mocking
//! - **DAG** (`dag.rs`): Dependency resolution and execution ordering
//!
//! ### graphica-core vs graphica-coordinator
//!
//! - **graphica-core** (this crate): Workflow definitions and trait contracts (lightweight)
//! - **graphica-coordinator**: Production runtime with streaming execution (Timely, RocksDB, Kafka)
//!
//! ## Example
//!
//! ```yaml
//! id: address_merge_v1
//! name: "ML + Heuristic Address Fusion"
//! steps:
//!   - id: ml_similarity
//!     type: ml_prediction
//!     config:
//!       model_id: address_bert_v2
//!       features: [street, city, zip]
//!   - id: confidence_check
//!     type: confidence_gate
//!     config:
//!       threshold: 0.85
//! ```

pub mod cancellation;
pub mod config;
pub mod dag;
pub mod definition;
pub mod engine;
pub mod error;
pub mod execution_context_v2;
pub mod executor;
pub mod executor_optimized;
pub(crate) mod field_transformer;
pub mod input;
pub mod lineage_tracker;
pub mod memory_monitor;
pub mod progress;
pub mod row_lineage_context;
pub mod row_storage;
pub mod runtime;
#[cfg(feature = "workflow-storage")]
pub mod streaming_deduplicator;
pub mod traits;

pub use definition::{
    AggFunction,

    Aggregation,
    AggregatorConfig,
    ConfidenceGateConfig,
    // ETL configs (Phase 4 - Export)
    CsvExporterConfig,
    // ETL configs (Phase 1 - Data Movement)
    CsvSourceConfig,
    // ETL configs (Phase 3 - Advanced Features)
    DataJoinerConfig,
    // ETL configs (Phase 2 - Data Quality)
    DataValidatorConfig,
    DbExtractConfig,
    DbLoaderConfig,
    DedupMethod,
    DeduplicatorConfig,
    FieldTransformation,
    FieldTransformerConfig,
    FuzzyAlgorithm,
    HeuristicConfig,
    JoinType,
    KeepStrategy,
    LoadMode,
    // ML/Fusion configs
    MLPredictionConfig,
    MappingMode,
    RdfLoaderConfig,

    RuleType,
    SemanticMapperConfig,
    Severity,
    SosValidationConfig,
    SosValidationSpec,

    StepConfig,
    StepType,

    TransformOperation,
    ValidationRule,
    WeightedVoteConfig,

    // Workflow core types
    WorkflowDefinition,
    WorkflowStep,
};

// Core traits for workflow execution (implemented by graphica-coordinator)
pub use traits::{
    ExecutionState, LineageCapture, LineageStats, RuntimeHealth, StateBackend, StepExecutor,
    WorkflowRuntime, WorkflowValidator,
};

pub use cancellation::CancellationToken;
pub use config::{ExecutionTimeout, ExecutionTimeoutBuilder, RetryPolicy};
pub use dag::DagExecutor;
pub use engine::{WorkflowEngine, WorkflowMetadata, WorkflowPersistenceEvent};
pub use error::WorkflowErrorCategory;
pub use execution_context_v2::{ExecutionContextV2, ResourceLimits as ResourceLimitsV2};
pub use executor::{
    ExecutionContext, FinalDecision, ResourceLimits, SosValidationCallback, SosValidationCheck,
    SosValidationStepResult, StepResult, TransformerCallback, WorkflowExecutor, WorkflowResult,
};
#[cfg(feature = "workflow-storage")]
pub use executor_optimized::OptimizedStepExecutor;
pub(crate) use field_transformer::RowTransformationStats;
pub use input::{
    DataSourceInputAdapter, DatasetInputAdapter, DatasetResolver, EntityFilterAdapter,
    ExecutionMode, InputAdapter, JsonInputAdapter, QueryExecutor, SparqlInputAdapter,
    WorkflowInput,
};
pub use lineage_tracker::{
    FieldModificationRecord, LineageTracker, MLPredictionStepRecord, PredictionRecord,
    RowTransformationEvent, StepExecutionRecord, TransformationType, WorkflowExecutionRecord,
};
pub use memory_monitor::{MemoryConfig, MemoryMonitor};
pub use progress::{ExecutionStatus, ProgressTracker, StepProgress, WorkflowProgress};
pub use row_lineage_context::{RowLineageContext, RowTransformationRecord};
#[cfg(feature = "workflow-storage")]
pub use row_storage::StorageManager;
pub use row_storage::{RowAccessor, RowStorage, StorageType};
pub use runtime::frame::{BatchFrame, BatchFrameMetadata};
pub use runtime::lineage::RuntimeLineageMode;
pub use runtime::metrics::RuntimeStepMetrics;
pub use runtime::spill::{
    SpillBackend, SpillDecision, SpillPolicy, SpillThresholds, StorageTieringPlan,
    StorageTieringPolicy, StorageTieringThresholds,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Workflow instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub definition: WorkflowDefinition,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Workflow {
    pub fn new(id: String, name: String, definition: WorkflowDefinition) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            name,
            definition,
            version: "1.0.0".to_string(),
            created_at: now,
            updated_at: now,
        }
    }
}
