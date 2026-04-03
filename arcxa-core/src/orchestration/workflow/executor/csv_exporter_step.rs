use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::utilities::parse_row_id_key;
use super::{ExecutionContext, WorkflowExecutor};
use crate::core::lineage::row_level::{RowId, RowLineageEvent, RowTransformation};
use uuid::Uuid;

impl WorkflowExecutor {
    /// Execute CSV exporter step - write data to CSV file
    pub(super) async fn execute_csv_exporter(
        &self,
        config: &crate::orchestration::workflow::definition::CsvExporterConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        let user_path = Path::new(&config.output_path);
        let directory = user_path
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let extension = user_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("csv");
        let base_name = user_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("output");

        let unique_id = context
            .workflow_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let unique_filename = format!("{}_{}.{}", base_name, unique_id, extension);
        let actual_output_path = Path::new(&directory).join(&unique_filename);
        let actual_output_path_str = actual_output_path.to_string_lossy().to_string();

        tracing::info!(
            "Executing CSV exporter: user_requested={}, actual_output={}, unique_id={}",
            config.output_path,
            actual_output_path_str,
            unique_id
        );

        let row_count = self.get_context_row_count(context)?;
        tracing::info!("CSV exporter: Context exposes {} rows", row_count);

        if row_count == 0 {
            tracing::warn!("CSV exporter: No rows found in context, returning empty result");
            tracing::warn!(
                "CSV exporter: working_data keys: {:?}",
                context
                    .working_data
                    .as_object()
                    .map(|object| object.keys().collect::<Vec<_>>())
            );
            tracing::warn!(
                "CSV exporter: step_outputs keys: {:?}",
                context.step_outputs.keys().collect::<Vec<_>>()
            );
            return Ok((
                true,
                serde_json::json!({
                    "_output_path": actual_output_path_str,
                    "_requested_path": config.output_path,
                    "_unique_id": unique_id,
                    "_rows_written": 0,
                }),
                1.0,
            ));
        }

        let has_lineage = self.lineage_tracker.is_some();
        if let Some(ref tracker) = context.progress_tracker {
            tracker.set_total_rows(row_count as u64);
        }

        tracing::info!("CSV exporter: Processing {} rows", row_count);

        let (rows_written, columns, lineage_rows, memory_bytes) = if let Some(batch_export) =
            self.try_execute_csv_export_batch(context, config, &actual_output_path_str)?
        {
            let lineage_rows = if has_lineage {
                Some(batch_export.frame.to_object_rows()?)
            } else {
                None
            };

            (
                batch_export.rows_written,
                batch_export.columns,
                lineage_rows,
                batch_export.frame.estimated_size_bytes(),
            )
        } else {
            let rows = self.get_rows_from_context(context)?;
            let columns: Vec<String> = rows
                .first()
                .and_then(|row| row.as_object())
                .map(|object| object.keys().cloned().collect())
                .unwrap_or_default();
            let rows_written =
                self.write_csv_export_rows(config, &actual_output_path_str, &rows, &columns)?;
            let memory_bytes = Self::estimate_json_memory(&serde_json::Value::Array(rows.clone()));
            let lineage_rows = if has_lineage {
                Some(self.get_context_object_rows(context)?)
            } else {
                None
            };

            (rows_written, columns, lineage_rows, memory_bytes)
        };

        let mut lineage_events = Vec::new();
        let step_id = if has_lineage {
            context
                .row_lineage
                .as_ref()
                .and_then(|ctx| ctx.current_step_id.clone())
        } else {
            None
        };
        let tenant_id = context
            .metadata
            .get("tenant_id")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let job_id = context
            .metadata
            .get("job_id")
            .cloned()
            .unwrap_or_else(|| "csv_export".to_string());

        let extract_row_id = |row: &serde_json::Map<String, serde_json::Value>| -> Option<RowId> {
            row.get("_row_id")
                .and_then(|value| value.as_str())
                .and_then(parse_row_id_key)
        };

        if let Some(rows) = lineage_rows.as_ref() {
            for (row_index, row) in rows.iter().enumerate() {
                if row_index > 0 && row_index % context.resource_limits.yield_interval == 0 {
                    tokio::task::yield_now().await;

                    if let Some(ref tracker) = context.progress_tracker {
                        tracker.update_rows_processed(row_index as u64);
                    }

                    if let Some(ref token) = context.cancellation_token {
                        if token.is_cancelled() {
                            tracing::warn!("CSV exporter cancelled after {} rows", row_index);
                            anyhow::bail!("Workflow execution cancelled");
                        }
                    }

                    tracing::debug!(
                        "CSV exporter yielded after {} rows ({:.1}% complete)",
                        row_index,
                        (row_index as f64 / row_count as f64) * 100.0
                    );
                }

                if has_lineage {
                    if let Some(source_row_id) = extract_row_id(row) {
                        let output_row_id =
                            RowId::csv(&actual_output_path_str, (row_index + 2) as u64);

                        let mut event = RowLineageEvent::success_with_step(
                            source_row_id,
                            format!("batch_{}", uuid::Uuid::new_v4()),
                            job_id.clone(),
                            step_id.clone(),
                            actual_output_path_str.clone(),
                            tenant_id.clone(),
                        );

                        event.output_row_id = Some(output_row_id);

                        let mut transformation = RowTransformation::new(
                            "csv_export".to_string(),
                            vec!["_row".to_string()],
                        );
                        let mut after_values = HashMap::new();
                        after_values.insert(
                            "output_path".to_string(),
                            serde_json::json!(actual_output_path_str),
                        );
                        after_values
                            .insert("output_line".to_string(), serde_json::json!(row_index + 1));
                        transformation.after_values = Some(after_values);
                        event.add_transformation(transformation);

                        lineage_events.push(event);
                    }
                }
            }
        } else if let Some(ref tracker) = context.progress_tracker {
            tracker.update_rows_processed(row_count as u64);
        }

        if !lineage_events.is_empty() {
            tracing::info!(
                "CSV exporter: Recording {} lineage events",
                lineage_events.len()
            );
            if let Some(tracker) = &self.lineage_tracker {
                tracker
                    .record_row_lineage_batch(lineage_events)
                    .await
                    .unwrap_or_else(|error| {
                        tracing::warn!("Failed to record CSV export lineage: {}", error);
                    });
            }
        }

        let memory_mb = memory_bytes as f64 / 1_000_000.0;

        tracing::info!(
            target: "workflow_memory",
            memory_bytes = memory_bytes,
            memory_mb = memory_mb,
            row_count = row_count,
            step = "csv_export",
            output_file = %actual_output_path_str,
            "Memory usage during CSV export ({:.2} MB)",
            memory_mb
        );

        tracing::info!(
            "CSV export complete: {} rows written to {} ({:.2} MB)",
            row_count,
            actual_output_path_str,
            memory_mb
        );

        let modifications = vec![serde_json::json!({
            "field_name": "_export",
            "old_value": serde_json::Value::Null,
            "new_value": actual_output_path_str.clone(),
            "is_reversible": false,
            "operations": rows_written,
            "metadata": {
                "output_path": actual_output_path_str.clone(),
                "requested_path": config.output_path.clone(),
                "rows_written": rows_written,
                "columns_exported": columns.len(),
            }
        })];

        Ok((
            true,
            serde_json::json!({
                "_output_path": actual_output_path_str,
                "_requested_path": config.output_path,
                "_unique_id": unique_id,
                "_rows_written": rows_written,
                "_columns": columns,
                "_modifications": modifications,
            }),
            1.0,
        ))
    }
}
