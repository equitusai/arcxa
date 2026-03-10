//! Workflow Lineage Tracking Trait
//!
//! Defines the interface for tracking workflow execution lineage.
//! Implemented by the coordinator's WorkflowLineageGenerator to generate RDF triples.

use crate::core::lineage::row_level::{RowId, RowLineageEvent};
use anyhow::Result;
use serde_json::Value as JsonValue;

/// Field modification metadata for lineage tracking
#[derive(Debug, Clone)]
pub struct FieldModificationRecord {
    pub field_name: String,
    pub old_value: JsonValue,
    pub new_value: JsonValue,
    pub is_reversible: bool,
    pub operation_count: usize,
}

/// Workflow execution context for lineage
#[derive(Debug, Clone)]
pub struct WorkflowExecutionRecord {
    pub execution_id: String,
    pub workflow_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// Step execution record for lineage
#[derive(Debug, Clone)]
pub struct StepExecutionRecord {
    pub execution_id: String,
    pub step_id: String,
    pub step_type: String,
    pub modifications: Vec<FieldModificationRecord>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// ML prediction record for lineage tracking
#[derive(Debug, Clone)]
pub struct PredictionRecord {
    pub attribute_name: String,
    pub value: JsonValue,
    pub confidence: f64,
}

/// ML prediction step record
#[derive(Debug, Clone)]
pub struct MLPredictionStepRecord {
    pub execution_id: String,
    pub step_id: String,
    pub model_id: String,
    pub model_version: String,
    pub predictions: Vec<PredictionRecord>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Row transformation event for tracking deduplication, merges, etc.
#[derive(Debug, Clone)]
pub struct RowTransformationEvent {
    pub execution_id: String,
    pub step_id: String,
    pub step_type: String,
    pub source_rows: Vec<RowId>,
    pub output_row: Option<RowId>,
    pub transformation_type: TransformationType,
    pub metadata: serde_json::Map<String, JsonValue>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Type of row transformation
#[derive(Debug, Clone)]
pub enum TransformationType {
    /// Rows were deduplicated
    Deduplication {
        kept_row: RowId,
        removed_rows: Vec<RowId>,
        strategy: String, // "first", "last", "merge"
    },
    /// Row was filtered out
    Filtered { reason: String, rule_id: String },
    /// Rows were merged
    Merge {
        source_rows: Vec<RowId>,
        merge_strategy: String,
    },
    /// Row was transformed
    Transform {
        transform_type: String,
        fields_modified: Vec<String>,
    },
}

/// Trait for tracking workflow lineage
///
/// This trait abstracts lineage generation so that graphica-core (workflow engine)
/// doesn't need to depend on graphica-coordinator (RDF store).
///
/// The coordinator implements this trait with WorkflowLineageGenerator to generate
/// RDF triples for field-level provenance tracking.
#[async_trait::async_trait]
pub trait LineageTracker: Send + Sync {
    /// Record the start of a workflow execution
    async fn record_workflow_start(&self, record: WorkflowExecutionRecord) -> Result<()>;

    /// Record a completed step execution with field modifications
    async fn record_step_execution(&self, record: StepExecutionRecord) -> Result<()>;

    /// Record ML predictions from a workflow step
    async fn record_ml_predictions(&self, record: MLPredictionStepRecord) -> Result<()>;

    /// Record the completion of a workflow execution
    async fn record_workflow_complete(
        &self,
        execution_id: String,
        success: bool,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()>;

    // Row-Level Lineage Tracking Methods

    /// Record row-level lineage events for ETL steps
    async fn record_row_lineage_batch(&self, _events: Vec<RowLineageEvent>) -> Result<()> {
        // Default implementation does nothing - allows backward compatibility
        Ok(())
    }

    /// Record a single row lineage event
    async fn record_row_lineage(&self, event: RowLineageEvent) -> Result<()> {
        // Default implementation delegates to batch method
        self.record_row_lineage_batch(vec![event]).await
    }

    /// Record row transformation (e.g., deduplication, merge)
    async fn record_row_transformation(
        &self,
        _transformation: RowTransformationEvent,
    ) -> Result<()> {
        // Default implementation does nothing
        Ok(())
    }

    /// Query row journey for debugging and auditing
    async fn get_row_journey(
        &self,
        _row_id: &crate::core::lineage::row_level::RowId,
    ) -> Result<Option<crate::core::lineage::row_level::RowJourney>> {
        // Default implementation returns None
        Ok(None)
    }
}
