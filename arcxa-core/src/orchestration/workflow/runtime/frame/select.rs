use super::batch::BatchFrame;
use crate::orchestration::workflow::error::{Result, WorkflowError};

impl BatchFrame {
    pub fn select_rows(&self, row_indices: &[usize]) -> Result<Self> {
        for row_index in row_indices {
            if *row_index >= self.row_count {
                return Err(WorkflowError::InvalidData(format!(
                    "BatchFrame row index {} is out of bounds for {} rows",
                    row_index, self.row_count
                )));
            }
        }

        let rows = self.to_json_values()?;
        let selected_rows = row_indices
            .iter()
            .map(|row_index| rows[*row_index].clone())
            .collect::<Vec<_>>();

        Ok(Self::from_json_values(&selected_rows)?.with_metadata(self.metadata.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::BatchFrame;
    use crate::orchestration::workflow::runtime::frame::BatchFrameMetadata;
    use serde_json::json;

    #[test]
    fn select_rows_preserves_metadata_and_order() {
        let frame = BatchFrame::from_json_values(&[
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
            json!({"id": 3, "name": "Charlie"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });

        let selected = frame.select_rows(&[2, 0]).unwrap();
        let rows = selected.to_json_values().unwrap();

        assert_eq!(selected.row_count(), 2);
        assert_eq!(
            selected.metadata().source_step_id.as_deref(),
            Some("extract")
        );
        assert_eq!(rows[0]["id"], 3);
        assert_eq!(rows[1]["id"], 1);
    }
}
