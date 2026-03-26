use anyhow::Result;

use super::{
    build_batch_rows_success_result, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor,
};
use crate::orchestration::workflow::runtime::operators::AggregatorBatchOperator;

impl WorkflowExecutor {
    pub(super) fn try_execute_aggregator_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::AggregatorConfig,
        rows: &[serde_json::Value],
    ) -> Result<Option<BatchStepExecutionResult>> {
        let operator = AggregatorBatchOperator;
        let Some(result) = self
            .try_with_context_batch_frame(context, rows, |frame| operator.execute(frame, config))?
        else {
            return Ok(None);
        };

        Ok(Some(build_batch_rows_success_result(
            result,
            vec![("_original_count".to_string(), serde_json::json!(rows.len()))],
        )?))
    }
}
