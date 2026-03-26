use anyhow::Result;

use super::{BatchFrame, BatchFrameMetadata, ExecutionContext};

impl ExecutionContext {
    /// Update the cached batch-frame metadata without rebuilding rows.
    pub fn set_batch_frame_metadata(&mut self, metadata: BatchFrameMetadata) {
        if let Some(frame) = self.batch_frame.take() {
            self.batch_frame = Some(frame.with_metadata(metadata));
        }
    }

    /// Reconstruct the current working rows as a batch-oriented frame.
    pub fn get_batch_frame(&self) -> Result<BatchFrame> {
        if let Some(frame) = &self.batch_frame {
            return Ok(frame.clone());
        }

        if let Some(rows) = self.working_data.as_array() {
            return Ok(BatchFrame::from_json_values(rows)?);
        }

        if let Some(rows) = self
            .working_data
            .get("_rows")
            .and_then(|value| value.as_array())
        {
            return Ok(BatchFrame::from_json_values(rows)?);
        }

        anyhow::bail!("ExecutionContext does not currently hold row-oriented JSON data")
    }

    pub(super) fn refresh_batch_frame_from_working_data(&mut self) -> Result<()> {
        self.batch_frame = if let Some(rows) = self.working_data.as_array() {
            Some(BatchFrame::from_json_values(rows)?)
        } else if let Some(rows) = self
            .working_data
            .get("_rows")
            .and_then(|value| value.as_array())
        {
            Some(BatchFrame::from_json_values(rows)?)
        } else {
            None
        };

        Ok(())
    }

    pub(super) fn infer_batch_frame_from_value(value: &serde_json::Value) -> Option<BatchFrame> {
        if let Some(rows) = value.as_array() {
            return BatchFrame::from_json_values(rows).ok();
        }

        value
            .get("_rows")
            .and_then(|rows| rows.as_array())
            .and_then(|rows| BatchFrame::from_json_values(rows).ok())
    }
}
