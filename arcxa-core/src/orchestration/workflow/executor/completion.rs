use std::collections::HashMap;

use anyhow::Result;

use super::state::{ExecuteLoopState, WorkflowRunState};
use super::{
    extract_materializable_rows, FinalDecision, StepResult, WorkflowExecutor, WorkflowResult,
    WorkflowStep,
};

impl WorkflowExecutor {
    pub(super) fn complete_failed_step_execution(
        &self,
        run_state: &WorkflowRunState,
        step: &WorkflowStep,
        step_result: &StepResult,
        state: &ExecuteLoopState,
    ) -> Option<WorkflowResult> {
        if step_result.success {
            return None;
        }

        Some(self.build_failed_workflow_completion(
            run_state,
            step,
            step_result,
            state,
            chrono::Utc::now(),
        ))
    }

    pub(super) fn build_failed_workflow_completion(
        &self,
        run_state: &WorkflowRunState,
        step: &WorkflowStep,
        step_result: &StepResult,
        state: &ExecuteLoopState,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> WorkflowResult {
        self.build_failed_workflow_result(
            run_state.execution_id.clone(),
            state.step_results.clone(),
            run_state.started_at,
            completed_at,
            step_result.confidence,
            format!("Step '{}' failed", step.id),
            state.context.working_data.clone(),
        )
    }

    pub(super) async fn complete_successful_workflow_execution(
        &self,
        run_state: &WorkflowRunState,
        step_results: HashMap<String, StepResult>,
        final_output: serde_json::Value,
    ) -> Result<WorkflowResult> {
        let completed_at = chrono::Utc::now();

        self.record_workflow_completion_lineage(&run_state.execution_id, true, completed_at)
            .await;

        self.build_successful_workflow_completion(
            run_state,
            step_results,
            final_output,
            completed_at,
        )
    }

    pub(super) fn build_successful_workflow_completion(
        &self,
        run_state: &WorkflowRunState,
        step_results: HashMap<String, StepResult>,
        final_output: serde_json::Value,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<WorkflowResult> {
        let final_decision = self.compute_final_decision(&step_results)?;
        let final_confidence = self.compute_final_confidence(&step_results);

        Ok(self.build_success_workflow_result(
            run_state.execution_id.clone(),
            final_decision,
            final_confidence,
            step_results,
            run_state.started_at,
            completed_at,
            final_output,
        ))
    }

    pub(super) fn build_success_workflow_result(
        &self,
        execution_id: String,
        final_decision: FinalDecision,
        confidence: f64,
        step_results: HashMap<String, StepResult>,
        started_at: chrono::DateTime<chrono::Utc>,
        completed_at: chrono::DateTime<chrono::Utc>,
        final_output: serde_json::Value,
    ) -> WorkflowResult {
        let output_rows = extract_materializable_rows(&final_output);

        WorkflowResult {
            execution_id,
            success: true,
            final_decision,
            confidence,
            step_results,
            started_at,
            completed_at,
            error: None,
            final_output,
            output_rows,
        }
    }

    pub(super) fn build_failed_workflow_result(
        &self,
        execution_id: String,
        step_results: HashMap<String, StepResult>,
        started_at: chrono::DateTime<chrono::Utc>,
        completed_at: chrono::DateTime<chrono::Utc>,
        confidence: f64,
        error: String,
        final_output: serde_json::Value,
    ) -> WorkflowResult {
        let output_rows = extract_materializable_rows(&final_output);

        WorkflowResult {
            execution_id,
            success: false,
            final_decision: FinalDecision::Reject,
            confidence,
            step_results,
            started_at,
            completed_at,
            error: Some(error),
            final_output,
            output_rows,
        }
    }
}
