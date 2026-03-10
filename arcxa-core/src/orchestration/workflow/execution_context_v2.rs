//! Optimized execution context with row storage backend
//!
//! This module provides an enhanced ExecutionContext that eliminates expensive
//! clone operations by using tiered row storage.

use super::error::{Result, WorkflowError};
use super::row_lineage_context::RowLineageContext;
#[cfg(feature = "workflow-storage")]
use super::row_storage::StorageManager;
use super::row_storage::{
    estimate_memory_size, RowAccessor, RowReference, RowStorage, StorageType,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Resource limits for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes (default: 10GB)
    pub max_memory_bytes: usize,

    /// Maximum row count (default: 200K)
    pub max_row_count: usize,

    /// Enable disk spilling when memory limit is reached
    pub enable_disk_spill: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            max_row_count: 200_000,
            enable_disk_spill: true,
        }
    }
}

/// Enhanced execution context with row storage
#[derive(Clone)]
pub struct ExecutionContextV2 {
    /// Original input data (immutable metadata only)
    pub input_data: serde_json::Value,

    /// Working data (metadata only, no _rows)
    pub working_data: serde_json::Value,

    /// Row storage reference (replaces _rows in working_data)
    pub row_storage: Option<RowStorage>,

    /// Step outputs (metadata only)
    pub step_outputs: HashMap<String, serde_json::Value>,

    /// Row storage per step (for intermediate results)
    pub step_row_storage: HashMap<String, RowStorage>,

    /// User-provided metadata
    pub metadata: HashMap<String, String>,

    /// Row-level lineage context
    pub row_lineage: Option<RowLineageContext>,

    /// Optional workflow identifier
    pub workflow_id: Option<String>,

    /// Current execution ID
    pub execution_id: String,

    /// Current step ID (for storage management)
    pub current_step_id: Option<String>,

    /// Resource limits
    pub resource_limits: ResourceLimits,

    /// Storage manager for lifecycle management
    #[cfg(feature = "workflow-storage")]
    pub storage_manager: Option<Arc<StorageManager>>,

    /// Performance metrics
    pub metrics: ExecutionMetrics,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub total_rows_processed: usize,
    pub storage_operations: HashMap<String, usize>,
    pub memory_high_water_mark: usize,
    pub storage_type_usage: HashMap<StorageType, usize>,
}

impl ExecutionContextV2 {
    /// Create a new execution context
    pub fn new(input_data: serde_json::Value) -> Self {
        Self {
            input_data: input_data.clone(),
            working_data: input_data,
            row_storage: None,
            step_outputs: HashMap::new(),
            step_row_storage: HashMap::new(),
            metadata: HashMap::new(),
            row_lineage: None,
            workflow_id: None,
            execution_id: uuid::Uuid::new_v4().to_string(),
            current_step_id: None,
            resource_limits: ResourceLimits::default(),
            #[cfg(feature = "workflow-storage")]
            storage_manager: None,
            metrics: ExecutionMetrics::default(),
        }
    }

    /// Create context with storage manager for optimal performance
    #[cfg(feature = "workflow-storage")]
    pub fn with_storage_manager(
        input_data: serde_json::Value,
        storage_manager: Arc<StorageManager>,
    ) -> Self {
        let mut ctx = Self::new(input_data);
        ctx.storage_manager = Some(storage_manager);
        ctx
    }

    /// Set the current step ID (for storage prefixing)
    pub fn set_current_step(&mut self, step_id: String) {
        self.current_step_id = Some(step_id);
    }

    /// Get row iterator for streaming access (zero-copy)
    pub fn get_row_iterator(&self) -> Result<impl Iterator<Item = Result<serde_json::Value>>> {
        match &self.row_storage {
            Some(storage) => Ok(RowAccessor::new(storage.clone()).iter()),
            None => {
                // Fallback to working_data for backwards compatibility
                if let Some(rows) = self.working_data.get("_rows").and_then(|v| v.as_array()) {
                    let rows = rows.clone();
                    Ok(RowAccessor::from_json(serde_json::Value::Array(rows)).iter())
                } else {
                    Err(WorkflowError::DataNotFound("No row data available".into()))
                }
            }
        }
    }

    /// Get rows with automatic backend selection
    pub fn get_rows(&self) -> Result<RowAccessor> {
        match &self.row_storage {
            Some(storage) => {
                self.record_storage_operation("get_rows", storage.storage_type());
                Ok(RowAccessor::new(storage.clone()))
            }
            None => {
                // Fallback to working_data for backwards compatibility
                if let Some(rows) = self.working_data.get("_rows") {
                    Ok(RowAccessor::from_json(rows.clone()))
                } else if let Some(rows) = self.find_rows_in_step_outputs() {
                    Ok(RowAccessor::from_json(rows))
                } else {
                    Err(WorkflowError::DataNotFound("No row data available".into()))
                }
            }
        }
    }

