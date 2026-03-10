//! Execution Storage - CRUD operations for workflow executions
//!
//! Provides a high-level API for workflow execution storage with support
//! for pluggable backends via the ExecutionStoreBackend trait.

use crate::workflows::domain::{
    ExecutionFilters, ExecutionLog, ExecutionStatus, WorkflowExecution,
};
use crate::workflows::storage::persistence::{
    ExecutionStoreBackend, InMemoryBackend, PersistenceError,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Execution storage with pluggable backend support
///
/// Provides a high-level API for workflow execution operations.
/// Uses ExecutionStoreBackend trait for storage, allowing different
/// implementations (in-memory, RocksDB, etc.).
#[derive(Clone)]
pub struct ExecutionStore {
    backend: Arc<dyn ExecutionStoreBackend>,
}

impl Default for ExecutionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionStore {
    /// Create a new execution store with in-memory backend
    pub fn new() -> Self {
        Self::with_backend(Arc::new(InMemoryBackend::new()))
    }

    /// Create an execution store with a custom backend
    pub fn with_backend(backend: Arc<dyn ExecutionStoreBackend>) -> Self {
        Self { backend }
    }

    /// Save a new execution
    ///
    /// ## Errors
    /// - If execution ID already exists
    pub async fn save(&self, execution: WorkflowExecution) -> Result<()> {
        self.backend
            .save(execution)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Get an execution by ID
    pub async fn get(&self, execution_id: &str) -> Result<Option<WorkflowExecution>> {
        self.backend
            .get(execution_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Get an execution by ID (required, returns error if not found)
    pub async fn get_required(&self, execution_id: &str) -> Result<WorkflowExecution> {
        self.get(execution_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Execution '{}' not found", execution_id))
    }

    /// Update an existing execution
    ///
    /// ## Errors
    /// - If execution doesn't exist
    pub async fn update(&self, execution: WorkflowExecution) -> Result<()> {
        self.backend
            .update(execution)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Update execution status
    ///
    /// Convenience method to update just the status without fetching entire execution.
    pub async fn update_status(&self, execution_id: &str, status: ExecutionStatus) -> Result<()> {
        let mut execution = self.get_required(execution_id).await?;
        execution.update_status(status);
        self.update(execution).await
    }

    /// Add a log entry to an execution
    ///
    /// Convenience method to append logs without fetching entire execution.
    pub async fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()> {
        self.backend
            .add_log(execution_id, log)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Set execution output
    pub async fn set_output(&self, execution_id: &str, output: serde_json::Value) -> Result<()> {
        let mut execution = self.get_required(execution_id).await?;
        execution.set_output(output);
        self.update(execution).await
    }

    /// Set execution error
    pub async fn set_error(&self, execution_id: &str, error: String) -> Result<()> {
        let mut execution = self.get_required(execution_id).await?;
        execution.set_error(error);
        self.update(execution).await
    }

    /// List all executions for a workflow
    pub async fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowExecution>> {
        self.backend
            .list_by_workflow(workflow_id, usize::MAX, 0)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// List executions with filters
    pub async fn list_filtered(
        &self,
        filters: &ExecutionFilters,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<WorkflowExecution>> {
        // Use backend filters when possible, otherwise fetch all and filter in memory
        let mut executions = if let Some(status) = filters.status {
            self.backend
                .list_by_status(status, usize::MAX, 0)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?
        } else if let (Some(start), Some(end)) = (filters.start_date, filters.end_date) {
            self.backend
                .list_by_time_range(start, end, usize::MAX, 0)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?
        } else {
            // No efficient backend filter available
            // For in-memory backend, fetch using a very wide time range
            // This covers cases where only start_date or end_date is set, or search is used
            use chrono::{DateTime, TimeZone};
            let min_time = chrono::Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
            let max_time = chrono::Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap();

            self.backend
                .list_by_time_range(min_time, max_time, usize::MAX, 0)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?
        };

        // Apply additional in-memory filters
        executions.retain(|e| filters.matches(e));

        // Sort by started_at descending (most recent first)
        executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Apply pagination
        let offset_val = offset.unwrap_or(0);
        let result: Vec<_> = executions.into_iter().skip(offset_val).collect();

        if let Some(limit_val) = limit {
            Ok(result.into_iter().take(limit_val).collect())
        } else {
            Ok(result)
        }
    }

    /// Get execution logs
    pub async fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>> {
        self.backend
            .get_logs(execution_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Count total executions
    pub async fn count(&self) -> Result<usize> {
        self.backend
            .count_total()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Count executions matching filters
    pub async fn count_filtered(&self, filters: &ExecutionFilters) -> Result<usize> {
        if let Some(status) = filters.status {
            self.backend
                .count_by_status(status)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        } else {
            // Need to fetch and filter manually for complex filters
            let executions = self.list_filtered(filters, None, None).await?;
            Ok(executions.len())
        }
    }

    /// Check if an execution exists
    pub async fn exists(&self, execution_id: &str) -> Result<bool> {
        Ok(self.get(execution_id).await?.is_some())
    }

    /// Delete an execution (for cleanup/testing)
    pub async fn delete(&self, execution_id: &str) -> Result<()> {
        self.backend
            .delete(execution_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::LogLevel;
    use chrono::Duration;
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
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        assert!(store.save(execution).await.is_ok());
    }

    #[tokio::test]
    async fn test_save_duplicate_execution() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution.clone()).await.unwrap();

        // Second save should fail
        let result = store.save(execution).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_execution() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        let retrieved = store.get("exec_001").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_get_nonexistent_execution() {
        let store = ExecutionStore::new();

        let retrieved = store.get("exec_999").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_get_required_execution() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        let retrieved = store.get_required("exec_001").await.unwrap();
        assert_eq!(retrieved.execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_get_required_nonexistent() {
        let store = ExecutionStore::new();

        let result = store.get_required("exec_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_execution() {
        let store = ExecutionStore::new();
        let mut execution = create_test_execution("exec_001", "wf_001");

        store.save(execution.clone()).await.unwrap();

        // Modify execution
        execution.update_status(ExecutionStatus::Running);

        store.update(execution).await.unwrap();

        // Verify update
        let updated = store.get("exec_001").await.unwrap().unwrap();
        assert_eq!(updated.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_update_nonexistent_execution() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        let result = store.update(execution).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        store
            .update_status("exec_001", ExecutionStatus::Running)
            .await
            .unwrap();

        let updated = store.get("exec_001").await.unwrap().unwrap();
        assert_eq!(updated.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_add_log() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        let log = ExecutionLog::info("Test log message");
        store.add_log("exec_001", log).await.unwrap();

        let logs = store.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Test log message");
        assert_eq!(logs[0].level, LogLevel::Info);
    }

    #[tokio::test]
    async fn test_set_output() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        let output = json!({"result": "success"});
        store.set_output("exec_001", output.clone()).await.unwrap();

        let updated = store.get("exec_001").await.unwrap().unwrap();
        assert_eq!(updated.output, Some(output));
    }

    #[tokio::test]
    async fn test_set_error() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        store
            .set_error("exec_001", "Test error".to_string())
            .await
            .unwrap();

        let updated = store.get("exec_001").await.unwrap().unwrap();
        assert_eq!(updated.error, Some("Test error".to_string()));
        assert_eq!(updated.status, ExecutionStatus::Failed);
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let store = ExecutionStore::new();

        store
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        store
            .save(create_test_execution("exec_002", "wf_001"))
            .await
            .unwrap();
        store
            .save(create_test_execution("exec_003", "wf_002"))
            .await
            .unwrap();

        let executions = store.list_by_workflow("wf_001").await.unwrap();
        assert_eq!(executions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_filtered_by_status() {
        let store = ExecutionStore::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        store.save(exec1).await.unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Completed);
        store.save(exec2).await.unwrap();

        let filters = ExecutionFilters {
            status: Some(ExecutionStatus::Running),
            ..Default::default()
        };

        let executions = store.list_filtered(&filters, None, None).await.unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_list_filtered_by_date() {
        let store = ExecutionStore::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.started_at = Utc::now() - Duration::days(2);
        store.save(exec1).await.unwrap();

        let exec2 = create_test_execution("exec_002", "wf_001");
        store.save(exec2).await.unwrap();

        let filters = ExecutionFilters {
            start_date: Some(Utc::now() - Duration::days(1)),
            ..Default::default()
        };

        let executions = store.list_filtered(&filters, None, None).await.unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].execution_id, "exec_002");
    }

    #[tokio::test]
    async fn test_list_filtered_with_search() {
        let store = ExecutionStore::new();

        store
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        store
            .save(create_test_execution("exec_002", "wf_002"))
            .await
            .unwrap();

        let filters = ExecutionFilters {
            search: Some("exec_001".to_string()),
            ..Default::default()
        };

        let executions = store.list_filtered(&filters, None, None).await.unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_list_filtered_with_pagination() {
        let store = ExecutionStore::new();

        for i in 1..=5 {
            let exec = create_test_execution(&format!("exec_{:03}", i), "wf_001");
            store.save(exec).await.unwrap();
        }

        let filters = ExecutionFilters::default();

        // Get first 2
        let page1 = store
            .list_filtered(&filters, Some(2), Some(0))
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        // Get next 2
        let page2 = store
            .list_filtered(&filters, Some(2), Some(2))
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        // Verify no overlap
        assert_ne!(page1[0].execution_id, page2[0].execution_id);
    }

    #[tokio::test]
    async fn test_count() {
        let store = ExecutionStore::new();

        assert_eq!(store.count().await.unwrap(), 0);

        store
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store
            .save(create_test_execution("exec_002", "wf_001"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        store.delete("exec_001").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_count_filtered() {
        let store = ExecutionStore::new();

        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        store.save(exec1).await.unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Completed);
        store.save(exec2).await.unwrap();

        let filters = ExecutionFilters {
            status: Some(ExecutionStatus::Running),
            ..Default::default()
        };

        assert_eq!(store.count_filtered(&filters).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = ExecutionStore::new();

        assert!(!store.exists("exec_001").await.unwrap());

        store
            .save(create_test_execution("exec_001", "wf_001"))
            .await
            .unwrap();
        assert!(store.exists("exec_001").await.unwrap());

        store.delete("exec_001").await.unwrap();
        assert!(!store.exists("exec_001").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();
        assert!(store.exists("exec_001").await.unwrap());

        store.delete("exec_001").await.unwrap();
        assert!(!store.exists("exec_001").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let store = ExecutionStore::new();

        let result = store.delete("exec_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let store = ExecutionStore::new();

        // Create executions concurrently using tokio tasks
        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = store.clone();
            let handle = tokio::spawn(async move {
                let execution = create_test_execution(&format!("exec_{:03}", i), "wf_001");
                store_clone.save(execution).await
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // Verify all executions created
        assert_eq!(store.count().await.unwrap(), 10);
    }

    #[tokio::test]
    async fn test_get_logs() {
        let store = ExecutionStore::new();
        let execution = create_test_execution("exec_001", "wf_001");

        store.save(execution).await.unwrap();

        store
            .add_log("exec_001", ExecutionLog::info("Log 1"))
            .await
            .unwrap();
        store
            .add_log("exec_001", ExecutionLog::warn("Log 2"))
            .await
            .unwrap();
        store
            .add_log("exec_001", ExecutionLog::error("Log 3"))
            .await
            .unwrap();

        let logs = store.get_logs("exec_001").await.unwrap();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].message, "Log 1");
        assert_eq!(logs[1].message, "Log 2");
        assert_eq!(logs[2].message, "Log 3");
    }
}
