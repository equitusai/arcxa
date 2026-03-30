//! Coordinator LineageTracker Implementation
//!
//! Implements the LineageTracker trait to generate RDF triples for workflow execution lineage.
//! Bridges the workflow executor (graphica-core) to the RDF store (graphica-coordinator).

use crate::storage::row_lineage_store::RowLineageStore;
use crate::storage::LineageStorage;
use crate::workflows::lineage::rdf::{
    FieldModification, ModelPrediction, WorkflowLineageGenerator,
};
use anyhow::Result;
use graphica_core::core::lineage::row_level::{RowId, RowJourney, RowLineageEvent};
use graphica_core::core::lineage::{DataRef, LineageEvent, ModelMetrics, ModelRef, TransformRef};
use graphica_core::orchestration::workflow::{
    FieldModificationRecord, LineageTracker, MLPredictionStepRecord, PredictionRecord,
    RowTransformationEvent, StepExecutionRecord, WorkflowExecutionRecord,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Coordinator implementation of LineageTracker
///
/// Converts workflow execution events into RDF triples via WorkflowLineageGenerator.
/// Also handles row-level lineage tracking via RowLineageStore.
pub struct CoordinatorLineageTracker {
    lineage_generator: Arc<WorkflowLineageGenerator>,
    row_lineage_store: Option<Arc<RowLineageStore>>,
    lineage_storage: Option<Arc<LineageStorage>>,
}

impl CoordinatorLineageTracker {
    /// Create a new lineage tracker
    pub fn new(lineage_generator: Arc<WorkflowLineageGenerator>) -> Self {
        Self {
            lineage_generator,
            row_lineage_store: None,
            lineage_storage: None,
        }
    }

    /// Create a lineage tracker with row-level lineage support
    pub fn with_row_lineage_store(
        lineage_generator: Arc<WorkflowLineageGenerator>,
        row_lineage_store: Arc<RowLineageStore>,
    ) -> Self {
        Self {
            lineage_generator,
            row_lineage_store: Some(row_lineage_store),
            lineage_storage: None,
        }
    }

    /// Attach durable lineage storage so workflow run lineage is queryable outside RDF.
    pub fn with_lineage_storage(mut self, lineage_storage: Arc<LineageStorage>) -> Self {
        self.lineage_storage = Some(lineage_storage);
        self
    }

    /// Convert FieldModificationRecord to FieldModification
    fn convert_modification(&self, record: &FieldModificationRecord) -> FieldModification {
        FieldModification {
            field_name: record.field_name.clone(),
            old_value: record.old_value.clone(),
            new_value: record.new_value.clone(),
            confidence: if record.is_reversible { 1.0 } else { 0.95 },
            is_reversible: record.is_reversible,
        }
    }

    fn workflow_definition_ref(
        &self,
        workflow_id: &str,
        extracted_at: chrono::DateTime<chrono::Utc>,
    ) -> DataRef {
        DataRef {
            system: "workflow_registry".to_string(),
            path: format!("workflows/{}", workflow_id),
            version: None,
            extracted_at,
            cdc_position: None,
        }
    }

    fn execution_state_ref(
        &self,
        execution_id: &str,
        state: &str,
        extracted_at: chrono::DateTime<chrono::Utc>,
    ) -> DataRef {
        DataRef {
            system: "workflow_execution".to_string(),
            path: format!("executions/{}/{}", execution_id, state),
            version: None,
            extracted_at,
            cdc_position: None,
        }
    }

    fn step_state_ref(
        &self,
        execution_id: &str,
        step_id: &str,
        extracted_at: chrono::DateTime<chrono::Utc>,
    ) -> DataRef {
        DataRef {
            system: "workflow_step".to_string(),
            path: format!("executions/{}/steps/{}", execution_id, step_id),
            version: None,
            extracted_at,
            cdc_position: None,
        }
    }

    fn build_step_transform(&self, record: &StepExecutionRecord) -> TransformRef {
        TransformRef {
            id: Uuid::new_v4(),
            transform_type: record.step_type.clone(),
            rule_id: record.step_id.clone(),
            version: "workflow_step_v1".to_string(),
            parameters: HashMap::from([
                (
                    "modification_count".to_string(),
                    json!(record.modifications.len()),
                ),
                ("step_id".to_string(), json!(record.step_id.clone())),
                ("step_type".to_string(), json!(record.step_type.clone())),
            ]),
            applied_at: record.completed_at,
            fields_modified: record
                .modifications
                .iter()
                .map(|modification| modification.field_name.clone())
                .collect(),
        }
    }

    async fn write_durable_event(&self, event: LineageEvent) -> Result<()> {
        if let Some(storage) = &self.lineage_storage {
            storage.write_all(event).await?;
        }
        Ok(())
    }

    fn build_workflow_start_event(&self, record: &WorkflowExecutionRecord) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "workflow_execution".to_string(),
            record_id: format!("workflow_execution:{}:start", record.execution_id),
            source_refs: vec![self.workflow_definition_ref(&record.workflow_id, record.started_at)],
            transforms: Vec::new(),
            model_refs: Vec::new(),
            output_ref: self.execution_state_ref(
                &record.execution_id,
                "started",
                record.started_at,
            ),
            ts: record.started_at,
            run_id: record.execution_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::from([
                ("event_kind".to_string(), "workflow_started".to_string()),
                ("workflow_id".to_string(), record.workflow_id.clone()),
                ("execution_id".to_string(), record.execution_id.clone()),
                ("status".to_string(), "running".to_string()),
            ]),
        }
    }

    fn build_step_execution_event(&self, record: &StepExecutionRecord) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "workflow_step_execution".to_string(),
            record_id: format!(
                "workflow_execution:{}:step:{}",
                record.execution_id, record.step_id
            ),
            source_refs: vec![self.execution_state_ref(
                &record.execution_id,
                "started",
                record.started_at,
            )],
            transforms: vec![self.build_step_transform(record)],
            model_refs: Vec::new(),
            output_ref: self.step_state_ref(
                &record.execution_id,
                &record.step_id,
                record.completed_at,
            ),
            ts: record.completed_at,
            run_id: record.execution_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::from([
                ("event_kind".to_string(), "step_completed".to_string()),
                ("execution_id".to_string(), record.execution_id.clone()),
                ("step_id".to_string(), record.step_id.clone()),
                ("step_type".to_string(), record.step_type.clone()),
                (
                    "modification_count".to_string(),
                    record.modifications.len().to_string(),
                ),
            ]),
        }
    }

    fn build_ml_prediction_event(&self, record: &MLPredictionStepRecord) -> LineageEvent {
        let average_confidence = if record.predictions.is_empty() {
            None
        } else {
            Some(
                record
                    .predictions
                    .iter()
                    .map(|prediction| prediction.confidence)
                    .sum::<f64>()
                    / record.predictions.len() as f64,
            )
        };

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "workflow_ml_prediction".to_string(),
            record_id: format!(
                "workflow_execution:{}:step:{}:predictions",
                record.execution_id, record.step_id
            ),
            source_refs: vec![self.step_state_ref(
                &record.execution_id,
                &record.step_id,
                record.started_at,
            )],
            transforms: Vec::new(),
            model_refs: vec![ModelRef {
                model_id: record.model_id.clone(),
                version: record.model_version.clone(),
                model_type: "workflow_prediction".to_string(),
                params_hash: String::new(),
                training_data: Vec::new(),
                metrics: ModelMetrics {
                    accuracy: None,
                    precision: None,
                    recall: None,
                    f1_score: None,
                    rmse: None,
                    custom_metrics: average_confidence
                        .map(|value| HashMap::from([("average_confidence".to_string(), value)]))
                        .unwrap_or_default(),
                },
                registry_uri: String::new(),
                inference_at: record.completed_at,
                features_used: Vec::new(),
                outputs: record
                    .predictions
                    .iter()
                    .map(|prediction| prediction.attribute_name.clone())
                    .collect(),
            }],
            output_ref: self.step_state_ref(
                &record.execution_id,
                &format!("{}:predictions", record.step_id),
                record.completed_at,
            ),
            ts: record.completed_at,
            run_id: record.execution_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::from([
                (
                    "event_kind".to_string(),
                    "ml_predictions_recorded".to_string(),
                ),
                ("execution_id".to_string(), record.execution_id.clone()),
                ("step_id".to_string(), record.step_id.clone()),
                ("model_id".to_string(), record.model_id.clone()),
                (
                    "prediction_count".to_string(),
                    record.predictions.len().to_string(),
                ),
            ]),
        }
    }

    fn build_workflow_completion_event(
        &self,
        execution_id: &str,
        success: bool,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "workflow_execution".to_string(),
            record_id: format!("workflow_execution:{}:complete", execution_id),
            source_refs: vec![self.execution_state_ref(execution_id, "started", completed_at)],
            transforms: Vec::new(),
            model_refs: Vec::new(),
            output_ref: self.execution_state_ref(
                execution_id,
                if success { "completed" } else { "failed" },
                completed_at,
            ),
            ts: completed_at,
            run_id: execution_id.to_string(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::from([
                ("event_kind".to_string(), "workflow_completed".to_string()),
                ("execution_id".to_string(), execution_id.to_string()),
                ("success".to_string(), success.to_string()),
                (
                    "status".to_string(),
                    if success { "completed" } else { "failed" }.to_string(),
                ),
            ]),
        }
    }
}

