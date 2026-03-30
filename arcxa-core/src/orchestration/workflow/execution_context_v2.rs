//! Optimized execution context with row storage backend
//!
//! This module provides an enhanced ExecutionContext that eliminates expensive
//! clone operations by using tiered row storage.

use super::error::{Result, WorkflowError};
use super::row_lineage_context::RowLineageContext;
#[cfg(feature = "workflow-storage")]
use super::row_storage::StorageManager;
use super::row_storage::{estimate_memory_size, RowAccessor, RowStorage, StorageType};
use super::runtime::frame::BatchFrame;
use super::runtime::metrics::{RuntimeStepMetrics, StorageDecisionMetric, StorageDecisionReason};
use super::runtime::spill::StorageTieringPlan;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "workflow-storage")]
use std::sync::Arc;

const MAX_RECENT_STORAGE_DECISIONS: usize = 32;

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

    /// Cached batch-oriented view of the current row set.
    pub batch_frame: Option<BatchFrame>,

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
    pub tiering_plan_usage: HashMap<StorageTieringPlan, usize>,
    pub storage_decision_reasons: HashMap<StorageDecisionReason, usize>,
    pub spill_events: usize,
    pub spill_bytes: usize,
    pub spill_reserved_bytes_high_water_mark: usize,
    pub execution_spill_reserved_bytes_current: usize,
    pub total_spill_reserved_bytes_current: usize,
    pub recent_storage_decisions: Vec<StorageDecisionMetric>,
}

impl ExecutionContextV2 {
    /// Create a new execution context
    pub fn new(input_data: serde_json::Value) -> Self {
        Self {
            input_data: input_data.clone(),
            working_data: input_data,
            row_storage: None,
            batch_frame: None,
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
            Some(storage) => Ok(RowAccessor::new(storage.clone())),
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

    /// Get the current rows as a batch-oriented frame.
    pub fn get_batch_frame(&self) -> Result<BatchFrame> {
        if let Some(frame) = &self.batch_frame {
            return Ok(frame.clone());
        }

        let rows = self.get_rows()?.to_vec()?;
        BatchFrame::from_json_values(&rows)
    }

    /// Set rows with automatic tiering based on size
    pub fn set_rows(&mut self, rows: Vec<serde_json::Value>) -> Result<()> {
        self.set_rows_with_batch_frame(rows, None)
    }

    fn set_rows_with_batch_frame(
        &mut self,
        rows: Vec<serde_json::Value>,
        batch_frame: Option<BatchFrame>,
    ) -> Result<()> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);
        let planned_tier =
            super::runtime::spill::StorageTieringPolicy::default().plan(row_count, estimated_size);

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
        let (
            storage,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        ) = if let Some(ref manager) = self.storage_manager {
            // Use storage manager for optimal tiering
            let step_id = self.current_step_id.as_deref().unwrap_or("unknown");
            let outcome = manager.store_rows_with_details(&self.execution_id, step_id, rows)?;
            let reason = if matches!(planned_tier, StorageTieringPlan::Parquet)
                && matches!(outcome.storage.storage_type(), StorageType::RocksDB)
            {
                StorageDecisionReason::ParquetFallbackToRocksDb
            } else {
                StorageDecisionReason::Planned
            };
            (
                outcome.storage,
                reason,
                outcome.reserved_spill_bytes,
                outcome.execution_reserved_spill_bytes,
                outcome.total_reserved_spill_bytes,
                outcome.storage_location,
            )
        } else {
            // Fallback to automatic tiering without manager
            let storage = RowStorage::from_rows(rows)?;
            let reason = match planned_tier {
                StorageTieringPlan::InMemory | StorageTieringPlan::Shared => {
                    StorageDecisionReason::Planned
                }
                StorageTieringPlan::RocksDb | StorageTieringPlan::Parquet => {
                    StorageDecisionReason::StorageManagerUnavailable
                }
            };
            (storage, reason, 0, 0, 0, None)
        };

        #[cfg(not(feature = "workflow-storage"))]
        let (
            storage,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        ) = {
            // Automatic tiering without manager
            let storage = RowStorage::from_rows(rows)?;
            let reason = match planned_tier {
                StorageTieringPlan::InMemory | StorageTieringPlan::Shared => {
                    StorageDecisionReason::Planned
                }
                StorageTieringPlan::RocksDb | StorageTieringPlan::Parquet => {
                    StorageDecisionReason::StorageManagerUnavailable
                }
            };
            (storage, reason, 0, 0, 0, None)
        };

        let storage_type = storage.storage_type();
        self.row_storage = Some(storage);
        self.batch_frame = batch_frame;
        self.record_storage_operation("set_rows", storage_type);
        self.record_storage_decision(
            "set_rows",
            planned_tier,
            storage_type,
            row_count,
            estimated_size,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        );

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

    /// Store a batch-oriented frame using the existing row storage path.
    pub fn set_batch_frame(&mut self, frame: BatchFrame) -> Result<()> {
        let rows = frame.to_json_values()?;
        self.set_rows_with_batch_frame(rows, Some(frame))
    }

    /// Store intermediate results for a step
    pub fn store_step_rows(&mut self, step_id: String, rows: Vec<serde_json::Value>) -> Result<()> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);
        let planned_tier =
            super::runtime::spill::StorageTieringPolicy::default().plan(row_count, estimated_size);

