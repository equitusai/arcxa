use crate::orchestration::workflow::definition::CsvExporterConfig;
use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use arrow2::array::{Array, BooleanArray, PrimitiveArray, Utf8Array};

use super::RuntimeOperator;

#[derive(Debug, Default)]
pub struct CsvExportBatchOperator;

impl RuntimeOperator for CsvExportBatchOperator {
    fn name(&self) -> &'static str {
        "csv_export"
    }
}

impl CsvExportBatchOperator {
    pub fn execute(&self, frame: &BatchFrame, config: &CsvExporterConfig) -> Result<usize> {
        let file = std::fs::File::create(&config.output_path)
            .map_err(|e| WorkflowError::IoError(e.to_string()))?;
        let mut writer = csv::WriterBuilder::new()
            .delimiter(config.delimiter.unwrap_or(',') as u8)
            .from_writer(file);

        let headers: Vec<String> = frame
            .schema()
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();

        if config.include_header && !headers.is_empty() {
            writer
                .write_record(&headers)
                .map_err(|e| WorkflowError::IoError(e.to_string()))?;
        }

        for row_index in 0..frame.row_count() {
            let record = frame
                .schema()
                .fields
                .iter()
                .zip(frame.columns().arrays().iter())
                .map(|(field, column)| {
                    csv_cell_string(field.data_type(), column.as_ref(), row_index)
                })
                .collect::<Result<Vec<_>>>()?;

            writer
                .write_record(&record)
                .map_err(|e| WorkflowError::IoError(e.to_string()))?;
        }

        writer
            .flush()
            .map_err(|e| WorkflowError::IoError(e.to_string()))?;

        Ok(frame.row_count())
    }
}

fn csv_cell_string(
    data_type: &arrow2::datatypes::DataType,
    column: &dyn Array,
    row_index: usize,
) -> Result<String> {
    if column.is_null(row_index) {
        return Ok(String::new());
    }

    match data_type {
        arrow2::datatypes::DataType::Boolean => {
            let array = column
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected BooleanArray in batch CSV export".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        arrow2::datatypes::DataType::Int64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Int64Array in batch CSV export".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        arrow2::datatypes::DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Float64Array in batch CSV export".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        arrow2::datatypes::DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Utf8Array in batch CSV export".into())
                })?;
            Ok(array.value(row_index).to_string())
        }
        other => Err(WorkflowError::NotImplemented(format!(
            "Batch CSV export does not support Arrow type {:?}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::CsvExportBatchOperator;
    use crate::orchestration::workflow::definition::CsvExporterConfig;
    use crate::orchestration::workflow::runtime::frame::BatchFrame;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn writes_csv_from_batch_frame() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("frame.csv");
        let frame = BatchFrame::from_json_values(&[
            json!({"id": 1, "name": "Alice", "active": true}),
            json!({"id": 2, "name": "Bob", "active": false}),
        ])
        .unwrap();

        let operator = CsvExportBatchOperator;
        let rows_written = operator
            .execute(
                &frame,
                &CsvExporterConfig {
                    output_path: output_path.to_string_lossy().into_owned(),
                    delimiter: Some(','),
                    include_header: true,
                    encoding: None,
                },
            )
            .unwrap();

        assert_eq!(rows_written, 2);

        let content = std::fs::read_to_string(output_path).unwrap();
        assert!(content.contains("active,id,name"));
        assert!(content.contains("true,1,Alice"));
        assert!(content.contains("false,2,Bob"));
    }
}
