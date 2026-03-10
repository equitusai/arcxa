//! In-Memory Approval Storage Backend
//!
//! Thread-safe in-memory implementation of ApprovalStoreBackend.
//! Suitable for development, testing, and lightweight deployments where
//! persistence of approval requests is not required.
//!
//! # Characteristics
//!
//! - Fast: No I/O, all operations in memory
//! - Thread-safe: Uses RwLock for concurrent access
//! - Ephemeral: Data lost on process restart
//! - Indexed queries: Fast lookups by execution_id, workflow_id, status
//!
//! # Usage
//!
//! ```ignore
//! use graphica_coordinator::workflows::storage::persistence::InMemoryApprovalBackend;
//!
//! let backend = InMemoryApprovalBackend::new();
//! backend.save(approval_request).await?;
//! ```

use super::error::{PersistenceError, Result};
use super::traits::ApprovalStoreBackend;
use crate::workflows::domain::{ApprovalRequest, ApprovalStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory approval request storage backend
///
/// Stores all approval requests in a HashMap protected by RwLock.
/// Maintains secondary indexes for efficient querying.
#[derive(Clone)]
pub struct InMemoryApprovalBackend {
    /// Main approval storage (request_id -> ApprovalRequest)
    approvals: Arc<RwLock<HashMap<String, ApprovalRequest>>>,
}

impl InMemoryApprovalBackend {
    /// Create a new empty in-memory approval backend
    pub fn new() -> Self {
        Self {
            approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get lock error as PersistenceError
    fn lock_error(operation: &str) -> PersistenceError {
        PersistenceError::InternalError {
            backend: "InMemoryApproval".to_string(),
            details: format!("Lock poisoned during {}", operation),
        }
    }
}

impl Default for InMemoryApprovalBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApprovalStoreBackend for InMemoryApprovalBackend {
    async fn save(&self, request: ApprovalRequest) -> Result<()> {
        let mut approvals = self
            .approvals
            .write()
            .map_err(|_| Self::lock_error("save"))?;

        if approvals.contains_key(&request.request_id) {
            return Err(PersistenceError::AlreadyExists {
                entity_type: "ApprovalRequest".to_string(),
                entity_id: request.request_id.clone(),
            });
        }

        approvals.insert(request.request_id.clone(), request);
        Ok(())
    }

    async fn get(&self, request_id: &str) -> Result<Option<ApprovalRequest>> {
        let approvals = self.approvals.read().map_err(|_| Self::lock_error("get"))?;

        Ok(approvals.get(request_id).cloned())
    }

    async fn update(&self, request: ApprovalRequest) -> Result<()> {
        let mut approvals = self
            .approvals
            .write()
            .map_err(|_| Self::lock_error("update"))?;

        if !approvals.contains_key(&request.request_id) {
            return Err(PersistenceError::NotFound {
                entity_type: "ApprovalRequest".to_string(),
                entity_id: request.request_id.clone(),
            });
        }

        approvals.insert(request.request_id.clone(), request);
        Ok(())
    }

    async fn delete(&self, request_id: &str) -> Result<()> {
        let mut approvals = self
            .approvals
            .write()
            .map_err(|_| Self::lock_error("delete"))?;

        if approvals.remove(request_id).is_none() {
            return Err(PersistenceError::NotFound {
                entity_type: "ApprovalRequest".to_string(),
                entity_id: request_id.to_string(),
            });
        }

        Ok(())
    }

    async fn list_by_execution(&self, execution_id: &str) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("list_by_execution"))?;

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| req.execution_id == execution_id)
            .cloned()
            .collect();

        // Sort by created_at (newest first)
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(results)
    }

    async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("list_by_workflow"))?;

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| req.workflow_id == workflow_id)
            .cloned()
            .collect();

        // Sort by created_at (newest first)
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let results: Vec<ApprovalRequest> = results.into_iter().skip(offset).take(limit).collect();

        Ok(results)
    }

    async fn list_by_status(
        &self,
        status: ApprovalStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("list_by_status"))?;

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| req.status == status)
            .cloned()
            .collect();

        // Sort by created_at (newest first)
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let results: Vec<ApprovalRequest> = results.into_iter().skip(offset).take(limit).collect();

        Ok(results)
    }

    async fn list_pending(&self, limit: usize) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("list_pending"))?;

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| req.status == ApprovalStatus::Pending)
            .cloned()
            .collect();

        // Sort by urgency (expiring soonest first)
        results.sort_by(|a, b| a.expires_at.cmp(&b.expires_at));

        // Apply limit
        let results: Vec<ApprovalRequest> = results.into_iter().take(limit).collect();

        Ok(results)
    }

    async fn find_expired(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("find_expired"))?;

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| req.status == ApprovalStatus::Pending && req.expires_at < as_of)
            .cloned()
            .collect();

        // Sort by expiration time (oldest first)
        results.sort_by(|a, b| a.expires_at.cmp(&b.expires_at));

        // Apply limit
        let results: Vec<ApprovalRequest> = results.into_iter().take(limit).collect();

        Ok(results)
    }

    async fn find_urgent(&self, within_secs: u64, limit: usize) -> Result<Vec<ApprovalRequest>> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("find_urgent"))?;

        let now = Utc::now();
        let urgency_threshold = now + chrono::Duration::seconds(within_secs as i64);

        let mut results: Vec<ApprovalRequest> = approvals
            .values()
            .filter(|req| {
                req.status == ApprovalStatus::Pending
                    && req.expires_at > now
                    && req.expires_at <= urgency_threshold
            })
            .cloned()
            .collect();

        // Sort by urgency (expiring soonest first)
        results.sort_by(|a, b| a.expires_at.cmp(&b.expires_at));

        // Apply limit
        let results: Vec<ApprovalRequest> = results.into_iter().take(limit).collect();

        Ok(results)
    }

    async fn count_total(&self) -> Result<usize> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("count_total"))?;

        Ok(approvals.len())
    }

    async fn count_by_status(&self, status: ApprovalStatus) -> Result<usize> {
        let approvals = self
            .approvals
            .read()
            .map_err(|_| Self::lock_error("count_by_status"))?;

        let count = approvals
            .values()
            .filter(|req| req.status == status)
            .count();

        Ok(count)
    }

    async fn health_check(&self) -> bool {
        // Simple health check - try to acquire read lock
        self.approvals.read().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_request(
        id: &str,
        execution_id: &str,
        workflow_id: &str,
        timeout_secs: u64,
    ) -> ApprovalRequest {
        ApprovalRequest::new(
            id.to_string(),
            "ddl_execution".to_string(),
            execution_id.to_string(),
            workflow_id.to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            timeout_secs,
        )
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let backend = InMemoryApprovalBackend::new();
        let request = create_test_request("req_1", "exec_1", "wf_1", 3600);

        // Save
        backend.save(request.clone()).await.unwrap();

        // Get
        let retrieved = backend.get("req_1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().request_id, "req_1");

        // Get non-existent
        let none = backend.get("req_999").await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_save_duplicate_fails() {
        let backend = InMemoryApprovalBackend::new();
        let request = create_test_request("req_1", "exec_1", "wf_1", 3600);

        backend.save(request.clone()).await.unwrap();

        // Try to save again
        let result = backend.save(request.clone()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PersistenceError::AlreadyExists { .. }
        ));
    }

    #[tokio::test]
    async fn test_update() {
        let backend = InMemoryApprovalBackend::new();
        let mut request = create_test_request("req_1", "exec_1", "wf_1", 3600);

        backend.save(request.clone()).await.unwrap();

        // Approve the request
        request.approve("user_alice".to_string()).unwrap();

        // Update
        backend.update(request.clone()).await.unwrap();

        // Verify update
        let retrieved = backend.get("req_1").await.unwrap().unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Approved);
        assert_eq!(retrieved.approved_by, Some("user_alice".to_string()));
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let backend = InMemoryApprovalBackend::new();
        let request = create_test_request("req_999", "exec_1", "wf_1", 3600);

        let result = backend.update(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PersistenceError::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = InMemoryApprovalBackend::new();
        let request = create_test_request("req_1", "exec_1", "wf_1", 3600);

        backend.save(request).await.unwrap();

        // Delete
        backend.delete("req_1").await.unwrap();

        // Verify deleted
        let retrieved = backend.get("req_1").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let backend = InMemoryApprovalBackend::new();

        let result = backend.delete("req_999").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PersistenceError::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_list_by_execution() {
        let backend = InMemoryApprovalBackend::new();

        // Create multiple requests for same execution
        backend
            .save(create_test_request("req_1", "exec_1", "wf_1", 3600))
            .await
            .unwrap();
        backend
            .save(create_test_request("req_2", "exec_1", "wf_1", 3600))
            .await
            .unwrap();
        backend
            .save(create_test_request("req_3", "exec_2", "wf_1", 3600))
            .await
            .unwrap();

        // List by execution_id
        let results = backend.list_by_execution("exec_1").await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.execution_id == "exec_1"));
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let backend = InMemoryApprovalBackend::new();

        // Create multiple requests for same workflow
        backend
            .save(create_test_request("req_1", "exec_1", "wf_1", 3600))
            .await
            .unwrap();
        backend
            .save(create_test_request("req_2", "exec_2", "wf_1", 3600))
            .await
            .unwrap();
        backend
            .save(create_test_request("req_3", "exec_3", "wf_2", 3600))
            .await
            .unwrap();

        // List by workflow_id with pagination
        let results = backend.list_by_workflow("wf_1", 10, 0).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.workflow_id == "wf_1"));
    }

    #[tokio::test]
    async fn test_list_by_workflow_pagination() {
        let backend = InMemoryApprovalBackend::new();

        // Create 5 requests for same workflow
        for i in 0..5 {
            backend
                .save(create_test_request(
                    &format!("req_{}", i),
                    &format!("exec_{}", i),
                    "wf_1",
                    3600,
                ))
                .await
                .unwrap();
        }

        // First page (limit 2)
        let page1 = backend.list_by_workflow("wf_1", 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        // Second page (limit 2, offset 2)
        let page2 = backend.list_by_workflow("wf_1", 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        // Third page (limit 2, offset 4)
        let page3 = backend.list_by_workflow("wf_1", 2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let backend = InMemoryApprovalBackend::new();

        // Create requests with different statuses
        let mut req1 = create_test_request("req_1", "exec_1", "wf_1", 3600);
        let mut req2 = create_test_request("req_2", "exec_2", "wf_1", 3600);
        let req3 = create_test_request("req_3", "exec_3", "wf_1", 3600);

        req1.approve("user_alice".to_string()).unwrap();
        req2.reject("user_bob".to_string(), Some("Not ready".to_string()))
            .unwrap();

        backend.save(req1).await.unwrap();
        backend.save(req2).await.unwrap();
        backend.save(req3).await.unwrap();

        // List by status
        let pending = backend
            .list_by_status(ApprovalStatus::Pending, 10, 0)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, "req_3");

        let approved = backend
            .list_by_status(ApprovalStatus::Approved, 10, 0)
            .await
            .unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].request_id, "req_1");

        let rejected = backend
            .list_by_status(ApprovalStatus::Rejected, 10, 0)
            .await
            .unwrap();
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].request_id, "req_2");
    }

    #[tokio::test]
    async fn test_list_pending() {
        let backend = InMemoryApprovalBackend::new();

        // Create requests with different expiration times
        backend
            .save(create_test_request("req_1", "exec_1", "wf_1", 1800))
            .await
            .unwrap(); // 30 min
        backend
            .save(create_test_request("req_2", "exec_2", "wf_1", 3600))
            .await
            .unwrap(); // 1 hour
        backend
            .save(create_test_request("req_3", "exec_3", "wf_1", 7200))
            .await
            .unwrap(); // 2 hours

        let pending = backend.list_pending(10).await.unwrap();
        assert_eq!(pending.len(), 3);

        // Should be sorted by expiration (soonest first)
        assert_eq!(pending[0].request_id, "req_1");
        assert_eq!(pending[1].request_id, "req_2");
        assert_eq!(pending[2].request_id, "req_3");
    }

    #[tokio::test]
    async fn test_find_expired() {
        let backend = InMemoryApprovalBackend::new();

        // Create request that's already expired (0 second timeout)
        let mut expired_req = create_test_request("req_expired", "exec_1", "wf_1", 0);
        // Manually set expires_at to past
        expired_req.expires_at = Utc::now() - chrono::Duration::seconds(60);

        backend.save(expired_req).await.unwrap();
        backend
            .save(create_test_request("req_valid", "exec_2", "wf_1", 3600))
            .await
            .unwrap();

        // Find expired
        let expired = backend.find_expired(Utc::now(), 10).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].request_id, "req_expired");
    }

    #[tokio::test]
    async fn test_find_urgent() {
        let backend = InMemoryApprovalBackend::new();

        // Create requests with different expiration times
        backend
            .save(create_test_request("req_urgent", "exec_1", "wf_1", 1800))
            .await
            .unwrap(); // 30 min (urgent)
        backend
            .save(create_test_request("req_normal", "exec_2", "wf_1", 7200))
            .await
            .unwrap(); // 2 hours (not urgent)

        // Find urgent (within 1 hour)
        let urgent = backend.find_urgent(3600, 10).await.unwrap();
        assert_eq!(urgent.len(), 1);
        assert_eq!(urgent[0].request_id, "req_urgent");
    }

    #[tokio::test]
    async fn test_count_total() {
        let backend = InMemoryApprovalBackend::new();

        assert_eq!(backend.count_total().await.unwrap(), 0);

        backend
            .save(create_test_request("req_1", "exec_1", "wf_1", 3600))
            .await
            .unwrap();
        backend
            .save(create_test_request("req_2", "exec_2", "wf_1", 3600))
            .await
            .unwrap();

        assert_eq!(backend.count_total().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_count_by_status() {
        let backend = InMemoryApprovalBackend::new();

        let mut req1 = create_test_request("req_1", "exec_1", "wf_1", 3600);
        let req2 = create_test_request("req_2", "exec_2", "wf_1", 3600);

        req1.approve("user_alice".to_string()).unwrap();

        backend.save(req1).await.unwrap();
        backend.save(req2).await.unwrap();

        assert_eq!(
            backend
                .count_by_status(ApprovalStatus::Pending)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            backend
                .count_by_status(ApprovalStatus::Approved)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            backend
                .count_by_status(ApprovalStatus::Rejected)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_health_check() {
        let backend = InMemoryApprovalBackend::new();
        assert!(backend.health_check().await);
    }
}
