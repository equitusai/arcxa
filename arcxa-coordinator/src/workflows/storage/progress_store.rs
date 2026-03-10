//! Progress tracking storage for workflow executions
//!
//! Provides persistent storage of workflow execution progress using RocksDB.
//! This allows progress to survive coordinator restarts and enables querying
//! of historical execution data.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::orchestration::workflow::{ExecutionStatus, WorkflowProgress};
use rocksdb::{IteratorMode, WriteBatch, DB};
use std::sync::Arc;

/// Column family for progress tracking
const CF_PROGRESS: &str = "progress";

/// Storage for workflow execution progress
pub struct ProgressStore {
    db: Arc<DB>,
}

impl ProgressStore {
    /// Create a new progress store
    pub fn new(db: Arc<DB>) -> Result<Self> {
        // Ensure column family exists
        if db.cf_handle(CF_PROGRESS).is_none() {
            anyhow::bail!("Column family '{}' not found in RocksDB", CF_PROGRESS);
        }

        Ok(Self { db })
    }

    /// Store workflow progress snapshot
    pub fn store_progress(&self, progress: &WorkflowProgress) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let key = Self::execution_key(&progress.execution_id);
        let value = serde_json::to_vec(progress).context("Failed to serialize progress")?;

        self.db
            .put_cf(cf, key, value)
            .context("Failed to store progress")?;

        tracing::debug!(
            "Stored progress for execution {} (status: {:?}, {:.1}% complete)",
            progress.execution_id,
            progress.status,
            progress.percent_complete.unwrap_or(0.0)
        );