#[async_trait::async_trait]
impl LineageTracker for CoordinatorLineageTracker {
    /// Record the start of a workflow execution
    async fn record_workflow_start(&self, record: WorkflowExecutionRecord) -> Result<()> {
        tracing::debug!(
            "Recording workflow start: execution_id={}, workflow_id={}",
            record.execution_id,
            record.workflow_id
        );

        // Generate RDF triples for workflow start and persist a durable run-lineage event.
        let rdf_result = self.lineage_generator.record_workflow_start(
            &record.execution_id,
            &record.workflow_id,
            record.started_at,
        );
        let durable_result = self
            .write_durable_event(self.build_workflow_start_event(&record))
            .await;

        rdf_result.and(durable_result)
    }

    /// Record a completed step execution with field modifications
    async fn record_step_execution(&self, record: StepExecutionRecord) -> Result<()> {
        tracing::warn!(
            "✓ COORDINATOR_LINEAGE: record_step_execution called - execution_id={}, step_id={}, step_type={}, modifications={}",
            record.execution_id,
            record.step_id,
            record.step_type,
            record.modifications.len()
        );

        // Convert modifications
        let modifications: Vec<FieldModification> = record
            .modifications
            .iter()
            .map(|m| self.convert_modification(m))
            .collect();

        tracing::warn!(
            "✓ COORDINATOR_LINEAGE: Converted {} modifications, calling lineage_generator.record_step_execution",
            modifications.len()
        );

        // Generate RDF triples for step execution and persist a durable run-lineage event.
        let result = self.lineage_generator.record_step_execution(
            &record.execution_id,
            &record.step_id,
            &record.step_type,
            modifications,
            record.started_at,
            record.completed_at,
        );
        let durable_result = self
            .write_durable_event(self.build_step_execution_event(&record))
            .await;

        match &result {
            Ok(_) => tracing::warn!(
                "✓ RDF_LINEAGE: Successfully generated RDF for step '{}'",
                record.step_id
            ),
            Err(e) => tracing::error!(
                "✗ RDF_LINEAGE: Failed to generate RDF for step '{}': {}",
                record.step_id,
                e
            ),
        }

        result.and(durable_result)
    }

