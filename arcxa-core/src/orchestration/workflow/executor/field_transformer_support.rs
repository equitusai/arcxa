use anyhow::Result;

use super::{
    build_batch_rows_success_result, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor,
};
use crate::orchestration::workflow::runtime::operators::FieldTransformerBatchOperator;

impl WorkflowExecutor {
    pub(super) fn try_execute_field_transformer_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::FieldTransformerConfig,
        rows: &[serde_json::Value],
    ) -> Result<Option<BatchStepExecutionResult>> {
        let operator = FieldTransformerBatchOperator;
        let Some(result) = self
            .try_with_context_batch_frame(context, rows, |frame| operator.execute(frame, config))?
        else {
            return Ok(None);
        };

        let modifications = result
            .modifications
            .into_iter()
            .map(|modification| modification.to_json())
            .collect::<Vec<_>>();

        Ok(Some(build_batch_rows_success_result(
            result.frame,
            vec![
                (
                    "_rows_transformed".to_string(),
                    serde_json::json!(result.stats.rows_transformed),
                ),
                (
                    "_fields_modified".to_string(),
                    serde_json::json!(result.stats.fields_modified),
                ),
                (
                    "_modifications".to_string(),
                    serde_json::Value::Array(modifications),
                ),
            ],
        )?))
    }
}
