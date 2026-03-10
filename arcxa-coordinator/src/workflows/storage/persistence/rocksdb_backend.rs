//! RocksDB Backend Implementation
//!
//! Production-grade persistent storage backend using RocksDB.
//! Provides durability, event sourcing, and high performance.

use super::error::{PersistenceError, Result};
use super::rocksdb_config::RocksDbConfig;
use super::traits::{Checkpoint, ExecutionStoreBackend};
use crate::workflows::domain::{
    ExecutionFilters, ExecutionLog, ExecutionStatus, WorkflowExecution,
};
use crate::workflows::storage::rocks_store::RocksExecutionStore;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error};

/// RocksDB backend for ExecutionStore
///
/// Wraps the existing RocksExecutionStore with an async interface.
/// Uses tokio::task::spawn_blocking to handle synchronous RocksDB operations.
pub struct RocksDbBackend {
    inner: Arc<RocksExecutionStore>,
}

impl RocksDbBackend {
    /// Create a new RocksDB backend at the given path with default configuration
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let inner = RocksExecutionStore::open(path)
            .map_err(|e| PersistenceError::storage_unavailable("RocksDB", e))?;

        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Create a new RocksDB backend with custom configuration
    pub fn open_with_config<P: AsRef<Path>>(path: P, config: RocksDbConfig) -> Result<Self> {
        let inner = RocksExecutionStore::open_with_config(path, config)
            .map_err(|e| PersistenceError::storage_unavailable("RocksDB", e))?;

        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Get the underlying RocksExecutionStore for advanced operations
    pub fn inner(&self) -> &Arc<RocksExecutionStore> {
        &self.inner
    }

    /// Get execution by ID (required, returns error if not found)
    ///
    /// This is a helper method, not part of the ExecutionStoreBackend trait
    pub async fn get_required(&self, id: &str) -> Result<WorkflowExecution> {
        self.get(id)
            .await?
            .ok_or_else(|| PersistenceError::execution_not_found(id))
    }

    /// Check if execution exists
    ///
    /// This is a helper method, not part of the ExecutionStoreBackend trait
    pub async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.get(id).await?.is_some())
    }

    /// Append log to execution (alias for add_log)
    ///
    /// This is a helper method, not part of the ExecutionStoreBackend trait
    pub async fn append_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        self.add_log(execution_id, log).await
    }
}

