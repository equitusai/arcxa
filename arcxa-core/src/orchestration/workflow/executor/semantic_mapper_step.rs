use anyhow::Result;

use super::utilities::parse_row_id_key;
use super::{ExecutionContext, WorkflowExecutor};
use crate::core::lineage::row_level::{RowId, RowLineageEvent, RowTransformation};

impl WorkflowExecutor {
    /// Execute semantic mapper step
    ///
    /// Uses the transformer callback if available for real ontology mapping
    /// with column lineage support. Falls back to stub implementation otherwise.
    pub(super) async fn execute_semantic_mapper(
        &self,
        config: &crate::orchestration::workflow::definition::SemanticMapperConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        tracing::info!(
            "TRACE: execute_semantic_mapper ENTRY - ontology={:?}, mode={:?}, has_callback={}",
            config.target_ontology,
            config.mapping_mode,
            self.transformer_callback.is_some()
        );

        tracing::info!("TRACE: Calling get_rows_from_context...");
        let rows = match self.get_rows_from_context(context) {
            Ok(r) => {
                tracing::info!("TRACE: get_rows_from_context returned {} rows", r.len());
                r
            }
            Err(e) => {
                tracing::error!("TRACE: get_rows_from_context FAILED: {:?}", e);
                return Err(e);
            }
        };

        let table_from_row_id = |row_id_str: &str| -> Option<String> {
            let mut parts = row_id_str.splitn(3, ':');
            let source_type = parts.next()?;
            let source_id = parts.next()?;

            if source_type == "csv" {
                return std::path::Path::new(source_id)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());
            }

            Some(source_id.to_string())
        };

