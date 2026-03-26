use anyhow::{Context, Result};
use uuid::Uuid;

use super::state::{
    ExecuteLoopOutcome, ExecuteLoopState, WorkflowExecutionSession, WorkflowRunState,
};
use super::{ExecutionContext, WorkflowExecutor, WorkflowResult, WorkflowStep};

impl WorkflowExecutor {
    pub(super) async fn execute_session(
        &self,
        session: WorkflowExecutionSession,
        input: ExecutionContext,
    ) -> Result<WorkflowResult> {
        let WorkflowExecutionSession {
            run_state,
            execution_order,
        } = session;
        let outcome = self
            .execute_ordered_steps(&run_state, execution_order, input)
            .await?;

        self.complete_session_outcome(&run_state, outcome).await
    }

    pub(super) async fn complete_session_outcome(
        &self,
        run_state: &WorkflowRunState,
        outcome: ExecuteLoopOutcome,
    ) -> Result<WorkflowResult> {
        match outcome {
            ExecuteLoopOutcome::Failed(result) => Ok(result),
            ExecuteLoopOutcome::Completed {
                context,
                step_results,
            } => {
                self.complete_successful_workflow_execution(
                    run_state,
                    step_results,
                    context.working_data.clone(),
                )
                .await
            }
        }
    }

    pub(super) async fn initialize_execution_session(&self) -> Result<WorkflowExecutionSession> {
        let run_state = self.create_workflow_run_state();

        self.record_workflow_start_lineage(&run_state.execution_id, run_state.started_at)
            .await;

        let execution_order = self.compute_execution_order()?;

        Ok(WorkflowExecutionSession {
            run_state,
            execution_order,
        })
    }

    pub(super) fn create_workflow_run_state(&self) -> WorkflowRunState {
        WorkflowRunState {
            execution_id: format!("exec_{}", Uuid::new_v4()),
            started_at: chrono::Utc::now(),
        }
    }

    pub(super) fn compute_execution_order(&self) -> Result<Vec<WorkflowStep>> {
        self.dag
            .execution_order()
            .context("Failed to compute execution order")
    }

    pub(super) async fn execute_ordered_steps(
        &self,
        run_state: &WorkflowRunState,
        execution_order: Vec<WorkflowStep>,
        context: ExecutionContext,
    ) -> Result<ExecuteLoopOutcome> {
        let mut state = ExecuteLoopState::new(context);

        for step in execution_order {
            if let Some(result) = self
                .execute_ordered_step(run_state, &step, &mut state)
                .await?
            {
                return Ok(ExecuteLoopOutcome::Failed(result));
            }
        }

        Ok(state.into_completed_outcome())
    }

    pub(super) async fn execute_ordered_step(
        &self,
        run_state: &WorkflowRunState,
        step: &WorkflowStep,
        state: &mut ExecuteLoopState,
    ) -> Result<Option<WorkflowResult>> {
        self.prepare_step_execution_state(step, &mut state.context);

        let step_result = self
            .execute_step(step, &state.context)
            .await
            .with_context(|| format!("Failed to execute step '{}'", step.id))?;

        self.finalize_step_execution(&run_state.execution_id, step, &step_result, state)
            .await?;

        Ok(self.complete_failed_step_execution(run_state, step, &step_result, state))
    }
}
