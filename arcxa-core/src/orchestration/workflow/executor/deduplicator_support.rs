use anyhow::Result;

use super::{
    build_batch_rows_success_result, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor,
};
use crate::orchestration::workflow::definition::{DedupMethod, KeepStrategy};
use crate::orchestration::workflow::runtime::operators::DeduplicatorBatchOperator;

impl WorkflowExecutor {
    pub(super) fn try_execute_deduplicator_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::DeduplicatorConfig,
    ) -> Result<Option<BatchStepExecutionResult>> {
        if !matches!(config.method, DedupMethod::Exact)
            || !matches!(config.keep, KeepStrategy::First)
        {
            return Ok(None);
        }

        let operator = DeduplicatorBatchOperator;
        if let Some((result, original_count)) =
            self.try_with_cached_context_batch_frame(context, |frame| {
                let original_count = frame.row_count();
                operator
                    .execute(frame, config)
                    .map(|result| (result, original_count))
            })?
        {
            return self.build_deduplicator_batch_result(config, result, original_count);
        }

        let rows = self.get_rows_from_context(context)?;
        let Some(result) = self.try_with_context_batch_frame(context, &rows, |frame| {
            operator.execute(frame, config)
        })?
        else {
            return Ok(None);
        };

        self.build_deduplicator_batch_result(config, result, rows.len())
    }

    fn build_deduplicator_batch_result(
        &self,
        config: &crate::orchestration::workflow::definition::DeduplicatorConfig,
        result: crate::orchestration::workflow::runtime::frame::BatchFrame,
        original_count: usize,
    ) -> Result<Option<BatchStepExecutionResult>> {
        let deduped_count = result.row_count();
        let duplicate_count = original_count.saturating_sub(deduped_count);
        let dedup_rate = if original_count == 0 {
            0.0
        } else {
            (duplicate_count as f64 / original_count as f64) * 100.0
        };
        let modifications = vec![serde_json::json!({
            "field_name": "_deduplication",
            "old_value": original_count,
            "new_value": deduped_count,
            "is_reversible": false,
            "operations": duplicate_count,
            "metadata": {
                "method": format!("{:?}", config.method),
                "keep_strategy": format!("{:?}", config.keep),
                "key_fields": config.key_fields.clone(),
                "duplicates_removed": duplicate_count,
                "dedup_rate_percent": dedup_rate,
                "execution_path": "batch_frame",
            }
        })];

        Ok(Some(build_batch_rows_success_result(
            result,
            vec![
                (
                    "_original_count".to_string(),
                    serde_json::json!(original_count),
                ),
                (
                    "_duplicates_removed".to_string(),
                    serde_json::json!(duplicate_count),
                ),
                (
                    "_modifications".to_string(),
                    serde_json::Value::Array(modifications),
                ),
            ],
        )?))
    }
}