        let table_name = config
            .table_name
            .clone()
            .or_else(|| {
                context
                    .working_data
                    .get("_table_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                rows.get(0)
                    .and_then(|row| row.get("_row_id").and_then(|v| v.as_str()))
                    .and_then(|s| table_from_row_id(s))
            })
            .or_else(|| context.workflow_id.clone())
            .unwrap_or_else(|| "source_data".to_string());

        let source_id = config
            .source_id
            .clone()
            .or_else(|| {
                context
                    .working_data
                    .get("_datasource_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| context.metadata.get("datasource_id").cloned())
            .unwrap_or_else(|| "default".to_string());

        let mut transformer_config = serde_json::json!({
            "source_id": source_id,
            "table_name": table_name,
            "target_ontology": config.target_ontology,
            "auto_approve_threshold": config.auto_approve_threshold,
            "mapping_mode": format!("{:?}", config.mapping_mode),
            "entity_uri": config.entity_uri,
        });

        if let Some(ref session_id) = config.mapping_session_id {
            if let Some(obj) = transformer_config.as_object_mut() {
                obj.insert("session_id".to_string(), serde_json::json!(session_id));
            }
        }

        if let Some(ref row_lineage) = context.row_lineage {
            if let Some(obj) = transformer_config.as_object_mut() {
                obj.insert("job_id".to_string(), serde_json::json!(row_lineage.job_id));
                obj.insert(
                    "tenant_id".to_string(),
                    serde_json::json!(row_lineage.tenant_id),
                );
                obj.insert(
                    "execution_id".to_string(),
                    serde_json::json!(row_lineage.execution_id),
                );
            }
        }

        let mut data = serde_json::json!({
            "rows": rows,
        });

        if let Some(schema) = context
            .working_data
            .get("schema")
            .or_else(|| context.working_data.get("_schema"))
            .cloned()
        {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("schema".to_string(), schema);
            }
        }

        if let Some(callback) = &self.transformer_callback {
            tracing::info!(
                "TRACE: Semantic mapper has transformer callback available, invoking..."
            );
            let result = callback("ontology_map", &transformer_config, &mut data, context).await;

            match result {
                Ok(()) => {
                    tracing::info!(
                        "TRACE: Semantic mapper executed via transformer callback successfully"
                    );
                    let (mapped_rows, ontology_mapping, modifications) =
                        if let Some(data_object) = data.as_object_mut() {
                            (
                                data_object.remove("rows").unwrap_or(serde_json::json!([])),
                                data_object.remove("ontology_mapping"),
                                data_object.remove("_modifications"),
                            )
                        } else {
                            (serde_json::json!([]), None, None)
                        };
                    let row_count = mapped_rows.as_array().map(|a| a.len()).unwrap_or(0);

                    let memory_bytes = Self::estimate_json_memory(&mapped_rows);
                    let memory_mb = memory_bytes as f64 / 1_000_000.0;
                    let memory_gb = memory_bytes as f64 / 1_000_000_000.0;

                    tracing::info!(
                        target: "workflow_memory",
                        memory_bytes = memory_bytes,
                        memory_mb = memory_mb,
                        memory_gb = memory_gb,
                        row_count = row_count,
                        step = "semantic_mapper",
                        ontology = ?config.target_ontology,
                        "Memory usage after semantic mapping ({:.2} MB, {:.3} GB)",
                        memory_mb,
                        memory_gb
                    );

                    let mut lineage_events = Vec::new();
                    let has_lineage = self.lineage_tracker.is_some();

                    if has_lineage {
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
                            .unwrap_or_else(|| "semantic_mapping".to_string());

                        let extract_row_id = |row: &serde_json::Value| -> Option<RowId> {
                            row.get("_row_id")
                                .or_else(|| row.get("unmapped._row_id"))
                                .and_then(|v| v.as_str())
                                .and_then(parse_row_id_key)
                        };

                        let modification_count = modifications
                            .as_ref()
                            .and_then(|m| m.as_array())
                            .map(|items| items.len())
                            .unwrap_or(0);

                        tracing::info!(
                            "Semantic mapper: Found {} field modifications for row lineage",
                            modification_count
                        );

                        if let Some(rows_array) = mapped_rows.as_array() {
                            tracing::info!(
                                "Semantic mapper: Processing {} rows for row lineage",
                                rows_array.len()
                            );
                            let mut rows_with_id = 0;
                            for row in rows_array {
                                if let Some(row_id) = extract_row_id(row) {
                                    rows_with_id += 1;
                                    let mut event = RowLineageEvent::success_with_step(
                                        row_id,
                                        format!("batch_{}", uuid::Uuid::new_v4()),
                                        job_id.clone(),
                                        step_id.clone(),
                                        format!(
                                            "semantic_mapper_{}",
                                            config.target_ontology.join(",")
                                        ),
                                        tenant_id.clone(),
                                    );

                                    let transformation = RowTransformation::new(
                                        format!(
                                            "ontology_mapping:{}",
                                            config.target_ontology.join(",")
                                        ),
                                        vec!["*".to_string()],
                                    );

                                    event.add_transformation(transformation);

                                    tracing::debug!(
                                        "Row lineage: Added summary transformation for {} (ontology: {}, {} field mappings tracked in RDF)",
                                        event.row_id,
                                        config.target_ontology.join(","),
                                        modification_count
                                    );

                                    lineage_events.push(event);
                                }
                            }
                            tracing::info!(
                                "Semantic mapper: Found {} rows with _row_id out of {} total rows",
                                rows_with_id,
                                rows_array.len()
                            );
                        } else {
                            tracing::warn!("Semantic mapper: mapped_rows is not an array!");
                        }

                        if !lineage_events.is_empty() {
                            tracing::info!(
                                "Semantic mapper: Recording {} lineage events",
                                lineage_events.len()
                            );
                            if let Some(tracker) = &self.lineage_tracker {
                                tracker
                                    .record_row_lineage_batch(lineage_events)
                                    .await
                                    .unwrap_or_else(|e| {
                                        tracing::warn!(
                                            "Failed to record semantic mapper lineage: {}",
                                            e
                                        );
                                    });
                            }
                        }
                    }

                    return Ok((
                        true,
                        serde_json::json!({
                            "_rows": mapped_rows,
                            "_row_count": row_count,
                            "ontology_mapping": ontology_mapping,
                            "_modifications": modifications,
                        }),
                        1.0,
                    ));
                }
                Err(e) => {
                    tracing::warn!(
                        "TRACE: Transformer callback failed: {}, falling back to stub",
                        e
                    );
                }
            }
        } else {
            tracing::warn!("TRACE: Semantic mapper NO transformer callback available!");
        }

        tracing::warn!("TRACE: Semantic mapper falling back to stub implementation");

        let rows_json = serde_json::Value::Array(rows.clone());
        let memory_bytes = Self::estimate_json_memory(&rows_json);
        let memory_mb = memory_bytes as f64 / 1_000_000.0;

        tracing::info!(
            target: "workflow_memory",
            memory_bytes = memory_bytes,
            memory_mb = memory_mb,
            row_count = rows.len(),
            step = "semantic_mapper_stub",
            "Memory usage (stub implementation, {:.2} MB)",
            memory_mb
        );

        let mut lineage_events = Vec::new();
        let has_lineage = self.lineage_tracker.is_some();

        if has_lineage {
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
                .unwrap_or_else(|| "semantic_mapping".to_string());

            let extract_row_id = |row: &serde_json::Value| -> Option<RowId> {
                row.get("_row_id")
                    .and_then(|v| v.as_str())
                    .and_then(parse_row_id_key)
            };

            for row in &rows {
                if let Some(row_id) = extract_row_id(row) {
                    let mut event = RowLineageEvent::success_with_step(
                        row_id,
                        format!("batch_{}", uuid::Uuid::new_v4()),
                        job_id.clone(),
                        step_id.clone(),
                        format!("semantic_mapper_{}_stub", config.target_ontology.join(",")),
                        tenant_id.clone(),
                    );

                    let transformation = RowTransformation::new(
                        "ontology_mapping_stub".to_string(),
                        vec!["all_fields".to_string()],
                    );
                    event.add_transformation(transformation);

                    lineage_events.push(event);
                }
            }

            if !lineage_events.is_empty() {
                tracing::info!(
                    "Semantic mapper (stub): Recording {} lineage events",
                    lineage_events.len()
                );
                if let Some(tracker) = &self.lineage_tracker {
                    tracker
                        .record_row_lineage_batch(lineage_events)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                "Failed to record semantic mapper (stub) lineage: {}",
                                e
                            );
                        });
                }
            }
        }

        Ok((
            true,
            serde_json::json!({
                "_target_ontology": config.target_ontology,
                "_mapping_mode": format!("{:?}", config.mapping_mode),
                "_status": "stub_implementation",
                "_rows": rows,
            }),
            0.0,
        ))
    }
}
