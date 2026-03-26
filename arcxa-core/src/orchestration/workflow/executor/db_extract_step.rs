use anyhow::Result;

use super::{build_rows_output, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor};
use crate::orchestration::workflow::runtime::frame::BatchFrame;

impl WorkflowExecutor {
    /// Execute DB extract step - placeholder for database extraction
    pub(super) async fn execute_db_extract(
        &self,
        config: &crate::orchestration::workflow::definition::DbExtractConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        tracing::info!(
            "Executing DB extract: datasource={}, table={:?}",
            config.datasource_id,
            config.table_name
        );

        if let Some(callback) = &self.db_extract_callback {
            let result = callback(config, context).await?;
            let frame = BatchFrame::from_object_rows(&result.rows).ok();
            let rows: Vec<serde_json::Value> = result
                .rows
                .into_iter()
                .map(serde_json::Value::Object)
                .collect();
            let row_count = if result.row_count > 0 {
                result.row_count
            } else {
                rows.len()
            };

            // Resource limit checks (row count only, memory handled by caller)
            if context.resource_limits.enforce_limits {
                if let Some(max_rows) = context.resource_limits.max_rows {
                    if row_count > max_rows {
                        tracing::warn!(
                            "DB extract exceeded row limit. Extracted {} rows, limit is {}. Continuing anyway for now.",
                            row_count,
                            max_rows
                        );
                    }
                }
            }

            let mut output = build_rows_output(
                rows,
                row_count,
                vec![
                    (
                        "_datasource_id".to_string(),
                        serde_json::json!(config.datasource_id),
                    ),
                    (
                        "_table_name".to_string(),
                        serde_json::json!(config.table_name),
                    ),
                    ("_query".to_string(), serde_json::json!(config.query)),
                    ("_status".to_string(), serde_json::json!("success")),
                ],
            );

            if let Some(schema) = result.schema {
                if let serde_json::Value::Object(ref mut obj) = output {
                    obj.insert("schema".to_string(), schema);
                }
            }

            return Ok(BatchStepExecutionResult::new(true, output, 1.0, frame));
        }

        // No callback - fallback to stub behavior
        tracing::warn!(
            "DB extract callback not set - would extract from {}.{:?}",
            config.datasource_id,
            config.table_name
        );

        Ok(BatchStepExecutionResult::success(build_rows_output(
            vec![],
            0,
            vec![
                (
                    "_datasource_id".to_string(),
                    serde_json::json!(config.datasource_id),
                ),
                (
                    "_table_name".to_string(),
                    serde_json::json!(config.table_name),
                ),
                ("_query".to_string(), serde_json::json!(config.query)),
                (
                    "_status".to_string(),
                    serde_json::json!("stub_implementation"),
                ),
            ],
        )))
    }
}