    /// Record ML predictions from workflow step
    async fn record_ml_predictions(&self, record: MLPredictionStepRecord) -> Result<()> {
        tracing::debug!(
            "Recording ML predictions: execution_id={}, model_id={}, count={}",
            record.execution_id,
            record.model_id,
            record.predictions.len()
        );

        // Convert predictions to ModelPrediction format
        let predictions: Vec<ModelPrediction> = record
            .predictions
            .iter()
            .map(|p| ModelPrediction {
                attribute_id: String::new(), // Will be generated by RDF generator
                attribute_name: p.attribute_name.clone(),
                value: p.value.clone(),
                confidence: p.confidence,
                model_id: record.model_id.clone(),
                model_version: record.model_version.clone(),
            })
            .collect();

        // Generate RDF triples for predictions and persist a durable run-lineage event.
        let rdf_result = self.lineage_generator.record_ml_predictions(
            &record.execution_id,
            &record.step_id,
            &record.model_id,
            &record.model_version,
            predictions,
            record.started_at,
            record.completed_at,
        );
        let durable_result = self
            .write_durable_event(self.build_ml_prediction_event(&record))
            .await;

        rdf_result.and(durable_result)
    }

    /// Record the completion of a workflow execution
    async fn record_workflow_complete(
        &self,
        execution_id: String,
        success: bool,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        tracing::debug!(
            "Recording workflow completion: execution_id={}, success={}",
            execution_id,
            success
        );

        // Generate RDF triples for workflow completion and persist a durable run-lineage event.
        let rdf_result =
            self.lineage_generator
                .record_workflow_complete(&execution_id, success, completed_at);
        let durable_result = self
            .write_durable_event(self.build_workflow_completion_event(
                &execution_id,
                success,
                completed_at,
            ))
            .await;

        rdf_result.and(durable_result)
    }

    // Row-Level Lineage Tracking Methods

    /// Record row-level lineage events for ETL steps
    async fn record_row_lineage_batch(&self, events: Vec<RowLineageEvent>) -> Result<()> {
        use graphica_core::core::lineage::row_level::RowLevelLineageSink;

        if events.is_empty() {
            tracing::info!("record_row_lineage_batch called with 0 events, skipping");
            return Ok(());
        }

        tracing::info!("===> RECORDING {} row lineage events", events.len());

        if let Some(store) = &self.row_lineage_store {
            tracing::info!("Writing to row lineage store...");
            store.write_rows_batch(events).await?;
            tracing::info!("Row lineage events written successfully");
        } else {
            tracing::warn!("Row lineage store not configured, events will not be persisted");
        }

        Ok(())
    }

