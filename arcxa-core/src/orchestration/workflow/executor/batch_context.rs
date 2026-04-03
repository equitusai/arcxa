use anyhow::Result;
use serde_json::{Map, Value};

use crate::orchestration::workflow::runtime::frame::{json_values_to_object_rows, BatchFrame};

use super::{ExecutionContext, WorkflowExecutor};

impl WorkflowExecutor {
    pub(super) fn context_has_row_payload(&self, context: &ExecutionContext) -> bool {
        context.batch_frame.is_some()
            || matches!(context.working_data, serde_json::Value::Array(_))
            || context
                .working_data
                .get("_rows")
                .and_then(|value| value.as_array())
                .is_some()
    }

    pub(super) fn get_cached_context_batch_frame(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<BatchFrame>> {
        if !self.context_has_row_payload(context) {
            return Ok(None);
        }

        match context.get_batch_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn get_context_batch_frame(
        &self,
        context: &ExecutionContext,
        rows: &[serde_json::Value],
    ) -> Result<Option<BatchFrame>> {
        if let Some(frame) = self.get_cached_context_batch_frame(context)? {
            return Ok(Some(frame));
        }

        match BatchFrame::from_json_values(rows) {
            Ok(frame) => Ok(Some(frame)),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn try_with_cached_context_batch_frame<T, E, F>(
        &self,
        context: &ExecutionContext,
        execute: F,
    ) -> Result<Option<T>>
    where
        E: Into<anyhow::Error>,
        F: FnOnce(BatchFrame) -> std::result::Result<T, E>,
    {
        let Some(frame) = self.get_cached_context_batch_frame(context)? else {
            return Ok(None);
        };

        match execute(frame) {
            Ok(result) => Ok(Some(result)),
            Err(_error) => Ok(None),
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

    pub(super) fn get_context_row_count(&self, context: &ExecutionContext) -> Result<usize> {
        if let Some(frame) = self.get_cached_context_batch_frame(context)? {
            return Ok(frame.row_count());
        }

        Ok(self.get_rows_from_context(context)?.len())
    }

    pub(super) fn get_context_object_rows(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<Map<String, Value>>> {
        if let Some(frame) = self.get_cached_context_batch_frame(context)? {
            return Ok(frame.to_object_rows()?);
        }

        let rows = self.get_rows_from_context(context)?;
        Ok(json_values_to_object_rows(&rows)?)
    }
}
