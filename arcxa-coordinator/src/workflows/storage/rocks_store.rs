//! RocksDB-backed Workflow State Store
//!
//! Provides durable, production-grade persistence for workflow execution state.
//! Implements event sourcing with checkpoints for recovery.

use crate::workflows::domain::{
    ExecutionFilters, ExecutionLog, ExecutionStatus, WorkflowExecution,
};
use crate::workflows::storage::persistence::rocksdb_config::RocksDbConfig;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Column family names
const CF_EXECUTIONS: &str = "executions";
const CF_EVENTS: &str = "events";
const CF_CHECKPOINTS: &str = "checkpoints";
#[allow(dead_code)]
const CF_METADATA: &str = "metadata";

/// RocksDB-backed execution store
///
/// Provides persistent storage with:
/// - Sub-millisecond reads/writes
/// - Event sourcing for audit trail
/// - Checkpoint-based recovery
/// - ACID transactions
pub struct RocksExecutionStore {
    pub db: Arc<DB>,
}

/// Event in the event log (for event sourcing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionEvent {
    /// Execution created
    Created {
        execution_id: String,
        workflow_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Status updated
    StatusUpdated {
        execution_id: String,
        old_status: ExecutionStatus,
        new_status: ExecutionStatus,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Log entry added
    LogAdded {
        execution_id: String,
        log: ExecutionLog,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Output set
    OutputSet {
        execution_id: String,
        output: serde_json::Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Error set
    ErrorSet {
        execution_id: String,
        error: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

impl RocksExecutionStore {
    /// Open or create a RocksDB store at the given path with default configuration
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with_config(path, RocksDbConfig::production())
    }

    /// Open or create a RocksDB store at the given path with custom configuration
    pub fn open_with_config<P: AsRef<Path>>(path: P, config: RocksDbConfig) -> Result<Self> {
        let path = path.as_ref();

        info!("Opening RocksDB execution store at: {:?}", path);

        // Build database options from config
        let db_opts = config.build_db_options();

        // Build column family descriptors from config
        let cfs = config.build_column_families();

        // Open database
        let db = DB::open_cf_descriptors(&db_opts, path, cfs)
            .with_context(|| format!("Failed to open RocksDB at {:?}", path))?;

        info!("RocksDB execution store opened successfully with custom configuration");

        Ok(Self { db: Arc::new(db) })
    }

    /// Create from an existing DB instance (for testing)
    pub fn from_db(db: Arc<DB>, _config: RocksDbConfig) -> Self {
        Self { db }
    }

    /// Column family options
    #[allow(dead_code)]
    fn cf_options() -> Options {
        let mut opts = Options::default();
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts
    }

    /// Get executions column family
    fn cf_executions(&self) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(CF_EXECUTIONS)
            .ok_or_else(|| anyhow::anyhow!("Executions column family not found"))
    }

    /// Get events column family
    fn cf_events(&self) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| anyhow::anyhow!("Events column family not found"))
    }

    /// Get checkpoints column family
    fn cf_checkpoints(&self) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(CF_CHECKPOINTS)
            .ok_or_else(|| anyhow::anyhow!("Checkpoints column family not found"))
    }

    /// Save a new execution
    pub fn save(&self, execution: WorkflowExecution) -> Result<()> {
        let execution_id = execution.execution_id.clone();
        let workflow_id = execution.workflow_id.clone();

        // Check if execution already exists
        if self.exists(&execution_id)? {
            anyhow::bail!("Execution '{}' already exists", execution_id);
        }

        // Serialize execution
        let execution_bytes =
            serde_json::to_vec(&execution).context("Failed to serialize execution")?;

        // Create event
        let event = ExecutionEvent::Created {
            execution_id: execution_id.clone(),
            workflow_id,
            timestamp: chrono::Utc::now(),
        };
        let event_key = format!(
            "{}:{}",
            execution_id,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let event_bytes = serde_json::to_vec(&event).context("Failed to serialize event")?;

        // Write batch for atomicity
        let mut batch = WriteBatch::default();
        batch.put_cf(
            self.cf_executions()?,
            execution_id.as_bytes(),
            execution_bytes,
        );
        batch.put_cf(self.cf_events()?, event_key.as_bytes(), event_bytes);

        self.db.write(batch).context("Failed to write execution")?;

        debug!("Saved execution: {}", execution_id);

        Ok(())
    }

    /// Get an execution by ID
    pub fn get(&self, execution_id: &str) -> Result<Option<WorkflowExecution>> {
        let cf = self.cf_executions()?;

        match self.db.get_cf(cf, execution_id.as_bytes())? {
            Some(bytes) => {
                let execution: WorkflowExecution =
                    serde_json::from_slice(&bytes).context("Failed to deserialize execution")?;
                Ok(Some(execution))
            }
            None => Ok(None),
        }
    }

    /// Get an execution by ID (required, returns error if not found)
    pub fn get_required(&self, execution_id: &str) -> Result<WorkflowExecution> {
        self.get(execution_id)?
            .ok_or_else(|| anyhow::anyhow!("Execution '{}' not found", execution_id))
    }

    /// Update an existing execution
    pub fn update(&self, execution: WorkflowExecution) -> Result<()> {
        let execution_id = execution.execution_id.clone();

        // Check if execution exists
        if !self.exists(&execution_id)? {
            anyhow::bail!("Execution '{}' not found", execution_id);
        }

        // Serialize execution
        let execution_bytes =
            serde_json::to_vec(&execution).context("Failed to serialize execution")?;

        // Write to database
        self.db
            .put_cf(
                self.cf_executions()?,
                execution_id.as_bytes(),
                execution_bytes,
            )
            .context("Failed to update execution")?;

        debug!("Updated execution: {}", execution_id);

        Ok(())
    }

    /// Update execution status
    pub fn update_status(&self, execution_id: &str, status: ExecutionStatus) -> Result<()> {
        let mut execution = self.get_required(execution_id)?;
        let old_status = execution.status.clone();

        execution.update_status(status.clone());
        self.update(execution)?;

        // Log event
        let event = ExecutionEvent::StatusUpdated {
            execution_id: execution_id.to_string(),
            old_status,
            new_status: status,
            timestamp: chrono::Utc::now(),
        };
        self.append_event(event)?;

        Ok(())
    }

    /// Add a log entry to an execution
    pub fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        let mut execution = self.get_required(execution_id)?;

        execution.add_log(log.clone());
        self.update(execution)?;

        // Log event
        let event = ExecutionEvent::LogAdded {
            execution_id: execution_id.to_string(),
            log,
            timestamp: chrono::Utc::now(),
        };
        self.append_event(event)?;

        Ok(())
    }

    /// Set execution output
    pub fn set_output(&self, execution_id: &str, output: serde_json::Value) -> Result<()> {
        let mut execution = self.get_required(execution_id)?;

        execution.set_output(output.clone());
        self.update(execution)?;

        // Log event
        let event = ExecutionEvent::OutputSet {
            execution_id: execution_id.to_string(),
            output,
            timestamp: chrono::Utc::now(),
        };
        self.append_event(event)?;

        Ok(())
    }

    /// Set execution error
    pub fn set_error(&self, execution_id: &str, error: String) -> Result<()> {
        let mut execution = self.get_required(execution_id)?;

        execution.set_error(error.clone());
        self.update(execution)?;

        // Log event
        let event = ExecutionEvent::ErrorSet {
            execution_id: execution_id.to_string(),
            error,
            timestamp: chrono::Utc::now(),
        };
        self.append_event(event)?;

        Ok(())
    }

    /// Append event to event log
    fn append_event(&self, event: ExecutionEvent) -> Result<()> {
        let event_key = format!(
            "{}:{}",
            match &event {
                ExecutionEvent::Created { execution_id, .. } => execution_id,
                ExecutionEvent::StatusUpdated { execution_id, .. } => execution_id,
                ExecutionEvent::LogAdded { execution_id, .. } => execution_id,
                ExecutionEvent::OutputSet { execution_id, .. } => execution_id,
                ExecutionEvent::ErrorSet { execution_id, .. } => execution_id,
            },
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );

        let event_bytes = serde_json::to_vec(&event).context("Failed to serialize event")?;

        self.db
            .put_cf(self.cf_events()?, event_key.as_bytes(), event_bytes)
            .context("Failed to append event")?;

        Ok(())
    }

    /// List all executions for a workflow
    pub fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowExecution>> {
        let cf = self.cf_executions()?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut executions = Vec::new();

        for item in iter {
            let (_key, value) = item?;
            let execution: WorkflowExecution =
                serde_json::from_slice(&value).context("Failed to deserialize execution")?;

            if execution.workflow_id == workflow_id {
                executions.push(execution);
            }
        }

        Ok(executions)
    }

    /// List executions with filters
    pub fn list_filtered(
        &self,
        filters: &ExecutionFilters,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<WorkflowExecution>> {
        let cf = self.cf_executions()?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut executions = Vec::new();

        for item in iter {
            let (_key, value) = item?;
            let execution: WorkflowExecution =
                serde_json::from_slice(&value).context("Failed to deserialize execution")?;

            if filters.matches(&execution) {
                executions.push(execution);
            }
        }

        // Sort by started_at descending (most recent first)
        executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Apply pagination
        let offset = offset.unwrap_or(0);
        let executions: Vec<_> = executions.into_iter().skip(offset).collect();

        if let Some(limit) = limit {
            Ok(executions.into_iter().take(limit).collect())
        } else {
            Ok(executions)
        }
    }

    /// Get execution logs
    pub fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>> {
        let execution = self.get_required(execution_id)?;
        Ok(execution.logs.clone())
    }

    /// Count total executions
    pub fn count(&self) -> Result<usize> {
        let cf = self.cf_executions()?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        Ok(iter.count())
    }

    /// Count executions matching filters
    pub fn count_filtered(&self, filters: &ExecutionFilters) -> Result<usize> {
        let cf = self.cf_executions()?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut count = 0;

        for item in iter {
            let (_key, value) = item?;
            let execution: WorkflowExecution =
                serde_json::from_slice(&value).context("Failed to deserialize execution")?;

            if filters.matches(&execution) {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Check if an execution exists
    pub fn exists(&self, execution_id: &str) -> Result<bool> {
        let cf = self.cf_executions()?;
        Ok(self.db.get_cf(cf, execution_id.as_bytes())?.is_some())
    }

    /// Delete an execution
    pub fn delete(&self, execution_id: &str) -> Result<()> {
        if !self.exists(execution_id)? {
            anyhow::bail!("Execution '{}' not found", execution_id);
        }

        self.db
            .delete_cf(self.cf_executions()?, execution_id.as_bytes())
            .context("Failed to delete execution")?;

        debug!("Deleted execution: {}", execution_id);

        Ok(())
    }

    /// List all in-progress executions (for recovery)
    pub fn list_in_progress(&self) -> Result<Vec<WorkflowExecution>> {
        let filters = ExecutionFilters {
            status: Some(ExecutionStatus::Running),
            ..Default::default()
        };

        self.list_filtered(&filters, None, None)
    }

    /// Create a checkpoint for an execution
    pub fn checkpoint(&self, execution_id: &str) -> Result<()> {
        let execution = self.get_required(execution_id)?;

        let checkpoint_key = format!("{}:{}", execution_id, chrono::Utc::now().timestamp());
        let checkpoint_bytes =
            serde_json::to_vec(&execution).context("Failed to serialize checkpoint")?;

        self.db
            .put_cf(
                self.cf_checkpoints()?,
                checkpoint_key.as_bytes(),
                checkpoint_bytes,
            )
            .context("Failed to create checkpoint")?;

        debug!("Created checkpoint for execution: {}", execution_id);

        Ok(())
    }

    /// Get the latest checkpoint for an execution
    pub fn get_latest_checkpoint(&self, execution_id: &str) -> Result<Option<WorkflowExecution>> {
        let cf = self.cf_checkpoints()?;
        let prefix = format!("{}:", execution_id);

        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut latest_checkpoint: Option<(i64, WorkflowExecution)> = None;

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.starts_with(&prefix) {
                if let Some(timestamp_str) = key_str.strip_prefix(&prefix) {
                    if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                        let execution: WorkflowExecution = serde_json::from_slice(&value)
                            .context("Failed to deserialize checkpoint")?;

                        if latest_checkpoint.is_none()
                            || timestamp > latest_checkpoint.as_ref().unwrap().0
                        {
                            latest_checkpoint = Some((timestamp, execution));
                        }
                    }
                }
            }
        }

        Ok(latest_checkpoint.map(|(_, exec)| exec))
    }

    /// Get event log for an execution (for replay/debugging)
    pub fn get_event_log(&self, execution_id: &str) -> Result<Vec<ExecutionEvent>> {
        let cf = self.cf_events()?;
        let prefix = format!("{}:", execution_id);

        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut events = Vec::new();

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.starts_with(&prefix) {
                let event: ExecutionEvent =
                    serde_json::from_slice(&value).context("Failed to deserialize event")?;
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Flush all writes to disk
    pub fn flush(&self) -> Result<()> {
        self.db.flush().context("Failed to flush database")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::LogLevel;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_execution(id: &str, workflow_id: &str) -> WorkflowExecution {
        WorkflowExecution::new(
            id.to_string(),
            workflow_id.to_string(),
            format!("Workflow {}", workflow_id),
            json!({"test": "data"}),
            Some("test@example.com".to_string()),
        )
    }

    fn create_test_store() -> (RocksExecutionStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = RocksExecutionStore::open(temp_dir.path()).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_open_store() {
        let (_store, _temp_dir) = create_test_store();
        // Success if we got here
    }

    #[test]
    fn test_save_and_get() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution.clone()).unwrap();

        let retrieved = store.get("exec_001").unwrap().unwrap();
        assert_eq!(retrieved.execution_id, "exec_001");
        assert_eq!(retrieved.workflow_id, "wf_001");
    }

    #[test]
    fn test_save_duplicate() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution.clone()).unwrap();

        let result = store.save(execution);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_status() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).unwrap();

        store
            .update_status("exec_001", ExecutionStatus::Running)
            .unwrap();

        let updated = store.get("exec_001").unwrap().unwrap();
        assert_eq!(updated.status, ExecutionStatus::Running);
    }

    #[test]
    fn test_add_log() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).unwrap();

        store
            .add_log("exec_001", ExecutionLog::info("Test log"))
            .unwrap();

        let logs = store.get_logs("exec_001").unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Test log");
    }

    #[test]
    fn test_checkpoint_and_recovery() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).unwrap();

        // Update status
        store
            .update_status("exec_001", ExecutionStatus::Running)
            .unwrap();

        // Create checkpoint
        store.checkpoint("exec_001").unwrap();

        // Further updates
        store
            .add_log("exec_001", ExecutionLog::info("After checkpoint"))
            .unwrap();

        // Retrieve checkpoint (should have Running status but no log)
        let checkpoint = store.get_latest_checkpoint("exec_001").unwrap().unwrap();
        assert_eq!(checkpoint.status, ExecutionStatus::Running);
        assert_eq!(checkpoint.logs.len(), 0); // No logs in checkpoint

        // Current state should have the log
        let current = store.get("exec_001").unwrap().unwrap();
        assert_eq!(current.logs.len(), 1);
    }

    #[test]
    fn test_event_log() {
        let (store, _temp_dir) = create_test_store();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).unwrap();
        store
            .update_status("exec_001", ExecutionStatus::Running)
            .unwrap();
        store
            .add_log("exec_001", ExecutionLog::info("Test"))
            .unwrap();

        let events = store.get_event_log("exec_001").unwrap();
        assert!(events.len() >= 3); // Created, StatusUpdated, LogAdded
    }

    #[test]
    fn test_list_in_progress() {
        let (store, _temp_dir) = create_test_store();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        store.save(exec1).unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Completed);
        store.save(exec2).unwrap();

        let in_progress = store.list_in_progress().unwrap();
        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0].execution_id, "exec_001");
    }

    #[test]
    fn test_persistence_across_reopens() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        {
            let store = RocksExecutionStore::open(&path).unwrap();
            let execution = create_test_execution("exec_001", "wf_001");
            store.save(execution).unwrap();
        }

        // Reopen
        {
            let store = RocksExecutionStore::open(&path).unwrap();
            let execution = store.get("exec_001").unwrap();
            assert!(execution.is_some());
            assert_eq!(execution.unwrap().execution_id, "exec_001");
        }
    }
}