#[async_trait]
impl ExecutionStoreBackend for RocksDbBackend {
    async fn save(&self, execution: WorkflowExecution) -> Result<()> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            inner.save(execution).map_err(|e| {
                error!("Failed to save execution: {}", e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn get(&self, id: &str) -> Result<Option<WorkflowExecution>> {
        let inner = Arc::clone(&self.inner);
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            inner.get(&id).map_err(|e| {
                error!("Failed to get execution {}: {}", id, e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn update(&self, execution: WorkflowExecution) -> Result<()> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            inner.update(execution).map_err(|e| {
                error!("Failed to update execution: {}", e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            inner.delete(&id).map_err(|e| {
                error!("Failed to delete execution {}: {}", id, e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let inner = Arc::clone(&self.inner);
        let workflow_id = workflow_id.to_string();

        tokio::task::spawn_blocking(move || {
            let mut executions = inner.list_by_workflow(&workflow_id).map_err(|e| {
                error!(
                    "Failed to list executions for workflow {}: {}",
                    workflow_id, e
                );
                PersistenceError::internal("RocksDB", e.to_string())
            })?;

            // Apply pagination
            let total = executions.len();
            if offset >= total {
                return Ok(Vec::new());
            }

            let end = std::cmp::min(offset + limit, total);
            executions = executions[offset..end].to_vec();

            debug!(
                "Listed {} executions for workflow {} (offset: {}, limit: {})",
                executions.len(),
                workflow_id,
                offset,
                limit
            );

            Ok(executions)
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn list_by_status(
        &self,
        status: ExecutionStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            let filters = ExecutionFilters {
                status: Some(status.clone()),
                ..Default::default()
            };

            let mut executions = inner
                .list_filtered(&filters, Some(limit), Some(offset))
                .map_err(|e| {
                    error!("Failed to list executions by status {:?}: {}", status, e);
                    PersistenceError::internal("RocksDB", e.to_string())
                })?;

            // Apply pagination
            let total = executions.len();
            if offset >= total {
                return Ok(Vec::new());
            }

            let end = std::cmp::min(offset + limit, total);
            executions = executions[offset..end].to_vec();

            debug!(
                "Listed {} executions with status {:?} (offset: {}, limit: {})",
                executions.len(),
                status,
                offset,
                limit
            );

            Ok(executions)
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn list_by_time_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            let filters = ExecutionFilters {
                start_date: Some(start),
                end_date: Some(end),
                ..Default::default()
            };

            let mut executions = inner
                .list_filtered(&filters, Some(limit), Some(offset))
                .map_err(|e| {
                    error!("Failed to list executions by time range: {}", e);
                    PersistenceError::internal("RocksDB", e.to_string())
                })?;

            // Apply pagination
            let total = executions.len();
            if offset >= total {
                return Ok(Vec::new());
            }

            let end_idx = std::cmp::min(offset + limit, total);
            executions = executions[offset..end_idx].to_vec();

            debug!(
                "Listed {} executions in time range (offset: {}, limit: {})",
                executions.len(),
                offset,
                limit
            );

            Ok(executions)
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let execution_id = execution_id.to_string();

        tokio::task::spawn_blocking(move || {
            inner.add_log(&execution_id, log).map_err(|e| {
                error!("Failed to add log to execution {}: {}", execution_id, e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>> {
        let inner = Arc::clone(&self.inner);
        let execution_id = execution_id.to_string();

        tokio::task::spawn_blocking(move || {
            inner.get_logs(&execution_id).map_err(|e| {
                error!("Failed to get logs for execution {}: {}", execution_id, e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn count_total(&self) -> Result<usize> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            inner.count().map_err(|e| {
                error!("Failed to count total executions: {}", e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn count_by_status(&self, status: ExecutionStatus) -> Result<usize> {
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            let filters = ExecutionFilters {
                status: Some(status),
                ..Default::default()
            };

            inner.count_filtered(&filters).map_err(|e| {
                error!("Failed to count executions by status: {}", e);
                PersistenceError::internal("RocksDB", e.to_string())
            })
        })
        .await
        .map_err(|e| PersistenceError::internal("tokio", e))?
    }

    async fn health_check(&self) -> bool {
        // Simple health check: try to count executions
        self.count_total().await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_backend() -> (RocksDbBackend, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let backend = RocksDbBackend::open(temp_dir.path()).unwrap();
        (backend, temp_dir)
    }

    fn create_test_execution(id: &str) -> WorkflowExecution {
        WorkflowExecution::new(
            id.to_string(),
            "test_workflow".to_string(),
            "Test Workflow".to_string(),
            serde_json::json!({"test": "data"}),
            Some("test_user".to_string()),
        )
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let (backend, _temp_dir) = create_test_backend();
        let execution = create_test_execution("test_exec_1");

        // Save execution
        backend.save(execution.clone()).await.unwrap();

        // Get execution
        let retrieved = backend.get("test_exec_1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().execution_id, "test_exec_1");
    }

    #[tokio::test]
    async fn test_get_required_not_found() {
        let (backend, _temp_dir) = create_test_backend();

        // Try to get non-existent execution
        let result = backend.get_required("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PersistenceError::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_update() {
        let (backend, _temp_dir) = create_test_backend();
        let mut execution = create_test_execution("test_exec_2");

        // Save initial execution
        backend.save(execution.clone()).await.unwrap();

        // Update execution
        execution.update_status(ExecutionStatus::Completed);
        backend.update(execution.clone()).await.unwrap();

        // Verify update
        let retrieved = backend.get_required("test_exec_2").await.unwrap();
        assert_eq!(retrieved.status, ExecutionStatus::Completed);
    }

    #[tokio::test]
    async fn test_delete() {
        let (backend, _temp_dir) = create_test_backend();
        let execution = create_test_execution("test_exec_3");

        // Save and delete
        backend.save(execution).await.unwrap();
        assert!(backend.exists("test_exec_3").await.unwrap());

        backend.delete("test_exec_3").await.unwrap();
        assert!(!backend.exists("test_exec_3").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let (backend, _temp_dir) = create_test_backend();

        // Create multiple executions for same workflow
        for i in 0..5 {
            let exec = create_test_execution(&format!("exec_{}", i));
            backend.save(exec).await.unwrap();
        }

        // List executions
        let executions = backend
            .list_by_workflow("test_workflow", 10, 0)
            .await
            .unwrap();
        assert_eq!(executions.len(), 5);

        // Test pagination
        let page1 = backend
            .list_by_workflow("test_workflow", 2, 0)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = backend
            .list_by_workflow("test_workflow", 2, 2)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let (backend, _temp_dir) = create_test_backend();

        // Create executions with different statuses
        for i in 0..3 {
            let mut exec = create_test_execution(&format!("exec_pending_{}", i));
            backend.save(exec.clone()).await.unwrap();
        }

        for i in 0..2 {
            let mut exec = create_test_execution(&format!("exec_completed_{}", i));
            backend.save(exec.clone()).await.unwrap();
            exec.update_status(ExecutionStatus::Completed);
            backend.update(exec).await.unwrap();
        }

        // List by status
        let pending = backend
            .list_by_status(ExecutionStatus::Pending, 10, 0)
            .await
            .unwrap();
        assert_eq!(pending.len(), 3);

        let completed = backend
            .list_by_status(ExecutionStatus::Completed, 10, 0)
            .await
            .unwrap();
        assert_eq!(completed.len(), 2);
    }

    #[tokio::test]
    async fn test_count() {
        let (backend, _temp_dir) = create_test_backend();

        // Initially empty
        assert_eq!(backend.count_total().await.unwrap(), 0);

        // Add executions
        for i in 0..7 {
            let exec = create_test_execution(&format!("exec_{}", i));
            backend.save(exec).await.unwrap();
        }

        assert_eq!(backend.count_total().await.unwrap(), 7);
    }

    #[tokio::test]
    async fn test_append_and_get_logs() {
        let (backend, _temp_dir) = create_test_backend();
        let execution = create_test_execution("test_exec_logs");

        backend.save(execution).await.unwrap();

        // Append logs
        let log1 = ExecutionLog::info("First log entry");
        let log2 = ExecutionLog::error("Error log entry");

        backend
            .append_log("test_exec_logs", log1.clone())
            .await
            .unwrap();
        backend
            .append_log("test_exec_logs", log2.clone())
            .await
            .unwrap();

        // Get logs
        let logs = backend.get_logs("test_exec_logs").await.unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "First log entry");
        assert_eq!(logs[1].message, "Error log entry");
    }
}
