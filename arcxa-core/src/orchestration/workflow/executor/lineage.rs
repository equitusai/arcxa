use super::super::lineage_tracker::{MLPredictionStepRecord, StepExecutionRecord};
use super::{StepResult, StepType, WorkflowExecutor, WorkflowStep};

impl WorkflowExecutor {
    pub(super) async fn record_step_lineage(
        &self,
        execution_id: &str,
        step: &WorkflowStep,
        step_result: &StepResult,
    ) {
        let Some(tracker) = &self.lineage_tracker else {
            tracing::error!(
                "✗ LINEAGE_TRACKER: NOT PRESENT for step '{}' - lineage will not be recorded!",
                step.id
            );
            return;
        };

        tracing::info!(
            "✓ LINEAGE_TRACKER: Present for step '{}' (type: {})",
            step.id,
            step.step_type
        );

        if let Some(record) = self.build_ml_prediction_step_record(execution_id, step, step_result)
        {
            tracker.record_ml_predictions(record).await.ok();
            return;
        }

        tracing::info!(
            "✓ LINEAGE_EXTRACT: Extracting modifications for step '{}'",
            step.id
        );
        let record = self.build_step_execution_record(execution_id, step, step_result);
        tracing::warn!(
            "✓ LINEAGE_EXTRACT: Extracted {} modifications for step '{}', calling record_step_execution",
            record.modifications.len(),
            step.id
        );

        match tracker.record_step_execution(record).await {
            Ok(_) => tracing::warn!(
                "✓ LINEAGE_RECORD: Successfully recorded lineage for step '{}'",
                step.id
            ),
            Err(e) => tracing::error!(
                "✗ LINEAGE_RECORD: Failed to record lineage for step '{}': {}",
                step.id,
                e
            ),
        }
    }

    pub(super) fn build_step_execution_record(
        &self,
        execution_id: &str,
        step: &WorkflowStep,
        step_result: &StepResult,
    ) -> StepExecutionRecord {
        StepExecutionRecord {
            execution_id: execution_id.to_string(),
            step_id: step.id.clone(),
            step_type: step.step_type.to_string(),
            modifications: self.extract_modifications(&step_result.output),
            started_at: step_result.started_at,
            completed_at: step_result.completed_at,
        }
    }

    pub(super) fn build_ml_prediction_step_record(
        &self,
        execution_id: &str,
        step: &WorkflowStep,
        step_result: &StepResult,
    ) -> Option<MLPredictionStepRecord> {
        if step.step_type != StepType::MlPrediction {
            return None;
        }

        let predictions = self.extract_predictions(&step_result.output, &step.config)?;

        Some(MLPredictionStepRecord {
            execution_id: execution_id.to_string(),
            step_id: step.id.clone(),
            model_id: predictions.model_id,
            model_version: predictions.model_version,
            predictions: predictions.predictions,
            started_at: step_result.started_at,
            completed_at: step_result.completed_at,
        })
    }
}
