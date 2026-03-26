use anyhow::{Context, Result};

use super::{ExecutionContext, WorkflowExecutor};

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

        let rows = self.get_rows_from_context(context)?;
        let row_count = rows.len();

        if let Some(callback) = &self.db_loader_callback {
            tracing::info!(
                "Using DB loader callback to load {} rows to {}.{}",
                row_count,
                config.datasource_id,
                config.table_name
            );

            let mode_str = format!("{:?}", config.mode);

            let rows_vec: Vec<serde_json::Map<String, serde_json::Value>> = rows
                .into_iter()
                .filter_map(|row| row.as_object().cloned())
                .collect();

            let rows_loaded = callback(
                &config.datasource_id,
                &config.table_name,
                rows_vec,
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

            tracing::info!(
                "Successfully loaded {} rows to {}.{}",
                rows_loaded,
                config.datasource_id,
                config.table_name
            );

            Ok((
                true,
                serde_json::json!({
                    "_datasource_id": config.datasource_id,
                    "_table_name": config.table_name,
                    "_rows_loaded": rows_loaded,
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
}