    /// Set rows with automatic tiering based on size
    pub fn set_rows(&mut self, rows: Vec<serde_json::Value>) -> Result<()> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);

        // Check resource limits
        if row_count > self.resource_limits.max_row_count {
            return Err(WorkflowError::ResourceLimit(format!(
                "Row count {} exceeds limit {}",
                row_count, self.resource_limits.max_row_count
            )));
        }

        // Update metrics
        self.metrics.total_rows_processed += row_count;
        self.metrics.memory_high_water_mark =
            self.metrics.memory_high_water_mark.max(estimated_size);

        // Choose storage backend
        #[cfg(feature = "workflow-storage")]
        let storage = if let Some(ref manager) = self.storage_manager {
            // Use storage manager for optimal tiering
            let step_id = self.current_step_id.as_deref().unwrap_or("unknown");
            let storage = manager.store_rows(&self.execution_id, step_id, rows)?;
            self.record_storage_operation("set_rows", storage.storage_type());
            storage
        } else {
            // Fallback to automatic tiering without manager
            let storage = RowStorage::from_rows(rows)?;
            self.record_storage_operation("set_rows", storage.storage_type());
            storage
        };

        #[cfg(not(feature = "workflow-storage"))]
        let storage = {
            // Automatic tiering without manager
            let storage = RowStorage::from_rows(rows)?;
            self.record_storage_operation("set_rows", storage.storage_type());
            storage
        };

        self.row_storage = Some(storage);

        // Update working_data metadata
        if let serde_json::Value::Object(ref mut obj) = self.working_data {
            obj.insert("_row_count".to_string(), serde_json::json!(row_count));
            obj.insert(
                "_storage_type".to_string(),
                serde_json::json!(self
                    .row_storage
                    .as_ref()
                    .unwrap()
                    .storage_type()
                    .to_string()),
            );
            // Remove old _rows field to save memory
            obj.remove("_rows");
        }

        Ok(())
    }

    /// Store intermediate results for a step
    pub fn store_step_rows(&mut self, step_id: String, rows: Vec<serde_json::Value>) -> Result<()> {
        #[cfg(feature = "workflow-storage")]
        let storage = if let Some(ref manager) = self.storage_manager {
            manager.store_rows(&self.execution_id, &step_id, rows)?
        } else {
            RowStorage::from_rows(rows)?
        };

        #[cfg(not(feature = "workflow-storage"))]
        let storage = RowStorage::from_rows(rows)?;

        self.record_storage_operation("store_step", storage.storage_type());
        self.step_row_storage.insert(step_id, storage);
        Ok(())
    }

    /// Get rows from a specific step
    pub fn get_step_rows(&self, step_id: &str) -> Option<RowAccessor> {
        self.step_row_storage.get(step_id).map(|storage| {
            self.record_storage_operation("get_step", storage.storage_type());
            RowAccessor::new(storage.clone())
        })
    }

    /// Merge step output into working data (optimized version)
    pub fn merge_step_output(&mut self, step_id: String, output: serde_json::Value) -> Result<()> {
        // Extract rows if present
        let (metadata, rows) = if let serde_json::Value::Object(mut obj) = output {
            let rows = obj.remove("_rows").and_then(|v| v.as_array().cloned());
            (serde_json::Value::Object(obj), rows)
        } else {
            (output, None)
        };

        // Store rows separately if present
        if let Some(rows) = rows {
            self.set_rows(rows)?;
        }

        // Merge metadata into working_data
        if let serde_json::Value::Object(ref mut working_obj) = self.working_data {
            if let serde_json::Value::Object(metadata_obj) = metadata {
                for (key, value) in metadata_obj {
                    if key != "_rows" {
                        // Skip _rows as it's now in row_storage
                        working_obj.insert(key, value);
                    }
                }
            }
        }

        Ok(())
    }

    /// Check memory usage and potentially spill to disk
    pub fn check_memory_pressure(&mut self) -> Result<()> {
        if let Some(ref storage) = self.row_storage {
            let mem_usage = storage.memory_usage();

            if mem_usage > self.resource_limits.max_memory_bytes {
                if self.resource_limits.enable_disk_spill {
                    self.spill_to_disk()?;
                } else {
                    return Err(WorkflowError::ResourceLimit(format!(
                        "Memory usage {} exceeds limit {}",
                        mem_usage, self.resource_limits.max_memory_bytes
                    )));
                }
            }
        }
        Ok(())
    }

    /// Spill current row storage to disk
    fn spill_to_disk(&mut self) -> Result<()> {
        #[cfg(feature = "workflow-storage")]
        {
            if let Some(storage) = &self.row_storage {
                if matches!(
                    storage.storage_type(),
                    StorageType::InMemory | StorageType::Shared
                ) {
                    tracing::info!(
                        "Spilling {} rows to disk due to memory pressure",
                        storage.len()
                    );

                    if let Some(ref manager) = self.storage_manager {
                        // Get current rows
                        let accessor = RowAccessor::new(storage.clone());
                        let rows = accessor.to_vec()?;

                        // Store in RocksDB
                        let step_id = self.current_step_id.as_deref().unwrap_or("spilled");
                        let disk_storage =
                            manager.create_rocks_storage(&self.execution_id, step_id, rows)?;

                        self.row_storage = Some(disk_storage);
                        self.record_storage_operation("spill_to_disk", StorageType::RocksDB);
                    }
                }
            }
        }
        #[cfg(not(feature = "workflow-storage"))]
        {
            tracing::warn!("Disk spilling not available without workflow-storage feature");
        }
        Ok(())
    }

    /// Find rows in step outputs (for backwards compatibility)
    fn find_rows_in_step_outputs(&self) -> Option<serde_json::Value> {
        for (_step_id, output) in &self.step_outputs {
            if let Some(rows) = output.get("_rows") {
                return Some(rows.clone());
            }
        }
        None
    }

    /// Record storage operation for metrics
    fn record_storage_operation(&self, operation: &str, storage_type: StorageType) {
        // Note: Using interior mutability pattern would be better here
        // For now, this is a no-op in the const context
        // In production, use Arc<Mutex<ExecutionMetrics>> or similar
    }

    /// Get execution metrics
    pub fn get_metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    /// Cleanup resources
    pub fn cleanup(&self) -> Result<()> {
        #[cfg(feature = "workflow-storage")]
        if let Some(ref manager) = self.storage_manager {
            manager.cleanup_execution(&self.execution_id)?;
        }
        Ok(())
    }
}

