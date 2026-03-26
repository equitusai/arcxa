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
        rows: &[serde_json::Value],
    ) -> Result<Option<BatchStepExecutionResult>> {
        if !matches!(config.method, DedupMethod::Exact)
            || !matches!(config.keep, KeepStrategy::First)
        {
            return Ok(None);
        }

        let operator = DeduplicatorBatchOperator;
        let Some(result) = self
            .try_with_context_batch_frame(context, rows, |frame| operator.execute(frame, config))?
        else {
            return Ok(None);
        };

        let deduped_count = result.row_count();
        let duplicate_count = rows.len().saturating_sub(deduped_count);
        let dedup_rate = if rows.is_empty() {
            0.0
        } else {
            (duplicate_count as f64 / rows.len() as f64) * 100.0
        };
        let modifications = vec![serde_json::json!({
            "field_name": "_deduplication",
            "old_value": rows.len(),
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
                ("_original_count".to_string(), serde_json::json!(rows.len())),
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
