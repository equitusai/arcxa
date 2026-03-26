use anyhow::Result;

use super::{BatchStepExecutionResult, ExecutionContext, WorkflowExecutor};
use crate::orchestration::workflow::field_transformer;

impl WorkflowExecutor {
    /// Execute field transformer step - REAL IMPLEMENTATION with RDF lineage
    pub(super) async fn execute_field_transformer(
        &self,
        config: &crate::orchestration::workflow::definition::FieldTransformerConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        if matches!(context.working_data, serde_json::Value::Array(_))
            || context
                .working_data
                .get("_rows")
                .and_then(|value| value.as_array())
                .is_some()
        {
            let rows = self.get_rows_from_context(context)?;
            if let Some(batch_result) =
                self.try_execute_field_transformer_batch(context, config, &rows)?
            {
                return Ok(batch_result);
            }
        }

        let (success, output, confidence) =
            field_transformer::execute_legacy_object_transform(config, &context.working_data)?;
        Ok(BatchStepExecutionResult::without_frame(
            success, output, confidence,
        ))
    }
}
