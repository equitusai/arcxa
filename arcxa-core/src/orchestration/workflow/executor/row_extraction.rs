use anyhow::Result;

use super::{ExecutionContext, WorkflowExecutor};

impl WorkflowExecutor {
    /// Extract rows from execution context, preferring the merged working set
    /// and falling back to stored step outputs when needed.
    pub(super) fn get_rows_from_context(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<serde_json::Value>> {
        if let Some(rows) = context
            .working_data
            .get("_rows")
            .and_then(|value| value.as_array())
        {
            return Ok(rows.clone());
        }

        if let Some(frame) = &context.batch_frame {
            return Ok(frame.to_json_values()?);
        }

        let mut best_rows: Option<Vec<serde_json::Value>> = None;
        for output in context.step_outputs.values() {
            if let Some(rows) = output.get("_rows").and_then(|value| value.as_array()) {
                best_rows = Some(rows.clone());
            }
        }
        if let Some(rows) = best_rows {
            return Ok(rows);
        }

        if let serde_json::Value::Array(rows) = &context.working_data {
            return Ok(rows.clone());
        }

        Ok(Vec::new())
    }
}