/// Migration helper to convert old context to new
impl From<super::executor::ExecutionContext> for ExecutionContextV2 {
    fn from(old: super::executor::ExecutionContext) -> Self {
        let mut new = Self::new(old.input_data);
        new.working_data = old.working_data;
        new.step_outputs = old.step_outputs;
        new.metadata = old.metadata;
        new.row_lineage = old.row_lineage;
        new.workflow_id = old.workflow_id;

        // Convert ResourceLimits
        new.resource_limits = ResourceLimits {
            max_memory_bytes: old
                .resource_limits
                .max_memory_bytes
                .unwrap_or(10_000_000_000),
            max_row_count: old.resource_limits.max_rows.unwrap_or(200_000),
            enable_disk_spill: false,
        };

        // Extract rows from working_data if present
        if let Some(rows) = new.working_data.get("_rows").and_then(|v| v.as_array()) {
            let _ = new.set_rows(rows.clone());
        }

        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_execution_context_row_storage() {
        let mut ctx = ExecutionContextV2::new(json!({}));

        // Small dataset
        let small_rows = vec![json!({"id": 1}); 100];
        ctx.set_rows(small_rows.clone()).unwrap();

        let accessor = ctx.get_rows().unwrap();
        assert_eq!(accessor.len(), 100);

        // Verify storage type
        assert_eq!(
            ctx.row_storage.as_ref().unwrap().storage_type(),
            StorageType::InMemory
        );
    }

    #[test]
    fn test_merge_step_output() {
        let mut ctx = ExecutionContextV2::new(json!({}));

        let output = json!({
            "_rows": [{"id": 1}, {"id": 2}],
            "step_metadata": "test",
            "row_count": 2
        });

        ctx.merge_step_output("step1".to_string(), output).unwrap();

        // Rows should be in row_storage
        assert!(ctx.row_storage.is_some());
        assert_eq!(ctx.get_rows().unwrap().len(), 2);

        // Metadata should be in working_data
        assert_eq!(ctx.working_data["step_metadata"], "test");
        assert_eq!(ctx.working_data["row_count"], 2);

        // _rows should not be in working_data
        assert!(ctx.working_data.get("_rows").is_none());
    }

    #[test]
    fn test_resource_limits() {
        let mut ctx = ExecutionContextV2::new(json!({}));
        ctx.resource_limits.max_row_count = 10;

        let too_many_rows = vec![json!({"id": 1}); 20];
        let result = ctx.set_rows(too_many_rows);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds limit"));
    }

    #[test]
    fn test_backwards_compatibility() {
        let mut ctx = ExecutionContextV2::new(json!({
            "_rows": [{"id": 1}, {"id": 2}]
        }));

        // Should find rows in working_data
        let accessor = ctx.get_rows().unwrap();
        assert_eq!(accessor.len(), 2);

        // Should be able to iterate
        let collected: Vec<_> = accessor.iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(collected.len(), 2);
    }
}
