use anyhow::Result;

use crate::orchestration::workflow::runtime::frame::BatchFrame;

use super::{ExecutionContext, WorkflowExecutor};

impl WorkflowExecutor {
    pub(super) fn get_context_batch_frame(
        &self,
        context: &ExecutionContext,
        rows: &[serde_json::Value],
    ) -> Result<Option<BatchFrame>> {
        let working_rows_available = matches!(context.working_data, serde_json::Value::Array(_))
            || context
                .working_data
                .get("_rows")
                .and_then(|value| value.as_array())
                .is_some();

        if working_rows_available {
            if let Ok(frame) = context.get_batch_frame() {
                return Ok(Some(frame));
            }
        }

        match BatchFrame::from_json_values(rows) {
            Ok(frame) => Ok(Some(frame)),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn try_with_context_batch_frame<T, E, F>(
        &self,
        context: &ExecutionContext,
        rows: &[serde_json::Value],
        execute: F,
    ) -> Result<Option<T>>
    where
        E: Into<anyhow::Error>,
        F: FnOnce(BatchFrame) -> std::result::Result<T, E>,
    {
        let Some(frame) = self.get_context_batch_frame(context, rows)? else {
            return Ok(None);
        };

        match execute(frame) {
            Ok(result) => Ok(Some(result)),
            Err(_error) => Ok(None),
        }
    }
}
