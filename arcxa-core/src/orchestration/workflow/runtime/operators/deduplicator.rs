use crate::orchestration::workflow::definition::DeduplicatorConfig;
use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use arrow2::array::{Array, BooleanArray, PrimitiveArray, Utf8Array};
use std::collections::HashSet;

use super::RuntimeOperator;

/// Batch-native deduplicator for the optimized small-dataset path.
///
/// This intentionally mirrors the current optimized executor behavior:
/// deduplication keeps the first-seen row for a key regardless of the richer
/// keep strategies exposed by the legacy executor.
#[derive(Debug, Default)]
pub struct DeduplicatorBatchOperator;

impl RuntimeOperator for DeduplicatorBatchOperator {
    fn name(&self) -> &'static str {
        "deduplicator"
    }
}

impl DeduplicatorBatchOperator {
    pub fn execute(&self, frame: BatchFrame, config: &DeduplicatorConfig) -> Result<BatchFrame> {
        let field_positions = config
            .key_fields
            .iter()
            .map(|field| {
                frame
                    .schema()
                    .fields
                    .iter()
                    .position(|schema_field| schema_field.name == *field)
            })
            .collect::<Vec<_>>();

        let mut seen_keys = HashSet::new();
        let mut keep_indices = Vec::new();

        for row_index in 0..frame.row_count() {
            let key = build_dedup_key(&frame, row_index, &field_positions)?;
            if seen_keys.insert(key) {
                keep_indices.push(row_index);
            }
        }

        frame.select_rows(&keep_indices)
    }
}

fn build_dedup_key(
    frame: &BatchFrame,
    row_index: usize,
    field_positions: &[Option<usize>],
) -> Result<String> {
    let mut key_parts = Vec::with_capacity(field_positions.len());

    for field_position in field_positions {
        let value = match field_position {
            Some(field_index) => {
                let field = &frame.schema().fields[*field_index];
                let column = frame.columns().arrays()[*field_index].as_ref();
                dedup_cell_string(field.data_type(), column, row_index)?
            }
            None => "null".to_string(),
        };

        key_parts.push(value);
    }

    Ok(key_parts.join("|"))
}

fn dedup_cell_string(
    data_type: &arrow2::datatypes::DataType,
    column: &dyn Array,
    row_index: usize,
) -> Result<String> {
    if column.is_null(row_index) {
        return Ok("null".to_string());
    }

    match data_type {
        arrow2::datatypes::DataType::Boolean => {
            let array = column
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected BooleanArray in batch deduplicator".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        arrow2::datatypes::DataType::Int64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Int64Array in batch deduplicator".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        arrow2::datatypes::DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Float64Array in batch deduplicator".into())
                })?;
            Ok(serde_json::Number::from_f64(array.value(row_index))
                .map(|number| number.to_string())
                .unwrap_or_else(|| "null".to_string()))
        }
        arrow2::datatypes::DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Utf8Array in batch deduplicator".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        other => Err(WorkflowError::NotImplemented(format!(
            "Batch deduplicator does not support Arrow type {:?}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::DeduplicatorBatchOperator;
    use crate::orchestration::workflow::definition::{
        DedupMethod, DeduplicatorConfig, KeepStrategy,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};
    use serde_json::json;

    #[test]
    fn preserves_metadata_and_keeps_first_seen_rows() {
        let frame = BatchFrame::from_json_values(&[
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
            json!({"id": 1, "name": "Alice Updated"}),
            json!({"id": 3, "name": "Charlie"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_people".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });

        let operator = DeduplicatorBatchOperator;
        let deduped = operator
            .execute(
                frame,
                &DeduplicatorConfig {
                    method: DedupMethod::Exact,
                    key_fields: vec!["id".to_string()],
                    threshold: None,
                    keep: KeepStrategy::First,
                },
            )
            .unwrap();

        let rows = deduped.to_json_values().unwrap();

        assert_eq!(deduped.row_count(), 3);
        assert_eq!(
            deduped.metadata().source_step_id.as_deref(),
            Some("extract_people")
        );
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["name"], "Bob");
        assert_eq!(rows[2]["name"], "Charlie");
    }
}
