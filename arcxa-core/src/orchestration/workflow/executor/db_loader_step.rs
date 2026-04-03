use anyhow::{Context, Result};
use std::collections::HashMap;

use super::utilities::parse_row_id_key;
use super::{DbLoadResult, ExecutionContext, WorkflowExecutor};
use crate::core::lineage::row_level::{RowId, RowLineageEvent, RowTransformation};

impl WorkflowExecutor {
    /// Execute DB loader step - loads data to database via callback
    pub(super) async fn execute_db_loader(
        &self,
        config: &crate::orchestration::workflow::definition::DbLoaderConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        tracing::info!(
            "Executing DB loader: datasource={}, table={}",
            config.datasource_id,
            config.table_name
        );

        let row_count = self.get_context_row_count(context)?;

        if let Some(callback) = &self.db_loader_callback {
            tracing::info!(
                "Using DB loader callback to load {} rows to {}.{}",
                row_count,
                config.datasource_id,
                config.table_name
            );

            let mode_str = format!("{:?}", config.mode);
            let object_rows = self.get_context_object_rows(context)?;
            let callback_rows = if self.lineage_tracker.is_some() {
                object_rows.clone()
            } else {
                object_rows
            };

            let load_result = callback(
                &config.datasource_id,
                &config.table_name,
                callback_rows,
                &mode_str,
                config.key_fields.clone(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to load data to {}.{}",
                    config.datasource_id, config.table_name
                )
            })?;

            if self.lineage_tracker.is_some() {
                let lineage_rows = self.get_context_object_rows(context)?;
                self.record_db_load_lineage(context, config, &lineage_rows, &load_result)
                    .await;
            }

            tracing::info!(
                "Successfully loaded {} rows to {}.{}",
                load_result.rows_loaded,
                config.datasource_id,
                config.table_name
            );

            Ok((
                true,
                serde_json::json!({
                    "_datasource_id": config.datasource_id,
                    "_table_name": config.table_name,
                    "_rows_loaded": load_result.rows_loaded,
                    "_mode": mode_str,
                    "_status": "success",
                }),
                1.0,
            ))
        } else {
            tracing::warn!(
                "DB loader callback not set - {} rows would be loaded to {}.{}",
                row_count,
                config.datasource_id,
                config.table_name
            );

            Ok((
                true,
                serde_json::json!({
                    "_datasource_id": config.datasource_id,
                    "_table_name": config.table_name,
                    "_rows_to_load": row_count,
                    "_mode": format!("{:?}", config.mode),
                    "_status": "stub_implementation",
                }),
                1.0,
            ))
        }
    }

    async fn record_db_load_lineage(
        &self,
        context: &ExecutionContext,
        config: &crate::orchestration::workflow::definition::DbLoaderConfig,
        rows: &[serde_json::Map<String, serde_json::Value>],
        load_result: &DbLoadResult,
    ) {
        let Some(tracker) = &self.lineage_tracker else {
            return;
        };

        if load_result.output_row_ids.is_empty() {
            return;
        }

        let step_id = context
            .row_lineage
            .as_ref()
            .and_then(|ctx| ctx.current_step_id.clone());
        let tenant_id = context
            .metadata
            .get("tenant_id")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let job_id = context
            .metadata
            .get("job_id")
            .cloned()
            .unwrap_or_else(|| "db_loader".to_string());
        let output_location = format!("{}.{}", config.datasource_id, config.table_name);

        let mut lineage_events = Vec::new();
        for (row_index, row) in rows.iter().enumerate() {
            let source_row_id = extract_source_row_id(row);
            let output_row_id = load_result.output_row_ids.get(row_index).cloned().flatten();

            let (Some(source_row_id), Some(output_row_id)) = (source_row_id, output_row_id) else {
                continue;
            };

            let mut event = RowLineageEvent::success_with_step(
                source_row_id,
                format!("batch_{}", uuid::Uuid::new_v4()),
                job_id.clone(),
                step_id.clone(),
                output_location.clone(),
                tenant_id.clone(),
            );
            event.output_row_id = Some(output_row_id);

            let mut transformation =
                RowTransformation::new("db_load".to_string(), vec!["_row".to_string()]);
            let mut after_values = HashMap::new();
            after_values.insert(
                "datasource_id".to_string(),
                serde_json::json!(config.datasource_id),
            );
            after_values.insert(
                "table_name".to_string(),
                serde_json::json!(config.table_name),
            );
            after_values.insert(
                "mode".to_string(),
                serde_json::json!(format!("{:?}", config.mode)),
            );
            transformation.after_values = Some(after_values);
            event.add_transformation(transformation);

            lineage_events.push(event);
        }

        if !lineage_events.is_empty() {
            tracker
                .record_row_lineage_batch(lineage_events)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!("Failed to record DB load lineage: {}", error);
                });
        }
    }
}

fn extract_source_row_id(row: &serde_json::Map<String, serde_json::Value>) -> Option<RowId> {
    row.get("_row_id")
        .and_then(|value| value.as_str())
        .and_then(parse_row_id_key)
}