        #[cfg(feature = "workflow-storage")]
        let (
            storage,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        ) = if let Some(ref manager) = self.storage_manager {
            let outcome = manager.store_rows_with_details(&self.execution_id, &step_id, rows)?;
            let reason = if matches!(planned_tier, StorageTieringPlan::Parquet)
                && matches!(outcome.storage.storage_type(), StorageType::RocksDB)
            {
                StorageDecisionReason::ParquetFallbackToRocksDb
            } else {
                StorageDecisionReason::Planned
            };
            (
                outcome.storage,
                reason,
                outcome.reserved_spill_bytes,
                outcome.execution_reserved_spill_bytes,
                outcome.total_reserved_spill_bytes,
                outcome.storage_location,
            )
        } else {
            let storage = RowStorage::from_rows(rows)?;
            let reason = match planned_tier {
                StorageTieringPlan::InMemory | StorageTieringPlan::Shared => {
                    StorageDecisionReason::Planned
                }
                StorageTieringPlan::RocksDb | StorageTieringPlan::Parquet => {
                    StorageDecisionReason::StorageManagerUnavailable
                }
            };
            (storage, reason, 0, 0, 0, None)
        };

        #[cfg(not(feature = "workflow-storage"))]
        let (
            storage,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        ) = {
            let storage = RowStorage::from_rows(rows)?;
            let reason = match planned_tier {
                StorageTieringPlan::InMemory | StorageTieringPlan::Shared => {
                    StorageDecisionReason::Planned
                }
                StorageTieringPlan::RocksDb | StorageTieringPlan::Parquet => {
                    StorageDecisionReason::StorageManagerUnavailable
                }
            };
            (storage, reason, 0, 0, 0, None)
        };

