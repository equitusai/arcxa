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
    ) -> Result<Option<BatchStepExecutionResult>> {
        let operator = AggregatorBatchOperator;
        if let Some((result, original_count)) =
            self.try_with_cached_context_batch_frame(context, |frame| {
                let original_count = frame.row_count();
                operator
                    .execute(frame, config)
                    .map(|result| (result, original_count))
            })?
        {
            return Ok(Some(build_batch_rows_success_result(
                result,
                vec![(
                    "_original_count".to_string(),
                    serde_json::json!(original_count),
                )],
            )?));
        }

        let rows = self.get_rows_from_context(context)?;
        let Some(result) = self.try_with_context_batch_frame(context, &rows, |frame| {
            operator.execute(frame, config)
        })?
        else {
            return Ok(None);
        };

        Ok(Some(build_batch_rows_success_result(
            result,
            vec![("_original_count".to_string(), serde_json::json!(rows.len()))],
        )?))
    }
}
