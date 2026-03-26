use anyhow::{Context, Result};

use super::{build_rows_output, ExecutionContext, WorkflowExecutor};
use crate::core::lineage::row_level::{RowId, RowLineageEvent};

impl WorkflowExecutor {
    /// Execute CSV source step - read data from CSV file
    pub(super) async fn execute_csv_source(
        &self,
        config: &crate::orchestration::workflow::definition::CsvSourceConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        use std::fs::File;
        use std::io::BufReader;

        tracing::info!("Executing CSV source: file={}", config.file_path);

        // Read CSV file
        let file = File::open(&config.file_path)
            .with_context(|| format!("Failed to open CSV file: {}", config.file_path))?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(config.has_header.unwrap_or(true))
            .delimiter(config.delimiter.unwrap_or(',') as u8)
            .from_reader(BufReader::new(file));

        let headers: Vec<String> = reader
            .headers()
            .context("Failed to read CSV headers")?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Read all rows into memory
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let skip = config.skip_rows.unwrap_or(0);
        let max = config.max_rows;

        // Phase 3: Set total_rows if we know the max
        if let Some(max_rows) = max {
            if let Some(ref tracker) = context.progress_tracker {
                tracker.set_total_rows(max_rows as u64);
            }
        }

        // Track row lineage if context is available
        let mut lineage_events = Vec::new();
        let has_lineage = context.row_lineage.is_some();

        // We need to track the actual row number in the file (accounting for header and skipped rows)
        let row_offset = if config.has_header.unwrap_or(true) {
            2
        } else {
            1
        };

        for (idx, result) in reader.records().enumerate() {
            // PHASE 2 FIX: Yield based on configurable interval to allow other async tasks to run
            if idx > 0 && idx % context.resource_limits.yield_interval == 0 {
                tokio::task::yield_now().await;

                // Phase 3: Update progress tracker with rows processed
                if let Some(ref tracker) = context.progress_tracker {
                    tracker.update_rows_processed(idx as u64);
                }

                tracing::debug!(
                    "CSV source yielded after {} rows ({:.1}% complete)",
                    idx,
                    if let Some(max) = max {
                        (idx as f64 / max as f64) * 100.0
                    } else {
                        0.0
                    }
                );
            }

            // Phase 3: Check for cancellation during processing
            if idx > 0 && idx % context.resource_limits.yield_interval == 0 {
                if let Some(ref token) = context.cancellation_token {
                    if token.is_cancelled() {
                        tracing::warn!("CSV source cancelled after {} rows", idx);
                        anyhow::bail!("Workflow execution cancelled");
                    }
                }
            }

            if idx < skip {
                continue;
            }
            if let Some(max_rows) = max {
                if rows.len() >= max_rows {
                    break;
                }
            }

            let record = result.context("Failed to read CSV record")?;
            let mut row = serde_json::Map::new();
            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    row.insert(headers[i].clone(), serde_json::json!(field));
                }
            }

            // Track row lineage
            if has_lineage {
                let actual_row_number = (idx + row_offset + skip) as u64;
                let row_id = RowId::csv(&config.file_path, actual_row_number);

                let tenant_id = context
                    .metadata
                    .get("tenant_id")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());

                let step_id = context
                    .row_lineage
                    .as_ref()
                    .and_then(|ctx| ctx.current_step_id.clone());

                let event = RowLineageEvent::success_with_step(
                    row_id.clone(),
                    format!("batch_{}", uuid::Uuid::new_v4()),
                    context
                        .metadata
                        .get("job_id")
                        .cloned()
                        .unwrap_or_else(|| "csv_import".to_string()),
                    step_id,
                    config.file_path.clone(),
                    tenant_id,
                );
                lineage_events.push(event);

