use std::collections::HashMap;

use super::{ExecutionContext, StepResult, WorkflowResult, WorkflowStep};

pub(super) enum ExecuteLoopOutcome {
    Completed {
        context: ExecutionContext,
        step_results: HashMap<String, StepResult>,
    },
    Failed(WorkflowResult),
}

#[derive(Clone)]
pub(super) struct WorkflowRunState {
    pub(super) execution_id: String,
    pub(super) started_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub(super) struct WorkflowExecutionSession {
    pub(super) run_state: WorkflowRunState,
    pub(super) execution_order: Vec<WorkflowStep>,
}

pub(super) struct ExecuteLoopState {
    pub(super) context: ExecutionContext,
    pub(super) step_results: HashMap<String, StepResult>,
}

impl ExecuteLoopState {
    pub(super) fn new(context: ExecutionContext) -> Self {
        Self {
            context,
            step_results: HashMap::new(),
        }
    }

    pub(super) fn into_completed_outcome(self) -> ExecuteLoopOutcome {
        ExecuteLoopOutcome::Completed {
            context: self.context,
            step_results: self.step_results,
        }
    }
}
