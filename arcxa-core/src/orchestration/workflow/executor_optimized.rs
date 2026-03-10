//! Optimized executor implementation using row storage
//!
//! This module shows how to migrate executor steps to use the new
//! row storage system, eliminating expensive clone operations.

use super::definition::{CsvExporterConfig, DeduplicatorConfig, SemanticMapperConfig};
use super::error::{Result, WorkflowError};
use super::execution_context_v2::ExecutionContextV2;
use super::row_storage::RowAccessor;
#[cfg(feature = "workflow-storage")]
use super::row_storage::StorageManager;
#[cfg(feature = "workflow-storage")]
use super::streaming_deduplicator::{StreamingDedupConfig, StreamingDeduplicator};
use serde_json;
use std::collections::HashSet;
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

        // Choose strategy based on dataset size
        let deduped_rows = if original_count > 100_000 {
            // Large dataset: use streaming with disk-backed seen keys
            self.streaming_deduplicate_large(&row_accessor, config)
                .await?
        } else if original_count > 10_000 {
            // Medium dataset: streaming with in-memory seen keys
            self.streaming_deduplicate_medium(&row_accessor, config)?
        } else {
            // Small dataset: in-memory processing
            self.in_memory_deduplicate(&row_accessor, config)?
        };

        let deduped_count = deduped_rows.len();
        let duplicates_removed = original_count - deduped_count;

        // Store deduplicated rows with automatic tiering
        context.set_rows(deduped_rows)?;

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
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                "_method": config.method.to_string(),
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
        tracing::info!("CSV_EXPORT_OPTIMIZED: Starting streaming export");

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

        // Open output file
        let file = std::fs::File::create(&config.output_path)
            .map_err(|e| WorkflowError::IoError(e.to_string()))?;
        let mut writer = csv::Writer::from_writer(file);

        // Get headers from first row
        let first_row = row_accessor
            .get(0)?
            .ok_or_else(|| WorkflowError::DataNotFound("No first row for headers".into()))?;

        let headers: Vec<String> = if let serde_json::Value::Object(obj) = &first_row {
            obj.keys().cloned().collect()
        } else {
            return Err(WorkflowError::InvalidData("Row is not an object".into()));
        };

        // Write headers
        writer
            .write_record(&headers)
            .map_err(|e| WorkflowError::IoError(e.to_string()))?;

        // Stream rows to CSV
        let mut rows_written = 0;
        let batch_size = if row_count > 100_000 { 10_000 } else { 1_000 };

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

        tracing::info!(
            "CSV_EXPORT_OPTIMIZED: Successfully exported {} rows",
            rows_written
        );

        Ok((
            true,
            serde_json::json!({
                "_output_path": config.output_path,
                "_rows_written": rows_written,
                "_storage_type": "streaming",
                "_batch_size": batch_size,
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

        // Process based on size
        let mapped_rows = if row_count > 50_000 {
            self.streaming_map_large(&row_accessor, config)?
        } else {
            self.in_memory_map(&row_accessor, config)?
        };

        let mapped_count = mapped_rows.len();
        context.set_rows(mapped_rows)?;

        Ok((
            true,
            serde_json::json!({
                "_row_count": mapped_count,
                "_original_count": row_count,
                "_target_ontology": config.target_ontology.clone(),
                "_storage_type": context.row_storage.as_ref()
                    .map(|s| s.storage_type().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
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

        context.set_rows(rows).unwrap();

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

        // Verify deduplicated data
        let result_rows = context.get_rows().unwrap().to_vec().unwrap();
        assert_eq!(result_rows.len(), 3);
        assert_eq!(result_rows[0]["id"], 1);
        assert_eq!(result_rows[1]["id"], 2);
        assert_eq!(result_rows[2]["id"], 3);
    }

    #[tokio::test]
    async fn test_streaming_csv_export() {
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

        context.set_rows(rows).unwrap();

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

        // Verify CSV file
        let csv_content = std::fs::read_to_string(&output_path).unwrap();
        assert!(csv_content.contains("id,name,age") || csv_content.contains("age,id,name"));
        assert!(csv_content.contains("Alice"));
        assert!(csv_content.contains("Bob"));
        assert!(csv_content.contains("Charlie"));
    }
}
