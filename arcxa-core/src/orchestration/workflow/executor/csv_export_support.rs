use anyhow::{Context, Result};

use super::{ExecutionContext, WorkflowExecutor};
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use crate::orchestration::workflow::runtime::operators::CsvExportBatchOperator;

pub(super) struct CsvExportBatchExecution {
    pub(super) rows_written: usize,
    pub(super) columns: Vec<String>,
    pub(super) frame: BatchFrame,
}

impl WorkflowExecutor {
    pub(super) fn try_execute_csv_export_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::CsvExporterConfig,
        actual_output_path: &str,
    ) -> Result<Option<CsvExportBatchExecution>> {
        let operator = CsvExportBatchOperator;
        let batch_config = crate::orchestration::workflow::definition::CsvExporterConfig {
            output_path: actual_output_path.to_string(),
            delimiter: config.delimiter,
            include_header: config.include_header,
            encoding: config.encoding.clone(),
        };

        if let Some(frame) = self.get_cached_context_batch_frame(context)? {
            return Ok(self
                .execute_csv_export_batch_frame(&operator, &batch_config, frame)
                .ok());
        }

        let rows = self.get_rows_from_context(context)?;
        let Some(frame) = self.get_context_batch_frame(context, &rows)? else {
            return Ok(None);
        };

        Ok(self
            .execute_csv_export_batch_frame(&operator, &batch_config, frame)
            .ok())
    }

    pub(super) fn write_csv_export_rows(
        &self,
        config: &crate::orchestration::workflow::definition::CsvExporterConfig,
        actual_output_path: &str,
        rows: &[serde_json::Value],
        columns: &[String],
    ) -> Result<usize> {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(actual_output_path)
            .with_context(|| format!("Failed to create output file: {}", actual_output_path))?;
        let mut writer = csv::WriterBuilder::new()
            .delimiter(config.delimiter.unwrap_or(',') as u8)
            .from_writer(BufWriter::new(file));

        if config.include_header {
            writer
                .write_record(columns)
                .context("Failed to write CSV header")?;
        }

        let mut rows_written = 0;
        for row in rows {
            let record: Vec<String> = columns
                .iter()
                .map(|col| {
                    row.get(col)
                        .map(|value| match value {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Null => String::new(),
                            _ => value.to_string(),
                        })
                        .unwrap_or_default()
                })
                .collect();

            writer
                .write_record(&record)
                .context("Failed to write CSV record")?;
            rows_written += 1;
        }

        writer.flush().context("Failed to flush CSV writer")?;
        Ok(rows_written)
    }

    fn execute_csv_export_batch_frame(
        &self,
        operator: &CsvExportBatchOperator,
        batch_config: &crate::orchestration::workflow::definition::CsvExporterConfig,
        frame: BatchFrame,
    ) -> Result<CsvExportBatchExecution> {
        let columns = frame
            .schema()
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        let rows_written = operator.execute(&frame, batch_config)?;

        Ok(CsvExportBatchExecution {
            rows_written,
            columns,
            frame,
        })
    }
}
