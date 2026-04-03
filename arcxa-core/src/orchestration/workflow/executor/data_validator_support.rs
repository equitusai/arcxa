use anyhow::Result;

use super::{
    build_batch_rows_step_result, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor,
};
use crate::orchestration::workflow::runtime::operators::DataValidatorBatchOperator;

impl WorkflowExecutor {
    pub(super) fn try_execute_data_validator_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::DataValidatorConfig,
    ) -> Result<Option<BatchStepExecutionResult>> {
        let operator = DataValidatorBatchOperator;
        let result = if let Some(result) = self
            .try_with_cached_context_batch_frame(context, |frame| operator.execute(frame, config))?
        {
            result
        } else {
            let rows = self.get_rows_from_context(context)?;
            let Some(result) = self.try_with_context_batch_frame(context, &rows, |frame| {
                operator.execute(frame, config)
            })?
            else {
                return Ok(None);
            };
            result
        };

        let success = result.success;
        let errors = result.errors;
        let warnings = result.warnings;
        let confidence = if errors.is_empty() { 1.0 } else { 0.0 };

        let error_count = errors.len();
        let warning_count = warnings.len();

        Ok(Some(build_batch_rows_step_result(
            result.frame,
            vec![
                (
                    "_errors".to_string(),
                    serde_json::Value::Array(errors.clone()),
                ),
                (
                    "_warnings".to_string(),
                    serde_json::Value::Array(warnings.clone()),
                ),
                ("_error_count".to_string(), serde_json::json!(error_count)),
                (
                    "_warning_count".to_string(),
                    serde_json::json!(warning_count),
                ),
            ],
            success,
            confidence,
        )?))
    }
}
