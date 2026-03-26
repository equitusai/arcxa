use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::utilities::parse_row_id_key;
use super::{build_rows_output, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor};
use crate::core::lineage::row_level::{RowId, RowLineageEvent, RowTransformation};
use crate::orchestration::workflow::lineage_tracker::{RowTransformationEvent, TransformationType};

impl WorkflowExecutor {
    /// Execute deduplicator step - remove duplicate records
    pub(super) async fn execute_deduplicator(
        &self,
        config: &crate::orchestration::workflow::definition::DeduplicatorConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        use crate::orchestration::workflow::definition::{DedupMethod, KeepStrategy};

        tracing::info!(
            "Executing deduplicator: method={:?}, keys={:?}",
            config.method,
            config.key_fields
        );

        tracing::info!("DEDUP: Starting deduplication step");
        let rows = self.get_rows_from_context(context)?;
        let original_count = rows.len();
        tracing::info!("DEDUP: Retrieved {} rows from context", original_count);

        if context.row_lineage.is_none() && self.lineage_tracker.is_none() {
            if let Some(batch_result) =
                self.try_execute_deduplicator_batch(context, config, &rows)?
            {
                return Ok(batch_result);
            }
        }

        let mut seen_keys: HashMap<String, Vec<usize>> = HashMap::new();
        let mut deduped_rows: Vec<serde_json::Value> = Vec::new();
        let mut duplicate_groups: Vec<(RowId, Vec<RowId>)> = Vec::new();
        let mut lineage_events = Vec::new();

        tracing::info!("DEDUP: Starting first pass - building dedup keys");
        let mut missing_field_warnings = HashSet::new();
        for (idx, row) in rows.iter().enumerate() {
            if idx > 0 && idx % context.resource_limits.yield_interval == 0 {
                tokio::task::yield_now().await;
                tracing::debug!(
                    "Deduplicator yielded after {} rows ({:.1}% complete)",
                    idx,
                    (idx as f64 / original_count as f64) * 100.0
                );
            }

            if idx > 0 && idx % 100_000 == 0 {
                tracing::info!(
                    "DEDUP: Processed {}/{} rows ({:.1}%)",
                    idx,
                    original_count,
                    (idx as f64 / original_count as f64) * 100.0
                );
            }

            let key = config
                .key_fields
                .iter()
                .map(|field| {
                    let field_exists = row.get(field).is_some();
                    let value = row
                        .get(field)
                        .map(|value| match value {
                            serde_json::Value::String(string) => string.clone(),
                            _ => value.to_string(),
                        })
                        .unwrap_or_default();

                    if !field_exists && !missing_field_warnings.contains(field) {
                        missing_field_warnings.insert(field.clone());
                        tracing::warn!(
                            "DEDUPLICATOR VALIDATION: Field '{}' not found in rows! This may indicate a data flow issue. \
                             Check if the semantic mapper is outputting field names correctly. \
                             Available fields in first row: {:?}",
                            field,
                            row.as_object()
                                .map(|object| object.keys().collect::<Vec<_>>())
                                .unwrap_or_default()
                        );
                    }

                    if idx < 5 {
                        tracing::info!(
                            "ROW {}: field='{}' value='{}' (found={})",
                            idx,
                            field,
                            value,
                            field_exists
                        );
                        if idx == 0 {
                            tracing::info!(
                                "ROW 0 ALL FIELDS: {:?}",
                                row.as_object().map(|object| object.keys().collect::<Vec<_>>())
                            );
                        }
                    }
                    value
                })
                .collect::<Vec<_>>()
                .join("|");

            let normalized_key = match &config.method {
                DedupMethod::Exact => key.clone(),
                DedupMethod::Fuzzy { algorithm: _ } => key.to_lowercase().trim().to_string(),
                DedupMethod::Semantic { model: _ } => key.clone(),
            };

            if idx < 5 {
                tracing::info!("ROW {}: normalized_key='{}'", idx, normalized_key);
            }

            seen_keys
                .entry(normalized_key)
                .or_insert_with(Vec::new)
                .push(idx);
        }

        tracing::info!("DEDUP: First pass complete");
        tracing::info!(
            "Deduplication: {} input rows, {} unique keys, {} groups with duplicates",
            original_count,
            seen_keys.len(),
            seen_keys.values().filter(|group| group.len() > 1).count()
        );

        if !missing_field_warnings.is_empty() {
            let empty_key_count = seen_keys.get("||").map(|group| group.len()).unwrap_or(0);
            if empty_key_count > 1 {
                tracing::error!(
                    "DEDUPLICATOR CRITICAL: {} records collapsed to empty key '||' due to missing fields: {:?}. \
                     This indicates a critical data flow issue - likely the semantic mapper is outputting \
                     field names that don't match the deduplicator configuration.",
                    empty_key_count,
                    missing_field_warnings
                );
            }
        }

        tracing::info!("DEDUP: Starting second pass - applying keep strategy and lineage tracking");
        let has_lineage = context.row_lineage.is_some() || self.lineage_tracker.is_some();
        let tenant_id = context
            .metadata
            .get("tenant_id")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let job_id = context
            .metadata
            .get("job_id")
            .cloned()
            .unwrap_or_else(|| "deduplication".to_string());
        let batch_id = format!("batch_{}", uuid::Uuid::new_v4());

        for (_, group) in seen_keys {
            if group.len() == 1 {
                deduped_rows.push(rows[group[0]].clone());

                if has_lineage {
                    let extract_row_id = |row: &serde_json::Value| -> Option<RowId> {
                        row.get("_row_id")
                            .and_then(|value| value.as_str())
                            .and_then(parse_row_id_key)
                    };

                    if let Some(row_id) = extract_row_id(&rows[group[0]]) {
                        let step_id = context
                            .row_lineage
                            .as_ref()
                            .and_then(|ctx| ctx.current_step_id.clone());

                        let mut event = RowLineageEvent::success_with_step(
                            row_id,
                            batch_id.clone(),
                            job_id.clone(),
                            step_id.clone(),
                            "deduplication_unique".to_string(),
                            tenant_id.clone(),
                        );

                        let mut transformation = RowTransformation::new(
                            "deduplication".to_string(),
                            vec!["_row".to_string()],
                        );
                        let mut after_values = HashMap::new();
                        after_values.insert("status".to_string(), serde_json::json!("unique"));
                        after_values.insert(
                            "strategy".to_string(),
                            serde_json::json!(format!("{:?}", config.keep)),
                        );
                        transformation.after_values = Some(after_values);
                        event.add_transformation(transformation);

                        lineage_events.push(event);
                    }
                }

                continue;
            }

            let (kept_idx, removed_indices) = match config.keep {
                KeepStrategy::First => {
                    let kept_idx = group[0];
                    let removed: Vec<usize> = group[1..].iter().copied().collect();
                    (kept_idx, removed)
                }
                KeepStrategy::Last => {
                    let kept_idx = *group.last().ok_or_else(|| {
                        anyhow::anyhow!("Deduplication group is empty (internal logic error)")
                    })?;
                    let removed: Vec<usize> = group[..group.len() - 1].iter().copied().collect();
                    (kept_idx, removed)
                }
                KeepStrategy::Merge | KeepStrategy::HighestQuality => {
                    let kept_idx = group[0];
                    let removed: Vec<usize> = group[1..].iter().copied().collect();
                    (kept_idx, removed)
                }
            };

            deduped_rows.push(rows[kept_idx].clone());

            if has_lineage {
                let extract_row_id = |row: &serde_json::Value| -> Option<RowId> {
                    row.get("_row_id")
                        .and_then(|value| value.as_str())
                        .and_then(parse_row_id_key)
                };

                if let Some(kept_row_id) = extract_row_id(&rows[kept_idx]) {
                    let removed_row_ids: Vec<RowId> = removed_indices
                        .iter()
                        .filter_map(|idx| rows.get(*idx).and_then(extract_row_id))
                        .collect();

                    if !removed_row_ids.is_empty() {
                        if let Some(tracker) = &self.lineage_tracker {
                            let transformation_event = RowTransformationEvent {
                                execution_id: context
                                    .metadata
                                    .get("execution_id")
                                    .cloned()
                                    .unwrap_or_else(|| format!("exec_{}", uuid::Uuid::new_v4())),
                                step_id: "deduplicator".to_string(),
                                step_type: "deduplication".to_string(),
                                source_rows: {
                                    let mut all = vec![kept_row_id.clone()];
                                    all.extend(removed_row_ids.clone());
                                    all
                                },
                                output_row: Some(kept_row_id.clone()),
                                transformation_type: TransformationType::Deduplication {
                                    kept_row: kept_row_id.clone(),
                                    removed_rows: removed_row_ids.clone(),
                                    strategy: format!("{:?}", config.keep),
                                },
                                metadata: serde_json::Map::new(),
                                timestamp: chrono::Utc::now(),
                            };

                            tracker
                                .record_row_transformation(transformation_event)
                                .await
                                .unwrap_or_else(|error| {
                                    tracing::warn!(
                                        "Failed to record row transformation: {}",
                                        error
                                    );
                                });
                        }

                        let step_id = context
                            .row_lineage
                            .as_ref()
                            .and_then(|ctx| ctx.current_step_id.clone());
                        let removed_count = removed_row_ids.len();

                        for removed_row_id in removed_row_ids {
                            let event = RowLineageEvent::filtered_with_step(
                                removed_row_id,
                                batch_id.clone(),
                                job_id.clone(),
                                step_id.clone(),
                                format!("Duplicate removed using {:?} strategy", config.keep),
                                "deduplication".to_string(),
                                tenant_id.clone(),
                            );
                            lineage_events.push(event);
                        }

                        let mut kept_event = RowLineageEvent::success_with_step(
                            kept_row_id.clone(),
                            batch_id.clone(),
                            job_id.clone(),
                            step_id.clone(),
                            "deduplication_kept".to_string(),
                            tenant_id.clone(),
                        );

                        let mut transformation = RowTransformation::new(
                            "deduplication".to_string(),
                            vec!["_row".to_string()],
                        );
                        let mut after_values = HashMap::new();
                        after_values.insert("status".to_string(), serde_json::json!("kept"));
                        after_values.insert(
                            "strategy".to_string(),
                            serde_json::json!(format!("{:?}", config.keep)),
                        );
                        after_values.insert(
                            "duplicates_removed".to_string(),
                            serde_json::json!(removed_count),
                        );
                        transformation.after_values = Some(after_values);
                        kept_event.add_transformation(transformation);

                        lineage_events.push(kept_event);
                    }
                }
            }
        }

        tracing::info!("DEDUP: Second pass complete, recording lineage events");
        if !lineage_events.is_empty() {
            tracing::info!("DEDUP: Recording {} lineage events", lineage_events.len());
            if let Some(tracker) = &self.lineage_tracker {
                tracker
                    .record_row_lineage_batch(lineage_events)
                    .await
                    .unwrap_or_else(|error| {
                        tracing::warn!("Failed to record row lineage: {}", error);
                    });
            }
        }

        let duplicate_count = original_count - deduped_rows.len();

        let deduped_json = serde_json::Value::Array(deduped_rows.clone());
        let memory_bytes = Self::estimate_json_memory(&deduped_json);
        let memory_mb = memory_bytes as f64 / 1_000_000.0;
        let memory_gb = memory_bytes as f64 / 1_000_000_000.0;
        let dedup_rate = (duplicate_count as f64 / original_count as f64) * 100.0;

        tracing::info!(
            target: "workflow_memory",
            memory_bytes = memory_bytes,
            memory_mb = memory_mb,
            memory_gb = memory_gb,
            row_count = deduped_rows.len(),
            original_count = original_count,
            duplicates_removed = duplicate_count,
            dedup_rate = dedup_rate,
            step = "deduplicator",
            "Memory usage after deduplication ({:.2} MB, {:.3} GB, {:.1}% dedup rate)",
            memory_mb,
            memory_gb,
            dedup_rate
        );

        tracing::info!(
            "Deduplication complete: {} -> {} rows ({} duplicates removed, {:.1}% dedup rate, {:.2} MB memory)",
            original_count,
            deduped_rows.len(),
            duplicate_count,
            dedup_rate,
            memory_mb
        );
        tracing::info!("DEDUP: Returning deduped rows to context");

        if context.resource_limits.enforce_limits {
            if let Some(max_rows) = context.resource_limits.max_rows {
                if deduped_rows.len() > max_rows {
                    tracing::warn!(
                        "Deduplicator output exceeded row limit. Result has {} rows, limit is {}. \
                         Consider increasing resource_limits.max_rows. Continuing anyway...",
                        deduped_rows.len(),
                        max_rows
                    );
                }
            }

            if let Some(max_mem) = context.resource_limits.max_memory_bytes {
                if memory_bytes > max_mem {
                    tracing::warn!(
                        "Deduplicator exceeded memory limit. Current: {:.2} GB, Limit: {:.2} GB. \
                         Consider increasing resource_limits.max_memory_bytes. Continuing anyway...",
                        memory_gb,
                        max_mem as f64 / 1_000_000_000.0
                    );
                }
            }
        }

        let modifications = vec![serde_json::json!({
            "field_name": "_deduplication",
            "old_value": original_count,
            "new_value": deduped_rows.len(),
            "is_reversible": false,
            "operations": duplicate_count,
            "metadata": {
                "method": format!("{:?}", config.method),
                "keep_strategy": format!("{:?}", config.keep),
                "key_fields": config.key_fields,
                "duplicates_removed": duplicate_count,
                "dedup_rate_percent": dedup_rate,
            }
        })];

        let deduped_count = deduped_rows.len();

        Ok(BatchStepExecutionResult::success(build_rows_output(
            deduped_rows,
            deduped_count,
            vec![
                (
                    "_original_count".to_string(),
                    serde_json::json!(original_count),
                ),
                (
                    "_duplicates_removed".to_string(),
                    serde_json::json!(duplicate_count),
                ),
                (
                    "_modifications".to_string(),
                    serde_json::Value::Array(modifications),
                ),
            ],
        )))
    }
}