    /// Record row transformation (e.g., deduplication, merge)
    async fn record_row_transformation(
        &self,
        transformation: RowTransformationEvent,
    ) -> Result<()> {
        tracing::debug!(
            "Recording row transformation: step_id={}, type={:?}",
            transformation.step_id,
            transformation.transformation_type
        );

        // For now, we log transformations but don't persist them separately
        // Future: Store in a transforms column family for auditing
        if let Some(_store) = &self.row_lineage_store {
            // Transformation events could be stored separately for journey reconstruction
            // For now, the individual row lineage events track the provenance
            tracing::debug!(
                "Transformation recorded: {} source rows -> {:?} output",
                transformation.source_rows.len(),
                transformation.output_row.as_ref().map(|r| r.to_key())
            );
        }

        Ok(())
    }

    /// Query row journey for debugging and auditing
    async fn get_row_journey(&self, row_id: &RowId) -> Result<Option<RowJourney>> {
        use graphica_core::core::lineage::row_level::RowLevelLineageSink;

        if let Some(store) = &self.row_lineage_store {
            let journey = store.trace_row_journey(row_id).await?;
            Ok(Some(journey))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::GraphicaRdfStore;
    use serde_json::json;

    #[tokio::test]
    async fn test_lineage_tracker_creation() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let generator = Arc::new(WorkflowLineageGenerator::new(Arc::new(store)));
        let tracker = CoordinatorLineageTracker::new(generator);

        // Just verify it constructs
        assert!(true);
    }

    #[tokio::test]
    async fn test_modification_conversion() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let generator = Arc::new(WorkflowLineageGenerator::new(Arc::new(store)));
        let tracker = CoordinatorLineageTracker::new(generator);

        let record = FieldModificationRecord {
            field_name: "email".to_string(),
            old_value: serde_json::json!("OLD@EXAMPLE.COM"),
            new_value: serde_json::json!("old@example.com"),
            is_reversible: false,
            operation_count: 2,
        };

        let modification = tracker.convert_modification(&record);

        assert_eq!(modification.field_name, "email");
        assert_eq!(modification.old_value, serde_json::json!("OLD@EXAMPLE.COM"));
        assert_eq!(modification.new_value, serde_json::json!("old@example.com"));
        assert_eq!(modification.is_reversible, false);
        assert_eq!(modification.confidence, 0.95); // Irreversible = 0.95 confidence
    }

    #[tokio::test]
    async fn test_tracker_builds_native_run_lineage_events_for_workflow_execution() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let generator = Arc::new(WorkflowLineageGenerator::new(Arc::new(store)));
        let tracker = CoordinatorLineageTracker::new(generator);

        let started_at = chrono::Utc::now();
        let start_event = tracker.build_workflow_start_event(&WorkflowExecutionRecord {
            execution_id: "exec_native_lineage".to_string(),
            workflow_id: "wf_customer_sync".to_string(),
            started_at,
        });
        let step_event = tracker.build_step_execution_event(&StepExecutionRecord {
            execution_id: "exec_native_lineage".to_string(),
            step_id: "transform_customers".to_string(),
            step_type: "field_transformer".to_string(),
            modifications: vec![FieldModificationRecord {
                field_name: "email".to_string(),
                old_value: json!("USER@EXAMPLE.COM"),
                new_value: json!("user@example.com"),
                is_reversible: false,
                operation_count: 1,
            }],
            started_at,
            completed_at: started_at,
        });
        let completion_event =
            tracker.build_workflow_completion_event("exec_native_lineage", true, started_at);

        assert_eq!(start_event.run_id, "exec_native_lineage");
        assert_eq!(start_event.dataset, "workflow_execution");
        assert_eq!(
            start_event.metadata.get("workflow_id"),
            Some(&"wf_customer_sync".to_string())
        );

        assert_eq!(step_event.run_id, "exec_native_lineage");
        assert_eq!(step_event.dataset, "workflow_step_execution");
        assert_eq!(step_event.transforms.len(), 1);
        assert_eq!(
            step_event.transforms[0].fields_modified,
            vec!["email".to_string()]
        );

        assert_eq!(completion_event.run_id, "exec_native_lineage");
        assert_eq!(
            completion_event.metadata.get("status"),
            Some(&"completed".to_string())
        );
    }
}
