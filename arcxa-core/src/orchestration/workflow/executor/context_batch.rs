use anyhow::Result;

use super::{BatchFrame, BatchFrameMetadata, ExecutionContext, LARGE_ROW_PAYLOAD_THRESHOLD};

impl ExecutionContext {
    fn should_strip_rows_from_working_data(
        output: &serde_json::Value,
        batch_frame: Option<&BatchFrame>,
    ) -> bool {
        batch_frame.is_some()
            && output
                .get("_row_count")
                .and_then(|value| value.as_u64())
                .map(|row_count| row_count > LARGE_ROW_PAYLOAD_THRESHOLD)
                .unwrap_or(false)
            && output
                .get("_rows")
                .and_then(|value| value.as_array())
                .is_some()
    }

    pub(super) fn build_working_output_with_batch(
        output: &serde_json::Value,
        batch_frame: Option<&BatchFrame>,
    ) -> serde_json::Value {
        if !Self::should_strip_rows_from_working_data(output, batch_frame) {
            return output.clone();
        }

        match output {
            serde_json::Value::Object(map) => {
                let mut stripped = serde_json::Map::with_capacity(map.len().saturating_sub(1));
                for (key, value) in map {
                    if key != "_rows" {
                        stripped.insert(key.clone(), value.clone());
                    }
                }
                serde_json::Value::Object(stripped)
            }
            _ => output.clone(),
        }
    }

    pub(super) fn materialize_working_output(&self) -> Result<serde_json::Value> {
        if self.working_data.as_array().is_some()
            || self
                .working_data
                .get("_rows")
                .and_then(|value| value.as_array())
                .is_some()
            || self.batch_frame.is_none()
        {
            return Ok(self.working_data.clone());
        }

        let frame = self
            .batch_frame
            .as_ref()
            .expect("batch frame presence already checked");
        let rows = frame.to_json_values()?;

        match &self.working_data {
            serde_json::Value::Object(map) => {
                let mut materialized = map.clone();
                materialized.insert("_rows".to_string(), serde_json::Value::Array(rows));
                materialized
                    .entry("_row_count".to_string())
                    .or_insert_with(|| serde_json::json!(frame.row_count()));
                Ok(serde_json::Value::Object(materialized))
            }
            _ => Ok(self.working_data.clone()),
        }
    }

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
