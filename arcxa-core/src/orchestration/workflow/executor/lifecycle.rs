use super::{WorkflowExecutionRecord, WorkflowExecutor};

impl WorkflowExecutor {
    pub(super) async fn record_workflow_start_lineage(
        &self,
        execution_id: &str,
        started_at: chrono::DateTime<chrono::Utc>,
    ) {
        let Some(tracker) = &self.lineage_tracker else {
            return;
        };

        tracker
            .record_workflow_start(WorkflowExecutionRecord {
                execution_id: execution_id.to_string(),
                workflow_id: format!("workflow_{}", chrono::Utc::now().timestamp()),
                started_at,
            })
            .await
            .ok();
    }

    pub(super) async fn record_workflow_completion_lineage(
        &self,
        execution_id: &str,
        success: bool,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) {
        let Some(tracker) = &self.lineage_tracker else {
            return;
        };

        tracker
            .record_workflow_complete(execution_id.to_string(), success, completed_at)
            .await
            .ok();
    }
}