        self.record_storage_operation("store_step", storage.storage_type());
        self.record_storage_decision(
            "store_step",
            planned_tier,
            storage.storage_type(),
            row_count,
            estimated_size,
            storage_reason,
            reserved_spill_bytes,
            execution_reserved_spill_bytes,
            total_reserved_spill_bytes,
            storage_location,
        );
        self.step_row_storage.insert(step_id, storage);
        Ok(())
    }

    /// Get rows from a specific step
    pub fn get_step_rows(&self, step_id: &str) -> Option<RowAccessor> {
        self.step_row_storage
            .get(step_id)
            .map(|storage| RowAccessor::new(storage.clone()))
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
                        let spill_row_count = storage.len();
                        let spill_bytes = storage.memory_usage();

                        // Store in RocksDB
                        let step_id = self.current_step_id.as_deref().unwrap_or("spilled");
                        let placement = manager.create_rocks_storage_with_details(
                            &self.execution_id,
                            step_id,
                            rows,
                        )?;

                        self.row_storage = Some(placement.storage);
                        self.record_storage_operation("spill_to_disk", StorageType::RocksDB);
                        self.record_storage_decision(
                            "spill_to_disk",
                            StorageTieringPlan::RocksDb,
                            StorageType::RocksDB,
                            spill_row_count,
                            spill_bytes,
                            StorageDecisionReason::MemoryPressureSpill,
                            placement.reserved_spill_bytes,
                            placement.execution_reserved_spill_bytes,
                            placement.total_reserved_spill_bytes,
                            placement.storage_location,
                        );
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
    fn record_storage_operation(&mut self, operation: &str, storage_type: StorageType) {
        *self
            .metrics
            .storage_operations
            .entry(operation.to_string())
            .or_insert(0) += 1;
        *self
            .metrics
            .storage_type_usage
            .entry(storage_type)
            .or_insert(0) += 1;
    }

    fn record_storage_decision(
        &mut self,
        operation: &str,
        planned_tier: StorageTieringPlan,
        actual_storage_type: StorageType,
        row_count: usize,
        estimated_bytes: usize,
        reason: StorageDecisionReason,
        reserved_spill_bytes: usize,
        execution_reserved_spill_bytes: usize,
        total_reserved_spill_bytes: usize,
        storage_location: Option<String>,
    ) {
        *self
            .metrics
            .tiering_plan_usage
            .entry(planned_tier)
            .or_insert(0) += 1;
        *self
            .metrics
            .storage_decision_reasons
            .entry(reason)
            .or_insert(0) += 1;

        if matches!(reason, StorageDecisionReason::MemoryPressureSpill) {
            self.metrics.spill_events += 1;
            self.metrics.spill_bytes += estimated_bytes;
        }
        self.metrics.execution_spill_reserved_bytes_current = execution_reserved_spill_bytes;
        self.metrics.total_spill_reserved_bytes_current = total_reserved_spill_bytes;
        self.metrics.spill_reserved_bytes_high_water_mark = self
            .metrics
            .spill_reserved_bytes_high_water_mark
            .max(total_reserved_spill_bytes);

        if self.metrics.recent_storage_decisions.len() >= MAX_RECENT_STORAGE_DECISIONS {
            self.metrics.recent_storage_decisions.remove(0);
        }

        self.metrics
            .recent_storage_decisions
            .push(StorageDecisionMetric {
                operation: operation.to_string(),
                planned_tier,
                actual_storage_type,
                row_count,
                estimated_bytes,
                reason,
                reserved_spill_bytes,
                execution_reserved_spill_bytes,
                total_reserved_spill_bytes,
                storage_location: storage_location.clone(),
            });

        match reason {
            StorageDecisionReason::Planned => {
                tracing::debug!(
                    operation,
                    ?planned_tier,
                    actual_storage_type = %actual_storage_type,
                    row_count,
                    estimated_bytes,
                    reserved_spill_bytes,
                    execution_reserved_spill_bytes,
                    total_reserved_spill_bytes,
                    storage_location = storage_location.as_deref().unwrap_or(""),
                    "Recorded storage placement decision"
                );
            }
            _ => {
                tracing::warn!(
                    operation,
                    ?planned_tier,
                    actual_storage_type = %actual_storage_type,
                    row_count,
                    estimated_bytes,
                    reserved_spill_bytes,
                    execution_reserved_spill_bytes,
                    total_reserved_spill_bytes,
                    storage_location = storage_location.as_deref().unwrap_or(""),
                    ?reason,
                    "Recorded storage fallback or spill decision"
                );
            }
        }
    }

    /// Get execution metrics
    pub fn get_metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    /// Build a serializable runtime telemetry snapshot for a single step.
    ///
    /// This surfaces the current storage decision and spill state beyond the
    /// execution context so operator/reporting layers can emit the same storage
    /// signals that the context records internally.
    pub fn build_runtime_step_metrics(
        &self,
        input_rows: usize,
        output_rows: usize,
        materialization_count: usize,
    ) -> RuntimeStepMetrics {
        let latest_decision = self.metrics.recent_storage_decisions.last();

        RuntimeStepMetrics {
            input_rows,
            output_rows,
            materialization_count,
            spill_events: self.metrics.spill_events,
            spill_bytes: self.metrics.spill_bytes,
            memory_high_water_mark: self.metrics.memory_high_water_mark,
            storage_type: self
                .row_storage
                .as_ref()
                .map(|storage| storage.storage_type().to_string()),
            storage_operation: latest_decision.map(|decision| decision.operation.clone()),
            planned_tier: latest_decision.map(|decision| match decision.planned_tier {
                StorageTieringPlan::InMemory => "in_memory".to_string(),
                StorageTieringPlan::Shared => "shared".to_string(),
                StorageTieringPlan::RocksDb => "rocksdb".to_string(),
                StorageTieringPlan::Parquet => "parquet".to_string(),
            }),
            storage_decision_reason: latest_decision.map(|decision| match decision.reason {
                StorageDecisionReason::Planned => "planned".to_string(),
                StorageDecisionReason::StorageManagerUnavailable => {
                    "storage_manager_unavailable".to_string()
                }
                StorageDecisionReason::ParquetFallbackToRocksDb => {
                    "parquet_fallback_to_rocks_db".to_string()
                }
                StorageDecisionReason::MemoryPressureSpill => "memory_pressure_spill".to_string(),
            }),
            reserved_spill_bytes: latest_decision
                .map(|decision| decision.reserved_spill_bytes)
                .unwrap_or(0),
            execution_reserved_spill_bytes: self.metrics.execution_spill_reserved_bytes_current,
            total_reserved_spill_bytes: self.metrics.total_spill_reserved_bytes_current,
            storage_location: latest_decision
                .and_then(|decision| decision.storage_location.clone()),
            pushdown_applied: false,
        }
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
    fn test_batch_frame_bridge_round_trip() {
        let mut ctx = ExecutionContextV2::new(json!({}));
        let rows = vec![
            json!({"id": 1, "name": "alice", "active": true}),
            json!({"id": 2, "name": "bob", "active": false}),
        ];

        let frame = super::BatchFrame::from_json_values(&rows).expect("frame to build");
        ctx.set_batch_frame(frame).expect("frame to store");

        let round_tripped = ctx
            .get_batch_frame()
            .expect("frame to load")
            .to_json_values()
            .expect("frame to convert");

        assert_eq!(round_tripped, rows);
    }

    #[test]
    fn test_batch_frame_bridge_preserves_metadata() {
        let mut ctx = ExecutionContextV2::new(json!({}));
        let rows = vec![json!({"id": 1, "name": "alice"})];

        let frame = super::BatchFrame::from_json_values(&rows)
            .expect("frame to build")
            .with_metadata(super::super::runtime::frame::BatchFrameMetadata {
                source_step_id: Some("extract_1".to_string()),
                source_kind: Some("db_extract".to_string()),
                source_id: None,
            });

        ctx.set_batch_frame(frame).expect("frame to store");

        let round_tripped = ctx.get_batch_frame().expect("frame to load");
        assert_eq!(round_tripped.row_count(), 1);
        assert_eq!(
            round_tripped.metadata().source_step_id.as_deref(),
            Some("extract_1")
        );
        assert_eq!(
            round_tripped.metadata().source_kind.as_deref(),
            Some("db_extract")
        );
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

    #[test]
    fn test_set_rows_records_in_memory_storage_decision_metrics() {
        let mut ctx = ExecutionContextV2::new(json!({}));

        ctx.set_rows(vec![json!({"id": 1}); 100]).unwrap();

        assert_eq!(ctx.metrics.storage_operations.get("set_rows"), Some(&1));
        assert_eq!(
            ctx.metrics.storage_type_usage.get(&StorageType::InMemory),
            Some(&1)
        );
        assert_eq!(
            ctx.metrics
                .tiering_plan_usage
                .get(&StorageTieringPlan::InMemory),
            Some(&1)
        );
        assert_eq!(
            ctx.metrics
                .storage_decision_reasons
                .get(&StorageDecisionReason::Planned),
            Some(&1)
        );

        let decision = ctx
            .metrics
            .recent_storage_decisions
            .last()
            .expect("storage decision to be recorded");
        assert_eq!(decision.operation, "set_rows");
        assert_eq!(decision.planned_tier, StorageTieringPlan::InMemory);
        assert_eq!(decision.actual_storage_type, StorageType::InMemory);
        assert_eq!(decision.reason, StorageDecisionReason::Planned);
    }

    #[test]
    fn test_set_rows_records_storage_manager_unavailable_fallback_metrics() {
        let mut ctx = ExecutionContextV2::new(json!({}));

        ctx.set_rows(vec![json!({"id": 1}); 150_000]).unwrap();

        assert_eq!(
            ctx.row_storage.as_ref().unwrap().storage_type(),
            StorageType::Shared
        );
        assert_eq!(
            ctx.metrics
                .tiering_plan_usage
                .get(&StorageTieringPlan::RocksDb),
            Some(&1)
        );
        assert_eq!(
            ctx.metrics.storage_type_usage.get(&StorageType::Shared),
            Some(&1)
        );
        assert_eq!(
            ctx.metrics
                .storage_decision_reasons
                .get(&StorageDecisionReason::StorageManagerUnavailable),
            Some(&1)
        );

        let decision = ctx
            .metrics
            .recent_storage_decisions
            .last()
            .expect("fallback decision to be recorded");
        assert_eq!(decision.operation, "set_rows");
        assert_eq!(decision.planned_tier, StorageTieringPlan::RocksDb);
        assert_eq!(decision.actual_storage_type, StorageType::Shared);
        assert_eq!(
            decision.reason,
            StorageDecisionReason::StorageManagerUnavailable
        );
    }

    #[cfg(feature = "workflow-storage")]
    #[test]
    fn test_set_rows_with_storage_manager_records_spill_reservation_metrics() {
        use tempfile::tempdir;

        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = Arc::new(
            StorageManager::new(rocks_dir.path(), temp_dir.path()).expect("storage manager"),
        );

        let mut ctx = ExecutionContextV2::with_storage_manager(json!({}), manager);
        ctx.set_current_step("storage_metrics".to_string());
        ctx.set_rows(vec![json!({"id": 1}); 150_000])
            .expect("rows to be stored");

        assert_eq!(
            ctx.row_storage.as_ref().unwrap().storage_type(),
            StorageType::RocksDB
        );
        assert!(ctx.metrics.execution_spill_reserved_bytes_current > 0);
        assert!(ctx.metrics.total_spill_reserved_bytes_current > 0);
        assert!(ctx.metrics.spill_reserved_bytes_high_water_mark > 0);

        let decision = ctx
            .metrics
            .recent_storage_decisions
            .last()
            .expect("storage decision to be recorded");
        assert!(decision.reserved_spill_bytes > 0);
        assert!(decision.execution_reserved_spill_bytes > 0);
        assert!(decision.total_reserved_spill_bytes > 0);
        assert!(decision.storage_location.is_some());
    }

    #[cfg(feature = "workflow-storage")]
    #[test]
    fn test_spill_to_disk_records_spill_metrics() {
        use tempfile::tempdir;

        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = Arc::new(
            StorageManager::new(rocks_dir.path(), temp_dir.path()).expect("storage manager"),
        );

        let mut ctx = ExecutionContextV2::with_storage_manager(json!({}), manager);
        ctx.resource_limits.max_memory_bytes = 1;
        ctx.resource_limits.enable_disk_spill = true;
        ctx.set_current_step("spill_test".to_string());
        ctx.set_rows(vec![json!({"id": 1, "value": "abc"})])
            .expect("rows to be stored");

        ctx.check_memory_pressure().expect("spill to succeed");

        assert_eq!(
            ctx.row_storage.as_ref().unwrap().storage_type(),
            StorageType::RocksDB
        );
        assert_eq!(ctx.metrics.spill_events, 1);
        assert!(ctx.metrics.spill_bytes > 0);
        assert_eq!(
            ctx.metrics
                .storage_decision_reasons
                .get(&StorageDecisionReason::MemoryPressureSpill),
            Some(&1)
        );

        let decision = ctx
            .metrics
            .recent_storage_decisions
            .last()
            .expect("spill decision to be recorded");
        assert_eq!(decision.operation, "spill_to_disk");
        assert_eq!(decision.planned_tier, StorageTieringPlan::RocksDb);
        assert_eq!(decision.actual_storage_type, StorageType::RocksDB);
        assert_eq!(decision.reason, StorageDecisionReason::MemoryPressureSpill);
    }

    #[cfg(feature = "workflow-storage")]
    #[test]
    fn test_build_runtime_step_metrics_surfaces_latest_storage_decision() {
        use tempfile::tempdir;

        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = Arc::new(
            StorageManager::new(rocks_dir.path(), temp_dir.path()).expect("storage manager"),
        );

        let mut ctx = ExecutionContextV2::with_storage_manager(json!({}), manager);
        ctx.set_current_step("runtime_metrics".to_string());
        ctx.set_rows(vec![json!({"id": 1}); 150_000])
            .expect("rows to be stored");

        let runtime_metrics = ctx.build_runtime_step_metrics(150_000, 150_000, 0);
        assert_eq!(runtime_metrics.input_rows, 150_000);
        assert_eq!(runtime_metrics.output_rows, 150_000);
        assert_eq!(runtime_metrics.storage_type.as_deref(), Some("rocksdb"));
        assert_eq!(runtime_metrics.planned_tier.as_deref(), Some("rocksdb"));
        assert_eq!(
            runtime_metrics.storage_decision_reason.as_deref(),
            Some("planned")
        );
        assert!(runtime_metrics.reserved_spill_bytes > 0);
        assert!(runtime_metrics.execution_reserved_spill_bytes > 0);
        assert!(runtime_metrics.total_reserved_spill_bytes > 0);
        assert!(runtime_metrics.storage_location.is_some());
    }
}