        Ok(())
    }

    /// Store multiple progress snapshots in a batch
    pub fn store_progress_batch(&self, progress_list: Vec<&WorkflowProgress>) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let mut batch = WriteBatch::default();

        for progress in progress_list {
            let key = Self::execution_key(&progress.execution_id);
            let value = serde_json::to_vec(progress).context("Failed to serialize progress")?;
            batch.put_cf(cf, key, value);
        }

        self.db
            .write(batch)
            .context("Failed to write progress batch")?;

        Ok(())
    }

    /// Get workflow progress by execution ID
    pub fn get_progress(&self, execution_id: &str) -> Result<Option<WorkflowProgress>> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let key = Self::execution_key(execution_id);

        match self.db.get_cf(cf, key)? {
            Some(bytes) => {
                let progress: WorkflowProgress =
                    serde_json::from_slice(&bytes).context("Failed to deserialize progress")?;
                Ok(Some(progress))
            }
            None => Ok(None),
        }
    }

    /// Get all progress records for a workflow
    pub fn get_workflow_executions(&self, workflow_id: &str) -> Result<Vec<WorkflowProgress>> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let mut executions = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            if let Ok(progress) = serde_json::from_slice::<WorkflowProgress>(&value) {
                if progress.workflow_id == workflow_id {
                    executions.push(progress);
                }
            }
        }

        // Sort by started_at descending (most recent first)
        executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(executions)
    }

    /// Get all active (running or queued) executions
    pub fn get_active_executions(&self) -> Result<Vec<WorkflowProgress>> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let mut active = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            if let Ok(progress) = serde_json::from_slice::<WorkflowProgress>(&value) {
                match progress.status {
                    ExecutionStatus::Queued | ExecutionStatus::Running => {
                        active.push(progress);
                    }
                    _ => {}
                }
            }
        }

        // Sort by started_at ascending (oldest first)
        active.sort_by(|a, b| a.started_at.cmp(&b.started_at));

        Ok(active)
    }

    /// Get recent executions (last N, regardless of status)
    pub fn get_recent_executions(&self, limit: usize) -> Result<Vec<WorkflowProgress>> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let mut executions = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            if let Ok(progress) = serde_json::from_slice::<WorkflowProgress>(&value) {
                executions.push(progress);
            }
        }

        // Sort by last_updated descending (most recent first)
        executions.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));

        // Take first N
        executions.truncate(limit);

        Ok(executions)
    }

    /// Delete progress for a specific execution
    pub fn delete_progress(&self, execution_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let key = Self::execution_key(execution_id);
        self.db
            .delete_cf(cf, key)
            .context("Failed to delete progress")?;

        tracing::debug!("Deleted progress for execution {}", execution_id);

        Ok(())
    }

    /// Clean up old completed/failed executions (keep recent N days)
    pub fn cleanup_old_executions(&self, keep_days: i64) -> Result<usize> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let cutoff = Utc::now() - chrono::Duration::days(keep_days);
        let mut to_delete = Vec::new();

        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            if let Ok(progress) = serde_json::from_slice::<WorkflowProgress>(&value) {
                // Only delete completed/failed/cancelled executions older than cutoff
                match progress.status {
                    ExecutionStatus::Completed
                    | ExecutionStatus::Failed
                    | ExecutionStatus::Cancelled => {
                        if let Some(completed_at) = progress.completed_at {
                            if completed_at < cutoff {
                                to_delete.push(key.to_vec());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let count = to_delete.len();

        if count > 0 {
            let mut batch = WriteBatch::default();
            for key in to_delete {
                batch.delete_cf(cf, key);
            }
            self.db
                .write(batch)
                .context("Failed to delete old executions")?;

            tracing::info!(
                "Cleaned up {} old executions (older than {} days)",
                count,
                keep_days
            );
        }

        Ok(count)
    }

    /// Get statistics about stored executions
    pub fn get_statistics(&self) -> Result<ProgressStatistics> {
        let cf = self
            .db
            .cf_handle(CF_PROGRESS)
            .context("Progress column family not found")?;

        let mut stats = ProgressStatistics::default();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            if let Ok(progress) = serde_json::from_slice::<WorkflowProgress>(&value) {
                stats.total_executions += 1;
                match progress.status {
                    ExecutionStatus::Queued => stats.queued += 1,
                    ExecutionStatus::Running => stats.running += 1,
                    ExecutionStatus::Completed => stats.completed += 1,
                    ExecutionStatus::Failed => stats.failed += 1,
                    ExecutionStatus::Cancelled => stats.cancelled += 1,
                }
            }
        }

        Ok(stats)
    }

    /// Generate key for execution progress
    fn execution_key(execution_id: &str) -> Vec<u8> {
        format!("exec:{}", execution_id).into_bytes()
    }
}

/// Statistics about workflow execution progress
#[derive(Debug, Default, Clone)]
pub struct ProgressStatistics {
    pub total_executions: usize,
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::orchestration::workflow::StepProgress;
    use rocksdb::Options;
    use tempfile::TempDir;

    fn create_test_db() -> (TempDir, Arc<DB>) {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(&opts, temp_dir.path(), vec![CF_PROGRESS]).unwrap();
        (temp_dir, Arc::new(db))
    }

    fn create_test_progress(
        execution_id: &str,
        workflow_id: &str,
        status: ExecutionStatus,
    ) -> WorkflowProgress {
        WorkflowProgress {
            execution_id: execution_id.to_string(),
            workflow_id: workflow_id.to_string(),
            status,
            current_step: None,
            total_steps: 5,
            steps_completed: 2,
            rows_processed: 1000,
            total_rows: Some(5000),
            percent_complete: Some(40.0),
            started_at: Utc::now(),
            completed_at: None,
            last_updated: Utc::now(),
            error: None,
            eta_seconds: Some(60),
        }
    }

    #[test]
    fn test_store_and_retrieve_progress() {
        let (_temp, db) = create_test_db();
        let store = ProgressStore::new(db).unwrap();

        let progress = create_test_progress("exec-1", "workflow-1", ExecutionStatus::Running);

        store.store_progress(&progress).unwrap();

        let retrieved = store.get_progress("exec-1").unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.execution_id, "exec-1");
        assert_eq!(retrieved.workflow_id, "workflow-1");
        assert_eq!(retrieved.status, ExecutionStatus::Running);
    }

    #[test]
    fn test_get_workflow_executions() {
        let (_temp, db) = create_test_db();
        let store = ProgressStore::new(db).unwrap();

        // Create multiple executions for same workflow
        store
            .store_progress(&create_test_progress(
                "exec-1",
                "workflow-1",
                ExecutionStatus::Running,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-2",
                "workflow-1",
                ExecutionStatus::Completed,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-3",
                "workflow-2",
                ExecutionStatus::Running,
            ))
            .unwrap();

        let executions = store.get_workflow_executions("workflow-1").unwrap();
        assert_eq!(executions.len(), 2);
        assert!(executions.iter().all(|e| e.workflow_id == "workflow-1"));
    }

    #[test]
    fn test_get_active_executions() {
        let (_temp, db) = create_test_db();
        let store = ProgressStore::new(db).unwrap();

        store
            .store_progress(&create_test_progress(
                "exec-1",
                "workflow-1",
                ExecutionStatus::Running,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-2",
                "workflow-1",
                ExecutionStatus::Queued,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-3",
                "workflow-1",
                ExecutionStatus::Completed,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-4",
                "workflow-1",
                ExecutionStatus::Failed,
            ))
            .unwrap();

        let active = store.get_active_executions().unwrap();
        assert_eq!(active.len(), 2);
        assert!(active
            .iter()
            .all(|e| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Queued)));
    }

    #[test]
    fn test_delete_progress() {
        let (_temp, db) = create_test_db();
        let store = ProgressStore::new(db).unwrap();

        let progress = create_test_progress("exec-1", "workflow-1", ExecutionStatus::Running);
        store.store_progress(&progress).unwrap();

        assert!(store.get_progress("exec-1").unwrap().is_some());

        store.delete_progress("exec-1").unwrap();

        assert!(store.get_progress("exec-1").unwrap().is_none());
    }

    #[test]
    fn test_statistics() {
        let (_temp, db) = create_test_db();
        let store = ProgressStore::new(db).unwrap();

        store
            .store_progress(&create_test_progress(
                "exec-1",
                "workflow-1",
                ExecutionStatus::Running,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-2",
                "workflow-1",
                ExecutionStatus::Running,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-3",
                "workflow-1",
                ExecutionStatus::Completed,
            ))
            .unwrap();
        store
            .store_progress(&create_test_progress(
                "exec-4",
                "workflow-1",
                ExecutionStatus::Failed,
            ))
            .unwrap();

        let stats = store.get_statistics().unwrap();
        assert_eq!(stats.total_executions, 4);
        assert_eq!(stats.running, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
    }
}
