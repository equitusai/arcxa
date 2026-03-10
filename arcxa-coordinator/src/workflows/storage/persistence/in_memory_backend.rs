//! In-Memory Storage Backend
//!
//! Thread-safe in-memory implementation of ExecutionStoreBackend.
//! Suitable for development, testing, and lightweight deployments where
//! persistence is not required.
//!
//! # Characteristics
//!
//! - Fast: No I/O, all operations in memory
//! - Thread-safe: Uses RwLock for concurrent access
//! - Ephemeral: Data lost on process restart
//! - No quota limits: Limited only by available RAM
//!
//! # Usage
//!
//! ```ignore
//! use graphica_coordinator::workflows::storage::persistence::InMemoryBackend;
//!
//! let backend = InMemoryBackend::new();
//! backend.save(execution).await?;
//! ```

use super::error::{PersistenceError, Result};
use super::traits::ExecutionStoreBackend;
use crate::workflows::domain::{ExecutionLog, ExecutionStatus, WorkflowExecution};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory execution storage backend
///
/// Stores all data in a HashMap protected by RwLock. All operations
/// are lock-based, making them fast but not durable.
#[derive(Clone)]
pub struct InMemoryBackend {
    /// Main execution storage
    executions: Arc<RwLock<HashMap<String, WorkflowExecution>>>,

    /// Execution logs stored separately for efficient append
    logs: Arc<RwLock<HashMap<String, Vec<ExecutionLog>>>>,
}

impl InMemoryBackend {
    /// Create a new empty in-memory backend
    pub fn new() -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
            logs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get lock error as PersistenceError
    fn lock_error(operation: &str) -> PersistenceError {
        PersistenceError::InternalError {
            backend: "InMemory".to_string(),
            details: format!("Lock poisoned during {}", operation),
        }
    }

    /// Merge logs into an execution
    fn merge_logs(&self, mut execution: WorkflowExecution) -> Result<WorkflowExecution> {
        let logs_map = self
            .logs
            .read()
            .map_err(|_| Self::lock_error("merge_logs"))?;

        if let Some(logs) = logs_map.get(&execution.execution_id) {
            execution.logs = logs.clone();
        }

        Ok(execution)
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionStoreBackend for InMemoryBackend {
    async fn save(&self, execution: WorkflowExecution) -> Result<()> {
        let mut executions = self
            .executions
            .write()
            .map_err(|_| Self::lock_error("save"))?;

        if executions.contains_key(&execution.execution_id) {
            return Err(PersistenceError::AlreadyExists {
                entity_type: "WorkflowExecution".to_string(),
                entity_id: execution.execution_id.clone(),
            });
        }

        executions.insert(execution.execution_id.clone(), execution);
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<WorkflowExecution>> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("get"))?;

        let execution = match executions.get(id).cloned() {
            Some(exec) => exec,
            None => return Ok(None),
        };

        Ok(Some(self.merge_logs(execution)?))
    }

    async fn update(&self, execution: WorkflowExecution) -> Result<()> {
        let mut executions = self
            .executions
            .write()
            .map_err(|_| Self::lock_error("update"))?;

        if !executions.contains_key(&execution.execution_id) {
            return Err(PersistenceError::NotFound {
                entity_type: "WorkflowExecution".to_string(),
                entity_id: execution.execution_id.clone(),
            });
        }

        executions.insert(execution.execution_id.clone(), execution);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut executions = self
            .executions
            .write()
            .map_err(|_| Self::lock_error("delete"))?;

        if executions.remove(id).is_none() {
            return Err(PersistenceError::NotFound {
                entity_type: "WorkflowExecution".to_string(),
                entity_id: id.to_string(),
            });
        }

        // Also remove logs
        let mut logs = self
            .logs
            .write()
            .map_err(|_| Self::lock_error("delete_logs"))?;
        logs.remove(id);

        Ok(())
    }

    async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("list_by_workflow"))?;

        let mut filtered: Vec<WorkflowExecution> = executions
            .values()
            .filter(|e| e.workflow_id == workflow_id)
            .cloned()
            .collect();

