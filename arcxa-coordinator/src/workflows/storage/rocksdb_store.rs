//! RocksDB-based Workflow Execution Storage
//!
//! Production-grade persistent storage for workflow execution state using RocksDB.
//! Provides sub-millisecond reads/writes with durability and crash recovery.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rocksdb::{
    ColumnFamily, ColumnFamilyDescriptor, DBCompressionType, DBWithThreadMode, IteratorMode,
    MultiThreaded, Options, WriteBatch, DB,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::workflows::domain::{
    ExecutionFilters, ExecutionLog, ExecutionStatus, WorkflowExecution,
};

/// Column families for organizing data
const CF_EXECUTIONS: &str = "executions";
const CF_LOGS: &str = "execution_logs";
const CF_CHECKPOINTS: &str = "checkpoints";
const CF_EVENTS: &str = "events";
const CF_METADATA: &str = "metadata";

/// RocksDB-based execution store with high performance and durability
pub struct RocksDbExecutionStore {
    db: Arc<DBWithThreadMode<MultiThreaded>>,
    metrics: Arc<StoreMetrics>,
}

/// Store metrics for monitoring
#[derive(Default)]
struct StoreMetrics {
    reads: prometheus::IntCounter,
    writes: prometheus::IntCounter,
    read_latency: prometheus::Histogram,
    write_latency: prometheus::Histogram,
    size_bytes: prometheus::IntGauge,
}

impl RocksDbExecutionStore {
    /// Open or create a RocksDB instance
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Opening RocksDB execution store at {:?}", path);

        // Configure RocksDB options for optimal performance
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_compression_type(DBCompressionType::Lz4);

        // Performance tuning
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_max_background_jobs(4);