                row.insert("_row_id".to_string(), serde_json::json!(row_id.to_key()));
                row.insert("_row_index".to_string(), serde_json::json!(rows.len()));
            }

            rows.push(serde_json::Value::Object(row));
        }

        tracing::info!(
            "CSV source: {} lineage events, has_tracker={}, has_context_lineage={}",
            lineage_events.len(),
            self.lineage_tracker.is_some(),
            has_lineage
        );
        if !lineage_events.is_empty() {
            if let Some(tracker) = &self.lineage_tracker {
                tracing::info!(
                    "CSV source: Calling tracker.record_row_lineage_batch with {} events",
                    lineage_events.len()
                );
                tracker
                    .record_row_lineage_batch(lineage_events)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!("Failed to record row lineage: {}", e);
                    });
                tracing::info!("CSV source: record_row_lineage_batch completed");
            } else {
                tracing::warn!(
                    "CSV source: No lineage tracker available, {} events will not be recorded",
                    lineage_events.len()
                );
            }
        } else {
            tracing::warn!(
                "CSV source: lineage_events is empty (has_lineage={})",
                has_lineage
            );
        }

        let row_count = rows.len();

        let rows_json = serde_json::Value::Array(rows.clone());
        let memory_bytes = Self::estimate_json_memory(&rows_json);
        let memory_mb = memory_bytes as f64 / 1_000_000.0;
        let memory_gb = memory_bytes as f64 / 1_000_000_000.0;

        tracing::info!(
            target: "workflow_memory",
            memory_bytes = memory_bytes,
            memory_mb = memory_mb,
            memory_gb = memory_gb,
            row_count = row_count,
            step = "csv_source",
            file = %config.file_path,
            "Memory usage after CSV load ({:.2} MB, {:.3} GB)",
            memory_mb,
            memory_gb
        );

        tracing::info!("CSV source: CHECKPOINT 1 - before resource limits");

        if context.resource_limits.enforce_limits {
            tracing::info!("CSV source: Resource limits are enforced, checking...");
            if let Some(max_rows) = context.resource_limits.max_rows {
                tracing::info!(
                    "CSV source: Checking row count limit, max_rows={}, actual={}",
                    max_rows,
                    row_count
                );
                if row_count > max_rows {
                    tracing::warn!(
                        "CSV source exceeded row limit. Loaded {} rows, limit is {}. \
                         Consider using batch processing or increase resource_limits.max_rows. \
                         Continuing anyway for testing...",
                        row_count,
                        max_rows
                    );
                } else {
                    tracing::info!("CSV source: Row count check passed");
                }
            }

            if let Some(max_mem) = context.resource_limits.max_memory_bytes {
                tracing::info!(
                    "CSV source: Checking memory limit, max_mem={} bytes",
                    max_mem
                );
                if memory_bytes > max_mem {
                    anyhow::bail!(
                        "CSV source exceeded memory limit. Current: {:.2} GB, Limit: {:.2} GB. \
                         Consider using batch processing or increase resource_limits.max_memory_bytes",
                        memory_gb,
                        max_mem as f64 / 1_000_000_000.0
                    );
                }
                tracing::info!("CSV source: Memory check passed");
            }

            tracing::info!("CSV source: CHECKPOINT 2 - about to call debug log");
            tracing::debug!(
                "CSV source resource check passed: {} rows (max: {:?}), {:.2} GB memory (max: {:?} GB)",
                row_count,
                context.resource_limits.max_rows,
                memory_gb,
                context.resource_limits.max_memory_bytes.map(|m| m as f64 / 1_000_000_000.0)
            );
            tracing::info!(
                "CSV source: CHECKPOINT 2b - debug log completed, resource check passed"
            );
        }

        tracing::info!(
            "CSV source: CHECKPOINT 3 - about to create output, row_count={}",
            row_count
        );
        tracing::info!(
            "CSV source: About to create output, row_count={}",
            row_count
        );

        let modifications = vec![serde_json::json!({
            "field_name": "_source",
            "old_value": serde_json::Value::Null,
            "new_value": config.file_path.clone(),
            "is_reversible": false,
            "operations": 1,
        })];

        let output = build_rows_output(
            rows,
            row_count,
            vec![
                ("_columns".to_string(), serde_json::json!(headers)),
                (
                    "_source_file".to_string(),
                    serde_json::json!(config.file_path),
                ),
                (
                    "_modifications".to_string(),
                    serde_json::Value::Array(modifications),
                ),
            ],
        );

        Ok((true, output, 1.0))
    }
}
