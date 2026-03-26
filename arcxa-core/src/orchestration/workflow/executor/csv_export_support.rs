use anyhow::{Context, Result};

use super::{ExecutionContext, WorkflowExecutor};
use crate::orchestration::workflow::runtime::operators::CsvExportBatchOperator;

impl WorkflowExecutor {
    pub(super) fn try_execute_csv_export_batch(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::CsvExporterConfig,
        actual_output_path: &str,
        rows: &[serde_json::Value],
    ) -> Result<Option<usize>> {
        let operator = CsvExportBatchOperator;
        let batch_config = crate::orchestration::workflow::definition::CsvExporterConfig {
            output_path: actual_output_path.to_string(),
            delimiter: config.delimiter,
            include_header: config.include_header,
            encoding: config.encoding.clone(),
        };

        self.try_with_context_batch_frame(context, rows, |frame| {
            operator.execute(&frame, &batch_config)
        })
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
}
