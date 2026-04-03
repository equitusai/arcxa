use anyhow::Result;
use std::collections::HashMap;

use super::state::ExecuteLoopState;
use super::{ExecutionContext, StepResult, WorkflowExecutor, WorkflowStep};

impl WorkflowExecutor {
    pub(super) async fn finalize_step_execution(
        &self,
        execution_id: &str,
        step: &WorkflowStep,
        step_result: &StepResult,
        state: &mut ExecuteLoopState,
    ) -> Result<()> {
        tracing::info!(
            "EXECUTE_LOOP: Step '{}' returned from execute_step successfully",
            step.id
        );

        tracing::info!(
            "EXECUTE_LOOP: Step '{}' completed successfully, recording lineage...",
            step.id
        );

        self.record_step_lineage(execution_id, step, step_result)
            .await;
        self.merge_and_store_step_result(&mut state.context, &mut state.step_results, step_result)
    }

    pub(super) fn merge_and_store_step_result(
        &self,
        context: &mut ExecutionContext,
        step_results: &mut HashMap<String, StepResult>,
        step_result: &StepResult,
    ) -> Result<()> {
        // Merge step output into working_data for next steps to access FIRST.
        // This enables data pipeline: each step can build upon previous steps' outputs.
        // IMPORTANT: Do this BEFORE stripping _rows so working_data has the full dataset.
        context
            .merge_step_output_with_batch(&step_result.output, step_result.batch_frame.clone())?;

        let output_for_storage =
            self.build_step_output_for_storage(&step_result.step_id, &step_result.output);

        context
            .step_outputs
            .insert(step_result.step_id.clone(), output_for_storage.clone());
        step_results.insert(
            step_result.step_id.clone(),
            self.build_stored_step_result(step_result, output_for_storage),
        );

        Ok(())
    }

    pub(super) fn build_step_output_for_storage(
        &self,
        step_id: &str,
        output: &serde_json::Value,
    ) -> serde_json::Value {
        // For large datasets, build stripped output WITHOUT cloning _rows.
        // This prevents expensive clone operations on multi-GB datasets.
        if let Some(row_count) = output.get("_row_count").and_then(|v| v.as_u64()) {
            if row_count > super::LARGE_ROW_PAYLOAD_THRESHOLD {
                tracing::info!(
                    "EXECUTE_LOOP: Step '{}' has {} rows, creating metadata-only output for storage (keeping _rows in working_data)",
                    step_id,
                    row_count
                );

                if let serde_json::Value::Object(output_obj) = output {
                    let mut stripped = serde_json::Map::new();
                    for (key, value) in output_obj {
                        if key != "_rows" {
                            stripped.insert(key.clone(), value.clone());
                        }
                    }
                    return serde_json::Value::Object(stripped);
                }
            }
        }

        output.clone()
    }

    pub(super) fn build_stored_step_result(
        &self,
        step_result: &StepResult,
        output_for_storage: serde_json::Value,
    ) -> StepResult {
        StepResult {
            step_id: step_result.step_id.clone(),
            success: step_result.success,
            output: output_for_storage,
            confidence: step_result.confidence,
            started_at: step_result.started_at,
            completed_at: step_result.completed_at,
            batch_metadata: step_result.batch_metadata.clone(),
            runtime_metrics: step_result.runtime_metrics.clone(),
            batch_frame: None,
        }
    }
}
