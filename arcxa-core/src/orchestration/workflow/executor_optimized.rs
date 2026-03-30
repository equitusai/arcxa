//! Optimized executor implementation using row storage
//!
//! This module shows how to migrate executor steps to use the new
//! row storage system, eliminating expensive clone operations.

#[cfg(feature = "workflow-storage")]
use super::definition::{
    AggregatorConfig, CsvExporterConfig, DataValidatorConfig, DeduplicatorConfig,
    SemanticMapperConfig,
};
#[cfg(feature = "workflow-storage")]
use super::error::{Result, WorkflowError};
#[cfg(feature = "workflow-storage")]
use super::execution_context_v2::ExecutionContextV2;
#[cfg(feature = "workflow-storage")]
use super::row_storage::RowAccessor;
#[cfg(feature = "workflow-storage")]
use super::row_storage::StorageManager;
#[cfg(feature = "workflow-storage")]
use super::runtime::operators::{
    AggregatorBatchOperator, CsvExportBatchOperator, DataValidatorBatchOperator,
    DeduplicatorBatchOperator, FieldTransformerBatchOperator, SemanticMapperBatchOperator,
};
#[cfg(feature = "workflow-storage")]
use super::streaming_deduplicator::{StreamingDedupConfig, StreamingDeduplicator};
#[cfg(feature = "workflow-storage")]
use std::collections::HashSet;
#[cfg(feature = "workflow-storage")]
use std::sync::Arc;

/// Example of optimized step execution
#[cfg(feature = "workflow-storage")]
pub struct OptimizedStepExecutor {
    storage_manager: Arc<StorageManager>,
}

#[cfg(feature = "workflow-storage")]
impl OptimizedStepExecutor {
    pub fn new(storage_manager: Arc<StorageManager>) -> Self {
        Self { storage_manager }
    }

    fn build_runtime_metrics_value(
        &self,
        context: &ExecutionContextV2,
        input_rows: usize,
        output_rows: usize,
        materialization_count: usize,
    ) -> Result<serde_json::Value> {
        serde_json::to_value(context.build_runtime_step_metrics(
            input_rows,
            output_rows,
            materialization_count,
        ))
        .map_err(|error| WorkflowError::InvalidData(error.to_string()))
    }