        // Sort by started_at descending (newest first)
        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Apply pagination
        let filtered: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        // Merge logs for each execution
        filtered.into_iter().map(|e| self.merge_logs(e)).collect()
    }

    async fn list_by_status(
        &self,
        status: ExecutionStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("list_by_status"))?;

        let mut filtered: Vec<WorkflowExecution> = executions
            .values()
            .filter(|e| e.status == status)
            .cloned()
            .collect();

        // Sort by started_at descending (newest first)
        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Apply pagination
        let filtered: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        // Merge logs for each execution
        filtered.into_iter().map(|e| self.merge_logs(e)).collect()
    }

    async fn list_by_time_range(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("list_by_time_range"))?;

        let mut filtered: Vec<WorkflowExecution> = executions
            .values()
            .filter(|e| e.started_at >= start_time && e.started_at <= end_time)
            .cloned()
            .collect();

        // Sort by started_at descending (newest first)
        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Apply pagination
        let filtered: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        // Merge logs for each execution
        filtered.into_iter().map(|e| self.merge_logs(e)).collect()
    }

    async fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        // Verify execution exists
        {
            let executions = self
                .executions
                .read()
                .map_err(|_| Self::lock_error("add_log_check"))?;

            if !executions.contains_key(execution_id) {
                return Err(PersistenceError::NotFound {
                    entity_type: "WorkflowExecution".to_string(),
                    entity_id: execution_id.to_string(),
                });
            }
        }

        // Add log
        let mut logs = self.logs.write().map_err(|_| Self::lock_error("add_log"))?;

        logs.entry(execution_id.to_string())
            .or_insert_with(Vec::new)
            .push(log);

        Ok(())
    }

    async fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>> {
        // Verify execution exists
        {
            let executions = self
                .executions
                .read()
                .map_err(|_| Self::lock_error("get_logs_check"))?;

            if !executions.contains_key(execution_id) {
                return Err(PersistenceError::NotFound {
                    entity_type: "WorkflowExecution".to_string(),
                    entity_id: execution_id.to_string(),
                });
            }
        }

        // Get logs
        let logs = self.logs.read().map_err(|_| Self::lock_error("get_logs"))?;

        Ok(logs.get(execution_id).cloned().unwrap_or_default())
    }

    async fn count_total(&self) -> Result<usize> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("count_total"))?;

        Ok(executions.len())
    }

    async fn count_by_status(&self, status: ExecutionStatus) -> Result<usize> {
        let executions = self
            .executions
            .read()
            .map_err(|_| Self::lock_error("count_by_status"))?;

        let count = executions.values().filter(|e| e.status == status).count();

        Ok(count)
    }

    async fn health_check(&self) -> bool {
        // Try to acquire read lock as health check
        self.executions.read().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::LogLevel;
    use serde_json::json;

    fn create_test_execution(id: &str, workflow_id: &str) -> WorkflowExecution {
        WorkflowExecution::new(
            id.to_string(),
            workflow_id.to_string(),
            format!("Workflow {}", workflow_id),
            json!({"test": "data"}),
            Some("test@example.com".to_string()),
        )
    }

    #[tokio::test]
    async fn test_save_execution() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        let result = backend.save(execution).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_save_duplicate_execution() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution.clone()).await.unwrap();

        let result = backend.save(execution).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            PersistenceError::AlreadyExists { entity_id, .. } => {
                assert_eq!(entity_id, "exec_001");
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_execution() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();

        let retrieved = backend.get("exec_001").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_get_nonexistent_execution() {
        let backend = InMemoryBackend::new();

        let retrieved = backend.get("exec_999").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_update_execution() {
        let backend = InMemoryBackend::new();
        let mut execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution.clone()).await.unwrap();

        execution.update_status(ExecutionStatus::Running);
        backend.update(execution).await.unwrap();

        let updated = backend.get("exec_001").await.unwrap().unwrap();
        assert_eq!(updated.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_update_nonexistent_execution() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        let result = backend.update(execution).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            PersistenceError::NotFound { entity_id, .. } => {
                assert_eq!(entity_id, "exec_001");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_execution() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();

        let exists = backend.get("exec_001").await.unwrap();
        assert!(exists.is_some());

        backend.delete("exec_001").await.unwrap();

        let exists = backend.get("exec_001").await.unwrap();
        assert!(exists.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let backend = InMemoryBackend::new();

        let result = backend.delete("exec_999").await;
        assert!(result.is_err());

        match result.unwrap_err() {
            PersistenceError::NotFound { .. } => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let backend = InMemoryBackend::new();

        backend
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        backend
            .save(create_test_execution("exec_002", "wf_001"))
            .await
            .unwrap();
        backend
            .save(create_test_execution("exec_003", "wf_002"))
            .await
            .unwrap();

        let executions = backend.list_by_workflow("wf_001", 100, 0).await.unwrap();
        assert_eq!(executions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_workflow_pagination() {
        let backend = InMemoryBackend::new();

        for i in 1..=5 {
            let exec = create_test_execution(&format!("exec_{:03}", i), "wf_001");
            backend.save(exec).await.unwrap();
        }

        // Get first 2
        let page1 = backend.list_by_workflow("wf_001", 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        // Get next 2
        let page2 = backend.list_by_workflow("wf_001", 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        // Verify no overlap
        assert_ne!(page1[0].execution_id, page2[0].execution_id);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let backend = InMemoryBackend::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        backend.save(exec1).await.unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Completed);
        backend.save(exec2).await.unwrap();

        let running = backend
            .list_by_status(ExecutionStatus::Running, 100, 0)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].execution_id, "exec_001");

        let completed = backend
            .list_by_status(ExecutionStatus::Completed, 100, 0)
            .await
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].execution_id, "exec_002");
    }

    #[tokio::test]
    async fn test_list_by_time_range() {
        let backend = InMemoryBackend::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.started_at = Utc::now() - chrono::Duration::hours(2);
        backend.save(exec1).await.unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.started_at = Utc::now();
        backend.save(exec2).await.unwrap();

        let start_time = Utc::now() - chrono::Duration::hours(1);
        let end_time = Utc::now() + chrono::Duration::hours(1);

        let executions = backend
            .list_by_time_range(start_time, end_time, 100, 0)
            .await
            .unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].execution_id, "exec_002");
    }

    #[tokio::test]
    async fn test_add_log() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();

        let log = ExecutionLog::info("Test log message");
        backend.add_log("exec_001", log).await.unwrap();

        let logs = backend.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Test log message");
        assert_eq!(logs[0].level, LogLevel::Info);
    }

    #[tokio::test]
    async fn test_add_multiple_logs() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();

        backend
            .add_log("exec_001", ExecutionLog::info("Log 1"))
            .await
            .unwrap();
        backend
            .add_log("exec_001", ExecutionLog::warn("Log 2"))
            .await
            .unwrap();
        backend
            .add_log("exec_001", ExecutionLog::error("Log 3"))
            .await
            .unwrap();

        let logs = backend.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].message, "Log 1");
        assert_eq!(logs[1].message, "Log 2");
        assert_eq!(logs[2].message, "Log 3");
    }

    #[tokio::test]
    async fn test_add_log_to_nonexistent() {
        let backend = InMemoryBackend::new();

        let log = ExecutionLog::info("Test log");
        let result = backend.add_log("exec_999", log).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PersistenceError::NotFound { .. } => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_get_logs_for_nonexistent() {
        let backend = InMemoryBackend::new();

        let result = backend.get_logs("exec_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_logs_empty() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();

        let logs = backend.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 0);
    }

    #[tokio::test]
    async fn test_count_total() {
        let backend = InMemoryBackend::new();

        assert_eq!(backend.count_total().await.unwrap(), 0);

        backend
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        assert_eq!(backend.count_total().await.unwrap(), 1);

        backend
            .save(create_test_execution("exec_002", "wf_001"))
            .await
            .unwrap();
        assert_eq!(backend.count_total().await.unwrap(), 2);

        backend.delete("exec_001").await.unwrap();
        assert_eq!(backend.count_total().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_count_by_status() {
        let backend = InMemoryBackend::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        backend.save(exec1).await.unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Running);
        backend.save(exec2).await.unwrap();

        let mut exec3 = create_test_execution("exec_003", "wf_001");
        exec3.update_status(ExecutionStatus::Completed);
        backend.save(exec3).await.unwrap();

        assert_eq!(
            backend
                .count_by_status(ExecutionStatus::Running)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            backend
                .count_by_status(ExecutionStatus::Completed)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            backend
                .count_by_status(ExecutionStatus::Failed)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_health_check() {
        let backend = InMemoryBackend::new();
        assert!(backend.health_check().await);
    }

    #[tokio::test]
    async fn test_delete_also_removes_logs() {
        let backend = InMemoryBackend::new();
        let execution = create_test_execution("exec_001", "wf_001");

        backend.save(execution).await.unwrap();
        backend
            .add_log("exec_001", ExecutionLog::info("Test log"))
            .await
            .unwrap();

        // Verify log exists
        let logs = backend.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 1);

        // Delete execution
        backend.delete("exec_001").await.unwrap();

        // Verify execution gone
        let exec = backend.get("exec_001").await.unwrap();
        assert!(exec.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let backend = InMemoryBackend::new();
        let backend_arc = Arc::new(backend);

        // Create executions concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let backend_clone = Arc::clone(&backend_arc);
            let handle = tokio::spawn(async move {
                let execution = create_test_execution(&format!("exec_{:03}", i), "wf_001");
                backend_clone.save(execution).await
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // Verify all executions created
        assert_eq!(backend_arc.count_total().await.unwrap(), 10);
    }
}