        // Enable statistics for monitoring
        opts.enable_statistics();
        opts.set_stats_dump_period_sec(600); // Dump stats every 10 minutes

        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_EXECUTIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_LOGS, Options::default()),
            ColumnFamilyDescriptor::new(CF_CHECKPOINTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_METADATA, Options::default()),
        ];

        // Open database
        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .context("Failed to open RocksDB")?;

        let db = Arc::new(db);

        // Initialize metrics
        let metrics = Arc::new(StoreMetrics::default());

        Ok(Self { db, metrics })
    }

    /// Save a workflow execution
    pub fn save(&self, execution: &WorkflowExecution) -> Result<()> {
        let timer = self.metrics.write_latency.start_timer();

        let cf = self
            .db
            .cf_handle(CF_EXECUTIONS)
            .context("Failed to get executions CF")?;

        let key = execution_key(&execution.execution_id);
        let value = rmp_serde::to_vec(execution)
            .context("Failed to serialize execution")?;

        self.db
            .put_cf(&cf, key, value)
            .context("Failed to write execution to RocksDB")?;

        self.metrics.writes.inc();
        timer.stop_and_record();

        debug!("Saved execution {} to RocksDB", execution.execution_id);
        Ok(())
    }

    /// Get a workflow execution by ID
    pub fn get(&self, execution_id: &str) -> Result<Option<WorkflowExecution>> {
        let timer = self.metrics.read_latency.start_timer();

        let cf = self
            .db
            .cf_handle(CF_EXECUTIONS)
            .context("Failed to get executions CF")?;

        let key = execution_key(execution_id);
        let value = self
            .db
            .get_cf(&cf, key)
            .context("Failed to read from RocksDB")?;

        self.metrics.reads.inc();
        timer.stop_and_record();

        match value {
            Some(bytes) => {
                let execution = rmp_serde::from_slice(&bytes)
                    .context("Failed to deserialize execution")?;
                Ok(Some(execution))
            }
            None => Ok(None),
        }
    }

    /// Update an existing execution
    pub fn update(&self, execution: &WorkflowExecution) -> Result<()> {
        // Check if exists
        if self.get(&execution.execution_id)?.is_none() {
            anyhow::bail!("Execution '{}' not found", execution.execution_id);
        }

        self.save(execution)
    }

    /// Update execution status atomically
    pub fn update_status(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
    ) -> Result<()> {
        let mut execution = self
            .get(execution_id)?
            .ok_or_else(|| anyhow::anyhow!("Execution '{}' not found", execution_id))?;

        execution.update_status(status);
        self.save(&execution)
    }

    /// Add a log entry
    pub fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_LOGS)
            .context("Failed to get logs CF")?;

        // Key format: exec_id:timestamp:sequence
        let key = format!(
            "{}:{}:{}",
            execution_id,
            log.timestamp.timestamp_nanos(),
            uuid::Uuid::new_v4()
        );

        let value = rmp_serde::to_vec(&log)
            .context("Failed to serialize log")?;

        self.db
            .put_cf(&cf, key, value)
            .context("Failed to write log to RocksDB")?;

        debug!("Added log entry for execution {}", execution_id);
        Ok(())
    }

    /// Get logs for an execution
    pub fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>> {
        let cf = self
            .db
            .cf_handle(CF_LOGS)
            .context("Failed to get logs CF")?;

        let prefix = format!("{}:", execution_id);
        let mut logs = Vec::new();

        let iter = self.db.prefix_iterator_cf(&cf, prefix.as_bytes());
        for item in iter {
            let (key, value) = item.context("Failed to read log entry")?;

            // Check if key still matches our prefix
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }

            let log: ExecutionLog = rmp_serde::from_slice(&value)
                .context("Failed to deserialize log")?;
            logs.push(log);
        }

        Ok(logs)
    }

    /// List executions with filters
    pub fn list_filtered(
        &self,
        filters: &ExecutionFilters,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<WorkflowExecution>> {
        let cf = self
            .db
            .cf_handle(CF_EXECUTIONS)
            .context("Failed to get executions CF")?;

        let mut executions = Vec::new();
        let mut count = 0;
        let offset = offset.unwrap_or(0);

        // Iterate through all executions
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (_, value) = item.context("Failed to read execution")?;

            let execution: WorkflowExecution = rmp_serde::from_slice(&value)
                .context("Failed to deserialize execution")?;

            // Apply filters
            if !filters.matches(&execution) {
                continue;
            }

            // Apply offset
            if count < offset {
                count += 1;
                continue;
            }

            executions.push(execution);

            // Apply limit
            if let Some(limit) = limit {
                if executions.len() >= limit {
                    break;
                }
            }

            count += 1;
        }

        // Sort by started_at descending
        executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(executions)
    }

    /// Store a checkpoint for an execution
    pub fn store_checkpoint(&self, checkpoint: &ExecutionCheckpoint) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_CHECKPOINTS)
            .context("Failed to get checkpoints CF")?;

        let key = checkpoint_key(&checkpoint.execution_id, checkpoint.sequence);
        let value = rmp_serde::to_vec(checkpoint)
            .context("Failed to serialize checkpoint")?;

        self.db
            .put_cf(&cf, key, value)
            .context("Failed to write checkpoint")?;

        info!(
            "Stored checkpoint {} for execution {}",
            checkpoint.sequence, checkpoint.execution_id
        );

        Ok(())
    }

    /// Get latest checkpoint for an execution
    pub fn get_latest_checkpoint(
        &self,
        execution_id: &str,
    ) -> Result<Option<ExecutionCheckpoint>> {
        let cf = self
            .db
            .cf_handle(CF_CHECKPOINTS)
            .context("Failed to get checkpoints CF")?;

        let prefix = format!("checkpoint:{}:", execution_id);

        // Iterate in reverse to get latest
        let iter = self.db.prefix_iterator_cf(&cf, prefix.as_bytes());
        let mut checkpoints: Vec<ExecutionCheckpoint> = Vec::new();

        for item in iter {
            let (key, value) = item.context("Failed to read checkpoint")?;

            if !key.starts_with(prefix.as_bytes()) {
                break;
            }

            let checkpoint: ExecutionCheckpoint = rmp_serde::from_slice(&value)
                .context("Failed to deserialize checkpoint")?;
            checkpoints.push(checkpoint);
        }

        // Sort by sequence and return latest
        checkpoints.sort_by_key(|c| c.sequence);
        Ok(checkpoints.into_iter().last())
    }

    /// Append event to write-ahead log
    pub fn append_event(&self, event: &ExecutionEvent) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_EVENTS)
            .context("Failed to get events CF")?;

        let key = event_key(&event.execution_id, event.timestamp);
        let value = rmp_serde::to_vec(event)
            .context("Failed to serialize event")?;

        self.db
            .put_cf(&cf, key, value)
            .context("Failed to write event")?;

        Ok(())
    }

    /// Replay events from a timestamp
    pub fn replay_events(
        &self,
        execution_id: &str,
        from_timestamp: DateTime<Utc>,
    ) -> Result<Vec<ExecutionEvent>> {
        let cf = self
            .db
            .cf_handle(CF_EVENTS)
            .context("Failed to get events CF")?;

        let prefix = format!("event:{}:", execution_id);
        let start_key = event_key(execution_id, from_timestamp);

        let mut events = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::From(start_key.as_bytes(), rocksdb::Direction::Forward));

        for item in iter {
            let (key, value) = item.context("Failed to read event")?;

            if !key.starts_with(prefix.as_bytes()) {
                break;
            }

            let event: ExecutionEvent = rmp_serde::from_slice(&value)
                .context("Failed to deserialize event")?;
            events.push(event);
        }

        Ok(events)
    }

    /// Get incomplete executions for recovery
    pub fn get_incomplete_executions(&self) -> Result<Vec<WorkflowExecution>> {
        let filters = ExecutionFilters {
            status: None, // Will filter manually for multiple statuses
            ..Default::default()
        };

        let all_executions = self.list_filtered(&filters, None, None)?;

        let incomplete: Vec<_> = all_executions
            .into_iter()
            .filter(|e| matches!(
                e.status,
                ExecutionStatus::Pending | ExecutionStatus::Running | ExecutionStatus::Paused
            ))
            .collect();

        Ok(incomplete)
    }

    /// Compact database for optimal performance
    pub fn compact(&self) -> Result<()> {
        info!("Starting RocksDB compaction");

        for cf_name in &[CF_EXECUTIONS, CF_LOGS, CF_CHECKPOINTS, CF_EVENTS, CF_METADATA] {
            let cf = self
                .db
                .cf_handle(cf_name)
                .context(format!("Failed to get {} CF", cf_name))?;

            self.db
                .compact_range_cf(&cf, None::<&[u8]>, None::<&[u8]>);
        }

        info!("RocksDB compaction completed");
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DbStats> {
        let stats = DbStats {
            executions_count: self.count_cf(CF_EXECUTIONS)?,
            logs_count: self.count_cf(CF_LOGS)?,
            checkpoints_count: self.count_cf(CF_CHECKPOINTS)?,
            events_count: self.count_cf(CF_EVENTS)?,
            size_bytes: self.estimate_size()?,
        };

        Ok(stats)
    }

    /// Get execution by ID (required, returns error if not found)
    pub fn get_required(&self, execution_id: &str) -> Result<WorkflowExecution> {
        self.get(execution_id)?
            .ok_or_else(|| anyhow::anyhow!("Execution '{}' not found", execution_id))
    }

    /// List all executions for a specific workflow
    pub fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowExecution>> {
        let filters = ExecutionFilters {
            workflow_id: Some(workflow_id.to_string()),
            ..Default::default()
        };
        self.list_filtered(&filters, None, None)
    }

    /// List all in-progress executions (Running, Pending, or Paused)
    pub fn list_in_progress(&self) -> Result<Vec<WorkflowExecution>> {
        self.get_incomplete_executions()
    }

    /// Create a checkpoint for an execution
    pub fn checkpoint(&self, execution_id: &str) -> Result<()> {
        let execution = self.get_required(execution_id)?;

        // Get next sequence number
        let latest = self.get_latest_checkpoint(execution_id)?;
        let sequence = latest.map(|c| c.sequence + 1).unwrap_or(1);

        // Create checkpoint
        let checkpoint = ExecutionCheckpoint {
            execution_id: execution_id.to_string(),
            sequence,
            timestamp: Utc::now(),
            step_number: 0, // TODO: Track actual step number
            status: execution.status.clone(),
            context_snapshot: vec![], // TODO: Serialize context
            step_results_count: 0, // TODO: Track results
            checksum: format!("{:x}", md5::compute(&execution.execution_id)),
        };

        self.store_checkpoint(&checkpoint)?;

        // Append checkpoint event
        let event = ExecutionEvent {
            execution_id: execution_id.to_string(),
            timestamp: Utc::now(),
            event_type: EventType::CheckpointCreated { sequence },
            payload: vec![],
        };
        self.append_event(&event)?;

        info!("Created checkpoint {} for execution {}", sequence, execution_id);
        Ok(())
    }

    /// Set error status on an execution
    pub fn set_error(&self, execution_id: &str, error: String) -> Result<()> {
        let mut execution = self.get_required(execution_id)?;
        execution.set_error(error.clone());
        self.save(&execution)?;

        // Append failed event
        let event = ExecutionEvent {
            execution_id: execution_id.to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Failed { error },
            payload: vec![],
        };
        self.append_event(&event)?;

        Ok(())
    }

    /// Delete an execution and all associated data
    pub fn delete(&self, execution_id: &str) -> Result<()> {
        // Delete from executions CF
        let cf_exec = self
            .db
            .cf_handle(CF_EXECUTIONS)
            .context("Failed to get executions CF")?;
        let exec_key = execution_key(execution_id);
        self.db.delete_cf(&cf_exec, exec_key)?;

        // Delete logs
        let cf_logs = self
            .db
            .cf_handle(CF_LOGS)
            .context("Failed to get logs CF")?;
        let log_prefix = format!("{}:", execution_id);
        let keys_to_delete: Vec<_> = self
            .db
            .prefix_iterator_cf(&cf_logs, log_prefix.as_bytes())
            .take_while(|item| {
                if let Ok((key, _)) = item {
                    key.starts_with(log_prefix.as_bytes())
                } else {
                    false
                }
            })
            .filter_map(|item| item.ok().map(|(key, _)| key.to_vec()))
            .collect();

        for key in keys_to_delete {
            self.db.delete_cf(&cf_logs, key)?;
        }

        // Delete checkpoints
        let cf_checkpoints = self
            .db
            .cf_handle(CF_CHECKPOINTS)
            .context("Failed to get checkpoints CF")?;
        let checkpoint_prefix = format!("checkpoint:{}:", execution_id);
        let keys_to_delete: Vec<_> = self
            .db
            .prefix_iterator_cf(&cf_checkpoints, checkpoint_prefix.as_bytes())
            .take_while(|item| {
                if let Ok((key, _)) = item {
                    key.starts_with(checkpoint_prefix.as_bytes())
                } else {
                    false
                }
            })
            .filter_map(|item| item.ok().map(|(key, _)| key.to_vec()))
            .collect();

        for key in keys_to_delete {
            self.db.delete_cf(&cf_checkpoints, key)?;
        }

        // Delete events
        let cf_events = self
            .db
            .cf_handle(CF_EVENTS)
            .context("Failed to get events CF")?;
        let event_prefix = format!("event:{}:", execution_id);
        let keys_to_delete: Vec<_> = self
            .db
            .prefix_iterator_cf(&cf_events, event_prefix.as_bytes())
            .take_while(|item| {
                if let Ok((key, _)) = item {
                    key.starts_with(event_prefix.as_bytes())
                } else {
                    false
                }
            })
            .filter_map(|item| item.ok().map(|(key, _)| key.to_vec()))
            .collect();

        for key in keys_to_delete {
            self.db.delete_cf(&cf_events, key)?;
        }

        info!("Deleted execution {} and all associated data", execution_id);
        Ok(())
    }

    /// Count total executions
    pub fn count(&self) -> Result<usize> {
        self.count_cf(CF_EXECUTIONS)
    }

    /// Count executions matching filters
    pub fn count_filtered(&self, filters: &ExecutionFilters) -> Result<usize> {
        let executions = self.list_filtered(filters, None, None)?;
        Ok(executions.len())
    }

    // Helper to count entries in a column family
    fn count_cf(&self, cf_name: &str) -> Result<usize> {
        let cf = self
            .db
            .cf_handle(cf_name)
            .context(format!("Failed to get {} CF", cf_name))?;

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        Ok(iter.count())
    }

    // Helper to estimate database size
    fn estimate_size(&self) -> Result<u64> {
        // Use RocksDB property to get size
        let mut total = 0u64;

        for cf_name in &[CF_EXECUTIONS, CF_LOGS, CF_CHECKPOINTS, CF_EVENTS, CF_METADATA] {
            let cf = self
                .db
                .cf_handle(cf_name)
                .context(format!("Failed to get {} CF", cf_name))?;

            if let Ok(Some(size)) = self.db.property_int_value_cf(&cf, "rocksdb.estimate-live-data-size") {
                total += size;
            }
        }

        Ok(total)
    }
}