    /// Optimized deduplicator that streams data
    pub async fn execute_deduplicator(
        &self,
        config: &DeduplicatorConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("DEDUP_OPTIMIZED: Starting deduplication with streaming");

        let row_accessor = context.get_rows()?;
        let original_count = row_accessor.len();

        tracing::info!("DEDUP_OPTIMIZED: Processing {} rows", original_count);

        let execution_path = if original_count > 100_000 {
            "streaming_disk"
        } else if original_count > 10_000 {
            "streaming_memory"
        } else {
            "batch_frame"
        };

        let deduped_count = if original_count > 100_000 {
            // Large dataset: use streaming with disk-backed seen keys
            let deduped_rows = self
                .streaming_deduplicate_large(&row_accessor, config)
                .await?;
            let deduped_count = deduped_rows.len();
            context.set_rows(deduped_rows)?;
            deduped_count
        } else if original_count > 10_000 {
            // Medium dataset: streaming with in-memory seen keys
            let deduped_rows = self.streaming_deduplicate_medium(&row_accessor, config)?;
            let deduped_count = deduped_rows.len();
            context.set_rows(deduped_rows)?;
            deduped_count
        } else {
            let frame = context.get_batch_frame()?;
            let operator = DeduplicatorBatchOperator;
            let deduped_frame = operator.execute(frame, config)?;
            let deduped_count = deduped_frame.row_count();
            context.set_batch_frame(deduped_frame)?;
            deduped_count
        };
        let duplicates_removed = original_count - deduped_count;

        // Track lineage if enabled
        if let Some(ref mut lineage) = context.row_lineage {
            // Record deduplication in lineage
            for i in 0..deduped_count {
                lineage.track_deduplication(i, vec![], &config.method.to_string());
            }
        }

        tracing::info!(
            "DEDUP_OPTIMIZED: Removed {} duplicates, {} rows remaining",
            duplicates_removed,
            deduped_count
        );

        Ok((
            true,
            serde_json::json!({
                "_row_count": deduped_count,
                "_original_count": original_count,
                "_duplicates_removed": duplicates_removed,
                "_execution_path": execution_path,
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_method": config.method.to_string(),
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    original_count,
                    deduped_count,
                    if execution_path == "row_json" { 1 } else { 0 },
                )?,
            }),
        ))
    }

    /// Streaming deduplication for large datasets (>100K rows)
    /// Uses advanced StreamingDeduplicator with Bloom filters, LRU cache, and RocksDB
    async fn streaming_deduplicate_large(
        &self,
        accessor: &RowAccessor,
        config: &DeduplicatorConfig,
    ) -> Result<Vec<serde_json::Value>> {
        tracing::info!(
            "DEDUP_OPTIMIZED: Using advanced streaming deduplicator with tiered storage"
        );

        // Create streaming dedup config with intelligent defaults based on dataset size
        let row_count = accessor.len();
        let streaming_config = StreamingDedupConfig {
            base: config.clone(),
            batch_size: 10_000, // 10K row batches
            cache_size: if row_count > 1_000_000 {
                100_000 // 100K cache for 1M+ rows
            } else {
                50_000 // 50K cache for 100K-1M rows
            },
            max_memory_bytes: if row_count > 1_000_000 {
                200_000_000 // 200MB for 1M+ rows
            } else {
                100_000_000 // 100MB for 100K-1M rows
            },
            bloom_expected_items: (row_count as f64 * 0.8) as usize, // Assume 80% unique
            bloom_false_positive_rate: 0.01,                         // 1% false positive rate
            parallel_processing: false, // Disable for now (not implemented yet)
            num_workers: 1,             // Single-threaded for now
        };

        // Create ExecutionContextV2 wrapper for the streaming deduplicator
        // We need to convert the RowAccessor data into the context format
        let rows = accessor.to_vec()?;
        let row_storage = super::row_storage::RowStorage::from_rows(rows)?;

        let mut temp_context = ExecutionContextV2 {
            input_data: serde_json::json!({}),
            working_data: serde_json::json!({}),
            row_storage: Some(row_storage),
            batch_frame: None,
            step_outputs: Default::default(),
            step_row_storage: Default::default(),
            metadata: Default::default(),
            row_lineage: None,
            workflow_id: None,
            execution_id: uuid::Uuid::new_v4().to_string(),
            current_step_id: Some("dedup".to_string()),
            resource_limits: Default::default(),
            storage_manager: Some(self.storage_manager.clone()),
            metrics: Default::default(),
        };

        // Create and execute streaming deduplicator with the advanced implementation
        let mut deduplicator = StreamingDeduplicator::new(
            streaming_config,
            &temp_context,
            None, // No lineage tracker for now
        )?;

        // Execute and get deduplicated rows directly
        let result = deduplicator.execute_and_get_rows(&temp_context).await?;

        tracing::info!(
            "DEDUP_OPTIMIZED: Advanced streaming deduplicator completed successfully. {} rows deduplicated",
            result.len()
        );

        Ok(result)
    }

    /// Streaming deduplication for medium datasets (10K-100K rows)
    fn streaming_deduplicate_medium(
        &self,
        accessor: &RowAccessor,
        config: &DeduplicatorConfig,
    ) -> Result<Vec<serde_json::Value>> {
        tracing::info!("DEDUP_OPTIMIZED: Using medium dataset strategy with in-memory seen keys");

        let mut seen_keys = HashSet::with_capacity(accessor.len() / 2);
        let mut result = Vec::new();

        // Process in 1K row batches
        for batch_result in accessor.iter_batches(1_000) {
            let batch = batch_result?;

            for row in batch {
                let key = self.build_dedup_key(&row, &config.key_fields)?;

                if seen_keys.insert(key) {
                    // First time seeing this key
                    result.push(row);
                }
            }

            // Check memory periodically
            if result.len() % 10_000 == 0 {
                let estimated_mem = result.capacity() * std::mem::size_of::<serde_json::Value>();
                if estimated_mem > 500_000_000 {
                    // 500MB threshold
                    tracing::warn!(
                        "DEDUP_OPTIMIZED: Memory usage high, consider using large dataset strategy"
                    );
                }
            }
        }

        Ok(result)
    }

    /// In-memory deduplication for small datasets (<10K rows)
    fn in_memory_deduplicate(
        &self,
        accessor: &RowAccessor,
        config: &DeduplicatorConfig,
    ) -> Result<Vec<serde_json::Value>> {
        tracing::info!(
            "DEDUP_OPTIMIZED: Using small dataset strategy with full in-memory processing"
        );

        // For small datasets, materialize all rows
        let rows = accessor.to_vec()?;
        let mut seen_keys = HashSet::new();
        let mut result = Vec::new();

        for row in rows {
            let key = self.build_dedup_key(&row, &config.key_fields)?;

            if seen_keys.insert(key) {
                result.push(row);
            }
        }

        Ok(result)
    }

    /// Build deduplication key from row
    fn build_dedup_key(&self, row: &serde_json::Value, key_fields: &[String]) -> Result<String> {
        let mut key_parts = Vec::new();

        for field in key_fields {
            let value = row
                .get(field)
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".to_string(),
                    _ => serde_json::to_string(v).unwrap_or_default(),
                })
                .unwrap_or_else(|| "null".to_string());

            key_parts.push(value);
        }

        Ok(key_parts.join("|"))
    }

    /// Optimized CSV export that streams data
    pub async fn execute_csv_export(
        &self,
        config: &CsvExporterConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("CSV_EXPORT_OPTIMIZED: Starting export");

        let row_accessor = context.get_rows()?;
        let row_count = row_accessor.len();

        if row_count == 0 {
            return Ok((
                true,
                serde_json::json!({
                    "_output_path": config.output_path,
                    "_rows_written": 0,
                    "_status": "no_data",
                }),
            ));
        }

        let (rows_written, storage_type, batch_size) = if row_count > 100_000 {
            let file = std::fs::File::create(&config.output_path)
                .map_err(|e| WorkflowError::IoError(e.to_string()))?;
            let mut writer = csv::WriterBuilder::new()
                .delimiter(config.delimiter.unwrap_or(',') as u8)
                .from_writer(file);

            let first_row = row_accessor
                .get(0)?
                .ok_or_else(|| WorkflowError::DataNotFound("No first row for headers".into()))?;

            let headers: Vec<String> = if let serde_json::Value::Object(obj) = &first_row {
                obj.keys().cloned().collect()
            } else {
                return Err(WorkflowError::InvalidData("Row is not an object".into()));
            };

            if config.include_header {
                writer
                    .write_record(&headers)
                    .map_err(|e| WorkflowError::IoError(e.to_string()))?;
            }

            let mut rows_written = 0;
            let batch_size = 10_000;

            for batch_result in row_accessor.iter_batches(batch_size) {
                let batch = batch_result?;

                for row in batch {
                    if let serde_json::Value::Object(obj) = row {
                        let record: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                obj.get(h)
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Null => String::new(),
                                        _ => v.to_string(),
                                    })
                                    .unwrap_or_default()
                            })
                            .collect();

                        writer
                            .write_record(&record)
                            .map_err(|e| WorkflowError::IoError(e.to_string()))?;

                        rows_written += 1;
                    }
                }

                if rows_written % 100_000 == 0 {
                    tracing::debug!("CSV_EXPORT_OPTIMIZED: Written {} rows", rows_written);
                }
            }

            writer
                .flush()
                .map_err(|e| WorkflowError::IoError(e.to_string()))?;

            (rows_written, "streaming", batch_size)
        } else {
            let frame = context.get_batch_frame()?;
            let operator = CsvExportBatchOperator;
            let rows_written = operator.execute(&frame, config)?;
            (rows_written, "batch_frame", row_count.max(1))
        };

        tracing::info!(
            "CSV_EXPORT_OPTIMIZED: Successfully exported {} rows",
            rows_written
        );

        Ok((
            true,
            serde_json::json!({
                "_output_path": config.output_path,
                "_rows_written": rows_written,
                "_storage_type": storage_type,
                "_batch_size": batch_size,
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    row_count,
                    rows_written,
                    if storage_type == "streaming" { 0 } else { 1 },
                )?,
            }),
        ))
    }

    /// Optimized semantic mapper that processes in chunks
    pub async fn execute_semantic_mapper(
        &self,
        config: &SemanticMapperConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("MAPPER_OPTIMIZED: Starting semantic mapping");

        let row_accessor = context.get_rows()?;
        let row_count = row_accessor.len();

        let mapped_count = if row_count > 50_000 {
            let mapped_rows = self.streaming_map_large(&row_accessor, config)?;
            let mapped_count = mapped_rows.len();
            context.set_rows(mapped_rows)?;
            mapped_count
        } else {
            let frame = context.get_batch_frame()?;
            let operator = SemanticMapperBatchOperator;
            let mapped_frame = operator.execute(frame, config)?;
            let mapped_count = mapped_frame.row_count();
            context.set_batch_frame(mapped_frame)?;
            mapped_count
        };

        Ok((
            true,
            serde_json::json!({
                "_row_count": mapped_count,
                "_original_count": row_count,
                "_target_ontology": config.target_ontology.clone(),
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    row_count,
                    mapped_count,
                    if row_count > 50_000 { 0 } else { 1 },
                )?,
            }),
        ))
    }

    /// Optimized data validator that keeps the small-dataset path batch-native.
    pub async fn execute_data_validator(
        &self,
        config: &DataValidatorConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("VALIDATOR_OPTIMIZED: Starting data validation");

        let row_accessor = context.get_rows()?;
        let row_count = row_accessor.len();

        let (success, errors, warnings, execution_path) = if row_count > 50_000 {
            let rows = row_accessor.to_vec()?;
            let (success, errors, warnings) = self.validate_rows_large(&rows, config)?;
            (success, errors, warnings, "row_json")
        } else {
            let frame = context.get_batch_frame()?;
            let operator = DataValidatorBatchOperator;
            let result = operator.execute(frame, config)?;
            let success = result.success;
            let errors = result.errors;
            let warnings = result.warnings;
            context.set_batch_frame(result.frame)?;
            (success, errors, warnings, "batch_frame")
        };

        Ok((
            success,
            serde_json::json!({
                "_row_count": row_count,
                "_errors": errors,
                "_warnings": warnings,
                "_error_count": errors.len(),
                "_warning_count": warnings.len(),
                "_execution_path": execution_path,
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    row_count,
                    row_count,
                    if execution_path == "row_json" { 1 } else { 0 },
                )?,
            }),
        ))
    }

    /// Optimized field transformer that keeps the small-dataset row path batch-native.
    pub async fn execute_field_transformer(
        &self,
        config: &super::definition::FieldTransformerConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("FIELD_TRANSFORMER_OPTIMIZED: Starting field transformation");

        let row_accessor = context.get_rows()?;
        let row_count = row_accessor.len();

        let (stats, execution_path) = if row_count > 50_000 {
            let rows = row_accessor.to_vec()?;
            let object_rows = rows
                .into_iter()
                .map(|row| {
                    row.as_object().cloned().ok_or_else(|| {
                        WorkflowError::InvalidData("Field transformer requires object rows".into())
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let (transformed_rows, stats) =
                super::field_transformer::transform_object_rows(&object_rows, config)?;
            context.set_rows(
                transformed_rows
                    .into_iter()
                    .map(serde_json::Value::Object)
                    .collect(),
            )?;
            (stats, "row_json")
        } else {
            let frame = context.get_batch_frame()?;
            let operator = FieldTransformerBatchOperator;
            let result = operator.execute(frame, config)?;
            let stats = result.stats;
            context.set_batch_frame(result.frame)?;
            (stats, "batch_frame")
        };

        Ok((
            true,
            serde_json::json!({
                "_row_count": row_count,
                "_rows_transformed": stats.rows_transformed,
                "_fields_modified": stats.fields_modified,
                "_execution_path": execution_path,
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    row_count,
                    row_count,
                    if execution_path == "row_json" { 1 } else { 0 },
                )?,
            }),
        ))
    }

    /// Optimized aggregator that keeps the small-dataset path batch-native.
    pub async fn execute_aggregator(
        &self,
        config: &AggregatorConfig,
        context: &mut ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value)> {
        tracing::info!("AGGREGATOR_OPTIMIZED: Starting aggregation");

        let row_accessor = context.get_rows()?;
        let original_count = row_accessor.len();

        let (aggregated_count, execution_path) = if original_count > 50_000 {
            let rows = row_accessor.to_vec()?;
            let aggregated_rows = self.aggregate_rows_large(&rows, config)?;
            let aggregated_count = aggregated_rows.len();
            context.set_rows(aggregated_rows)?;
            (aggregated_count, "row_json")
        } else {
            let frame = context.get_batch_frame()?;
            let operator = AggregatorBatchOperator;
            let aggregated_frame = operator.execute(frame, config)?;
            let aggregated_count = aggregated_frame.row_count();
            context.set_batch_frame(aggregated_frame)?;
            (aggregated_count, "batch_frame")
        };

        Ok((
            true,
            serde_json::json!({
                "_row_count": aggregated_count,
                "_original_count": original_count,
                "_execution_path": execution_path,
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_runtime_metrics": self.build_runtime_metrics_value(
                    context,
                    original_count,
                    aggregated_count,
                    if execution_path == "row_json" { 1 } else { 0 },
                )?,
            }),
        ))
    }

    /// Streaming semantic mapping for large datasets
    fn streaming_map_large(
        &self,
        accessor: &RowAccessor,
        _config: &SemanticMapperConfig,
    ) -> Result<Vec<serde_json::Value>> {
        tracing::info!("MAPPER_OPTIMIZED: Using streaming strategy for large dataset");

        let mut result = Vec::new();

        for batch_result in accessor.iter_batches(5_000) {
            let batch = batch_result?;

            for row in batch {
                // TODO: Call semantic mapping service using config.target_ontology
                // For now, just pass through the row
                result.push(row);
            }

            // Periodic memory check
            if result.len() % 50_000 == 0 {
                tracing::debug!("MAPPER_OPTIMIZED: Processed {} rows", result.len());
            }
        }

        Ok(result)
    }

    /// In-memory semantic mapping for smaller datasets
    fn in_memory_map(
        &self,
        accessor: &RowAccessor,
        _config: &SemanticMapperConfig,
    ) -> Result<Vec<serde_json::Value>> {
        tracing::info!("MAPPER_OPTIMIZED: Using in-memory strategy");

        let rows = accessor.to_vec()?;

        // TODO: Call semantic mapping service using config.target_ontology
        // For now, just pass through the rows
        Ok(rows)
    }

    fn validate_rows_large(
        &self,
        rows: &[serde_json::Value],
        config: &DataValidatorConfig,
    ) -> Result<(bool, Vec<serde_json::Value>, Vec<serde_json::Value>)> {
        use super::definition::{RuleType, Severity};

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for (row_idx, row) in rows.iter().enumerate() {
            for rule in &config.rules {
                let field_value = row.get(&rule.field);
                let is_valid = match &rule.rule_type {
                    RuleType::NotNull => matches!(field_value, Some(value) if !value.is_null()),
                    RuleType::Regex { pattern } => {
                        if let Some(serde_json::Value::String(string)) = field_value {
                            regex::Regex::new(pattern)
                                .map(|compiled| compiled.is_match(string))
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                    RuleType::Range { min, max } => {
                        if let Some(number) = field_value.and_then(|value| value.as_f64()) {
                            number >= *min && number <= *max
                        } else {
                            false
                        }
                    }
                    RuleType::InSet { values } => {
                        if let Some(serde_json::Value::String(string)) = field_value {
                            values.contains(string)
                        } else {
                            false
                        }
                    }
                    RuleType::Length { min, max } => {
                        if let Some(serde_json::Value::String(string)) = field_value {
                            string.len() >= *min && string.len() <= *max
                        } else {
                            false
                        }
                    }
                    _ => true,
                };

                if !is_valid {
                    let violation = serde_json::json!({
                        "row": row_idx,
                        "field": rule.field,
                        "rule_type": format!("{:?}", rule.rule_type),
                        "value": field_value,
                    });

                    match rule.severity {
                        Severity::Error => errors.push(violation),
                        Severity::Warning => warnings.push(violation),
                    }
                }
            }
        }

        let success = !config.fail_on_error || errors.is_empty();
        Ok((success, errors, warnings))
    }

    fn aggregate_rows_large(
        &self,
        rows: &[serde_json::Value],
        config: &AggregatorConfig,
    ) -> Result<Vec<serde_json::Value>> {
        use super::definition::AggFunction;
        use std::collections::HashMap;

        let mut groups: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
        for row in rows {
            let key = config
                .group_by
                .iter()
                .map(|field| {
                    row.get(field)
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|");
            groups.entry(key).or_default().push(row);
        }

        let mut result_rows = Vec::with_capacity(groups.len());
        for (key_str, group_rows) in groups {
            let mut result_row = serde_json::Map::new();

            let keys: Vec<&str> = key_str.split('|').collect();
            for (index, field) in config.group_by.iter().enumerate() {
                if index < keys.len() {
                    result_row.insert(field.clone(), serde_json::json!(keys[index]));
                }
            }

            for aggregation in &config.aggregations {
                let values: Vec<f64> = group_rows
                    .iter()
                    .filter_map(|row| row.get(&aggregation.field).and_then(|value| value.as_f64()))
                    .collect();

                let aggregate_value = match aggregation.function {
                    AggFunction::Sum => values.iter().sum(),
                    AggFunction::Avg => {
                        if values.is_empty() {
                            0.0
                        } else {
                            values.iter().sum::<f64>() / values.len() as f64
                        }
                    }
                    AggFunction::Count => values.len() as f64,
                    AggFunction::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
                    AggFunction::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    _ => 0.0,
                };

                let field_name = aggregation.alias.clone().unwrap_or_else(|| {
                    format!("{}_{:?}", aggregation.field, aggregation.function).to_lowercase()
                });
                result_row.insert(field_name, serde_json::json!(aggregate_value));
            }

            result_rows.push(serde_json::Value::Object(result_row));
        }

        Ok(result_rows)
    }
}

/// Extension trait for StorageManager to support deduplication
#[cfg(feature = "workflow-storage")]
impl StorageManager {
    /// Create temporary database for deduplication keys
    pub fn create_temp_db(&self, name: &str) -> Result<rocksdb::DB> {
        let path = self
            .temp_dir
            .join(format!("temp_{}_{}", name, uuid::Uuid::new_v4()));
        rocksdb::DB::open_default(&path).map_err(|e| WorkflowError::Storage(e.to_string()))
    }

    /// Cleanup temporary database
    pub fn cleanup_temp_db(&self, name: &str) -> Result<()> {
        // Find and remove temp DB directories
        if let Ok(entries) = std::fs::read_dir(&self.temp_dir) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.starts_with(&format!("temp_{}_", name)) {
                        std::fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "workflow-storage"))]
mod tests {
    use super::*;
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_optimized_deduplicator() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);

        // Create test data with duplicates
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
            json!({"id": 1, "name": "Alice"}), // Duplicate
            json!({"id": 3, "name": "Charlie"}),
            json!({"id": 2, "name": "Bob"}), // Duplicate
        ];

        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_dedup".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let config = DeduplicatorConfig {
            key_fields: vec!["id".to_string()],
            method: super::super::definition::DedupMethod::Exact,
            threshold: None,
            keep: super::super::definition::KeepStrategy::First,
        };

        let (success, output) = executor
            .execute_deduplicator(&config, &mut context)
            .await
            .unwrap();

        assert!(success);
        assert_eq!(output["_row_count"], 3);
        assert_eq!(output["_duplicates_removed"], 2);
        assert_eq!(output["_execution_path"], "batch_frame");
        assert_eq!(output["_runtime_metrics"]["input_rows"], 5);
        assert_eq!(output["_runtime_metrics"]["output_rows"], 3);
        assert_eq!(output["_runtime_metrics"]["storage_type"], "in_memory");

        // Verify deduplicated data
        let result_rows = context.get_rows().unwrap().to_vec().unwrap();
        assert_eq!(result_rows.len(), 3);
        assert_eq!(result_rows[0]["id"], 1);
        assert_eq!(result_rows[1]["id"], 2);
        assert_eq!(result_rows[2]["id"], 3);

        let result_frame = context.get_batch_frame().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_dedup")
        );
    }

    #[tokio::test]
    async fn test_batch_frame_csv_export_for_small_datasets() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);

        // Create test data
        let rows = vec![
            json!({"id": 1, "name": "Alice", "age": 30}),
            json!({"id": 2, "name": "Bob", "age": 25}),
            json!({"id": 3, "name": "Charlie", "age": 35}),
        ];

        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_csv".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let output_path = temp_dir.path().join("output.csv");
        let config = CsvExporterConfig {
            output_path: output_path.to_str().unwrap().to_string(),
            delimiter: Some(','),
            include_header: true,
            encoding: None,
        };

        let (success, output) = executor
            .execute_csv_export(&config, &mut context)
            .await
            .unwrap();

        assert!(success);
        assert_eq!(output["_rows_written"], 3);
        assert_eq!(output["_storage_type"], "batch_frame");

        // Verify CSV file
        let csv_content = std::fs::read_to_string(&output_path).unwrap();
        assert!(csv_content.contains("age,id,name"));
        assert!(csv_content.contains("30,1,Alice"));
        assert!(csv_content.contains("25,2,Bob"));
        assert!(csv_content.contains("35,3,Charlie"));

        let result_frame = context.get_batch_frame().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_csv")
        );
    }

    #[tokio::test]
    async fn test_semantic_mapper_preserves_batch_frame_metadata() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];
        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_small".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let config = SemanticMapperConfig {
            target_ontology: vec!["gph:Customer".to_string()],
            auto_approve_threshold: 0.95,
            mapping_mode: super::super::definition::MappingMode::Hybrid,
            mapping_session_id: None,
            source_id: None,
            table_name: None,
            entity_uri: None,
        };

        let (success, output) = executor
            .execute_semantic_mapper(&config, &mut context)
            .await
            .unwrap();

        assert!(success);
        assert_eq!(output["_row_count"], 2);

        let result_frame = context.get_batch_frame().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_small")
        );
        assert_eq!(
            result_frame.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
    }

    #[tokio::test]
    async fn test_batch_frame_data_validator_for_small_datasets() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);
        let rows = vec![
            json!({"name": "Alice", "age": 30, "status": "active"}),
            json!({"name": null, "age": 150, "status": "inactive"}),
            json!({"name": "Bob", "age": 21, "status": "paused"}),
        ];
        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_validate".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let config = DataValidatorConfig {
            rules: vec![
                super::super::definition::ValidationRule {
                    field: "name".to_string(),
                    rule_type: super::super::definition::RuleType::NotNull,
                    params: None,
                    severity: super::super::definition::Severity::Error,
                },
                super::super::definition::ValidationRule {
                    field: "age".to_string(),
                    rule_type: super::super::definition::RuleType::Range {
                        min: 0.0,
                        max: 120.0,
                    },
                    params: None,
                    severity: super::super::definition::Severity::Error,
                },
                super::super::definition::ValidationRule {
                    field: "status".to_string(),
                    rule_type: super::super::definition::RuleType::InSet {
                        values: vec!["active".to_string(), "inactive".to_string()],
                    },
                    params: None,
                    severity: super::super::definition::Severity::Warning,
                },
            ],
            fail_on_error: true,
        };

        let (success, output) = executor
            .execute_data_validator(&config, &mut context)
            .await
            .unwrap();

        assert!(!success);
        assert_eq!(output["_row_count"], 3);
        assert_eq!(output["_error_count"], 2);
        assert_eq!(output["_warning_count"], 1);
        assert_eq!(output["_execution_path"], "batch_frame");

        let result_frame = context.get_batch_frame().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_validate")
        );
    }

    #[tokio::test]
    async fn test_batch_frame_aggregator_for_small_datasets() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);
        let rows = vec![
            json!({"region": "east", "amount": 10.0, "orders": 1}),
            json!({"region": "east", "amount": 15.0, "orders": 2}),
            json!({"region": "west", "amount": 7.0, "orders": 3}),
        ];
        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_aggregate".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let config = AggregatorConfig {
            group_by: vec!["region".to_string()],
            aggregations: vec![
                super::super::definition::Aggregation {
                    field: "amount".to_string(),
                    function: super::super::definition::AggFunction::Sum,
                    alias: Some("total_amount".to_string()),
                },
                super::super::definition::Aggregation {
                    field: "orders".to_string(),
                    function: super::super::definition::AggFunction::Count,
                    alias: Some("order_count".to_string()),
                },
            ],
        };

        let (success, output) = executor
            .execute_aggregator(&config, &mut context)
            .await
            .unwrap();

        assert!(success);
        assert_eq!(output["_row_count"], 2);
        assert_eq!(output["_original_count"], 3);
        assert_eq!(output["_execution_path"], "batch_frame");
        assert_eq!(output["_runtime_metrics"]["input_rows"], 3);
        assert_eq!(output["_runtime_metrics"]["output_rows"], 2);
        assert_eq!(output["_runtime_metrics"]["storage_type"], "in_memory");
        assert_eq!(output["_runtime_metrics"]["planned_tier"], "in_memory");

        let result_frame = context.get_batch_frame().unwrap();
        let result_rows = result_frame.to_json_values().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_aggregate")
        );
        assert!(result_rows.iter().any(|row| {
            row["region"] == "east" && row["total_amount"] == 25.0 && row["order_count"] == 2.0
        }));
        assert!(result_rows.iter().any(|row| {
            row["region"] == "west" && row["total_amount"] == 7.0 && row["order_count"] == 1.0
        }));
    }

    #[tokio::test]
    async fn test_batch_frame_field_transformer_for_small_datasets() {
        let temp_dir = tempdir().unwrap();
        let storage_manager = Arc::new(
            StorageManager::new(
                &temp_dir.path().join("rocks"),
                &temp_dir.path().join("temp"),
            )
            .unwrap(),
        );

        let executor = OptimizedStepExecutor::new(storage_manager.clone());
        let mut context = ExecutionContextV2::with_storage_manager(json!({}), storage_manager);
        let rows = vec![
            json!({"email": "  TEST@EXAMPLE.COM  ", "status": "ACTIVE"}),
            json!({"email": "second@example.com", "status": "PENDING"}),
        ];
        let frame =
            BatchFrame::from_json_values(&rows)
                .unwrap()
                .with_metadata(BatchFrameMetadata {
                    source_step_id: Some("extract_transform".to_string()),
                    source_kind: Some("db_extract".to_string()),
                    source_id: None,
                });
        context.set_batch_frame(frame).unwrap();

        let config = super::super::definition::FieldTransformerConfig {
            transformations: vec![
                super::super::definition::FieldTransformation {
                    field: "email".to_string(),
                    operations: vec![
                        super::super::definition::TransformOperation::Trim,
                        super::super::definition::TransformOperation::Lower,
                    ],
                },
                super::super::definition::FieldTransformation {
                    field: "status".to_string(),
                    operations: vec![super::super::definition::TransformOperation::Lower],
                },
            ],
        };

        let (success, output) = executor
            .execute_field_transformer(&config, &mut context)
            .await
            .unwrap();

        assert!(success);
        assert_eq!(output["_row_count"], 2);
        assert_eq!(output["_rows_transformed"], 2);
        assert_eq!(output["_fields_modified"], 3);
        assert_eq!(output["_execution_path"], "batch_frame");

        let result_frame = context.get_batch_frame().unwrap();
        let result_rows = result_frame.to_json_values().unwrap();
        assert_eq!(
            result_frame.metadata().source_step_id.as_deref(),
            Some("extract_transform")
        );
        assert_eq!(result_rows[0]["email"], json!("test@example.com"));
        assert_eq!(result_rows[0]["status"], json!("active"));
        assert_eq!(result_rows[1]["status"], json!("pending"));
    }
}
