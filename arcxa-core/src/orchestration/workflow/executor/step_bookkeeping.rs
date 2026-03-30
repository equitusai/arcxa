use anyhow::Result;

use super::{
    BatchFrame, BatchFrameMetadata, BatchStepExecutionResult, ExecutionContext, StepResult,
    StepType, WorkflowExecutor, WorkflowStep,
};
use crate::orchestration::workflow::runtime::metrics::RuntimeStepMetrics;

impl WorkflowExecutor {
    pub(super) fn prepare_step_execution_state(
        &self,
        step: &WorkflowStep,
        context: &mut ExecutionContext,
    ) {
        if let Some(ref mut row_lineage) = context.row_lineage {
            row_lineage.set_current_step(step.id.clone());
        }
    }

    pub(super) fn ensure_step_can_start(
        &self,
        step: &WorkflowStep,
        context: &ExecutionContext,
    ) -> Result<()> {
        if let Some(ref token) = context.cancellation_token {
            if token.is_cancelled() {
                tracing::warn!("Workflow execution cancelled before step '{}'", step.id);
                anyhow::bail!("Workflow execution cancelled");
            }
        }

        Ok(())
    }

    pub(super) fn mark_step_execution_started(
        &self,
        step: &WorkflowStep,
        context: &ExecutionContext,
    ) {
        if let Some(ref tracker) = context.progress_tracker {
            tracker.set_current_step(step.id.clone(), format!("{:?}", step.step_type));
        }
    }

    pub(super) fn mark_step_execution_completed(&self, context: &ExecutionContext) {
        if let Some(ref tracker) = context.progress_tracker {
            tracker.complete_step();
        }
    }

    pub(super) fn finalize_step_result(
        &self,
        step: &WorkflowStep,
        started_at: chrono::DateTime<chrono::Utc>,
        completed_at: chrono::DateTime<chrono::Utc>,
        mut batch_result: BatchStepExecutionResult,
    ) -> StepResult {
        self.stamp_step_batch_metadata(step, batch_result.batch_frame.as_mut());
        let batch_metadata = batch_result
            .batch_frame
            .as_ref()
            .map(|frame| frame.metadata().clone());
        let runtime_metrics =
            self.extract_runtime_metrics(batch_result.batch_frame.as_ref(), &batch_result.output);

        // NOTE: We return the full output including _rows here.
        // The execute loop will strip _rows AFTER updating working_data so downstream
        // steps can still access the materialized data through working_data.
        StepResult {
            step_id: step.id.clone(),
            success: batch_result.success,
            output: batch_result.output,
            confidence: batch_result.confidence,
            started_at,
            completed_at,
            batch_metadata,
            runtime_metrics,
            batch_frame: batch_result.batch_frame,
        }
    }

    pub(super) fn stamp_step_batch_metadata(
        &self,
        step: &WorkflowStep,
        batch_frame: Option<&mut BatchFrame>,
    ) {
        let Some(frame) = batch_frame else {
            return;
        };

        if matches!(step.step_type, StepType::DbExtract)
            && frame.metadata().source_step_id.is_none()
            && frame.metadata().source_kind.is_none()
        {
            *frame = frame.clone().with_metadata(BatchFrameMetadata {
                source_step_id: Some(step.id.clone()),
                source_kind: Some("db_extract".to_string()),
                source_id: None,
            });
        }
    }

    pub(super) fn extract_runtime_metrics(
        &self,
        _batch_frame: Option<&BatchFrame>,
        output: &serde_json::Value,
    ) -> Option<RuntimeStepMetrics> {
        output
            .get("_runtime_metrics")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
    }
}