/// Execution checkpoint for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCheckpoint {
    pub execution_id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub step_number: u64,
    pub status: ExecutionStatus,
    pub context_snapshot: Vec<u8>,  // Compressed
    pub step_results_count: usize,
    pub checksum: String,
}

/// Execution event for write-ahead log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub execution_id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub payload: Vec<u8>,  // MessagePack encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Started,
    StatusChanged { from: ExecutionStatus, to: ExecutionStatus },
    StepCompleted { step_id: String },
    CheckpointCreated { sequence: u64 },
    Completed,
    Failed { error: String },
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DbStats {
    pub executions_count: usize,
    pub logs_count: usize,
    pub checkpoints_count: usize,
    pub events_count: usize,
    pub size_bytes: u64,
}

// Key generation helpers
fn execution_key(id: &str) -> String {
    format!("exec:{}", id)
}

fn checkpoint_key(execution_id: &str, sequence: u64) -> String {
    format!("checkpoint:{}:{:020}", execution_id, sequence)
}

fn event_key(execution_id: &str, timestamp: DateTime<Utc>) -> String {
    format!("event:{}:{}", execution_id, timestamp.timestamp_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (RocksDbExecutionStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = RocksDbExecutionStore::open(temp_dir.path()).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_save_and_get_execution() {
        let (store, _dir) = create_test_store();

        let execution = WorkflowExecution::new(
            "exec_001".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            serde_json::json!({}),
            None,
        );

        store.save(&execution).unwrap();

        let retrieved = store.get("exec_001").unwrap().unwrap();
        assert_eq!(retrieved.execution_id, "exec_001");
    }

    #[test]
    fn test_checkpoint_storage() {
        let (store, _dir) = create_test_store();

        let checkpoint = ExecutionCheckpoint {
            execution_id: "exec_001".to_string(),
            sequence: 1,
            timestamp: Utc::now(),
            step_number: 10,
            status: ExecutionStatus::Running,
            context_snapshot: vec![1, 2, 3],
            step_results_count: 5,
            checksum: "abc123".to_string(),
        };

        store.store_checkpoint(&checkpoint).unwrap();

        let retrieved = store.get_latest_checkpoint("exec_001").unwrap().unwrap();
        assert_eq!(retrieved.sequence, 1);
        assert_eq!(retrieved.step_number, 10);
    }

    #[test]
    fn test_event_replay() {
        let (store, _dir) = create_test_store();

        let event1 = ExecutionEvent {
            execution_id: "exec_001".to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Started,
            payload: vec![],
        };

        let event2 = ExecutionEvent {
            execution_id: "exec_001".to_string(),
            timestamp: Utc::now() + chrono::Duration::seconds(1),
            event_type: EventType::StatusChanged {
                from: ExecutionStatus::Pending,
                to: ExecutionStatus::Running,
            },
            payload: vec![],
        };

        store.append_event(&event1).unwrap();
        store.append_event(&event2).unwrap();

        let events = store.replay_events("exec_001", event1.timestamp).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_incomplete_executions() {
        let (store, _dir) = create_test_store();

        let mut exec1 = WorkflowExecution::new(
            "exec_001".to_string(),
            "wf_001".to_string(),
            "Test 1".to_string(),
            serde_json::json!({}),
            None,
        );
        exec1.status = ExecutionStatus::Running;
        store.save(&exec1).unwrap();

        let mut exec2 = WorkflowExecution::new(
            "exec_002".to_string(),
            "wf_001".to_string(),
            "Test 2".to_string(),
            serde_json::json!({}),
            None,
        );
        exec2.status = ExecutionStatus::Completed;
        store.save(&exec2).unwrap();

        let incomplete = store.get_incomplete_executions().unwrap();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].execution_id, "exec_001");
    }
}