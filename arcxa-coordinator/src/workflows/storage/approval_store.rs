//! Approval Storage - CRUD operations for approval requests
//!
//! Provides a high-level API for approval request storage with support
//! for pluggable backends via the ApprovalStoreBackend trait.

use crate::workflows::domain::{ApprovalRequest, ApprovalStatus};
use crate::workflows::storage::persistence::{
    ApprovalStoreBackend, InMemoryApprovalBackend, PersistenceError,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Approval request storage with pluggable backend support
///
/// Provides a high-level API for approval request operations.
/// Uses ApprovalStoreBackend trait for storage, allowing different
/// implementations (in-memory, RocksDB, etc.).
#[derive(Clone)]
pub struct ApprovalStore {
    backend: Arc<dyn ApprovalStoreBackend>,
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalStore {
    /// Create a new approval store with in-memory backend
    pub fn new() -> Self {
        Self::with_backend(Arc::new(InMemoryApprovalBackend::new()))
    }

    /// Create an approval store with a custom backend
    pub fn with_backend(backend: Arc<dyn ApprovalStoreBackend>) -> Self {
        Self { backend }
    }

    /// Save a new approval request
    ///
    /// ## Errors
    /// - If request ID already exists
    pub async fn save(&self, request: ApprovalRequest) -> Result<()> {
        self.backend
            .save(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Get an approval request by ID
    pub async fn get(&self, request_id: &str) -> Result<Option<ApprovalRequest>> {
        self.backend
            .get(request_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Get an approval request by ID (required, returns error if not found)
    pub async fn get_required(&self, request_id: &str) -> Result<ApprovalRequest> {
        self.get(request_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Approval request '{}' not found", request_id))
    }

    /// Update an existing approval request
    ///
    /// ## Errors
    /// - If request doesn't exist
    pub async fn update(&self, request: ApprovalRequest) -> Result<()> {
        self.backend
            .update(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Delete an approval request
    ///
    /// ## Errors
    /// - If request doesn't exist
    pub async fn delete(&self, request_id: &str) -> Result<()> {
        self.backend
            .delete(request_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Approve an approval request
    ///
    /// Convenience method that fetches, approves, and updates the request.
    ///
    /// ## Errors
    /// - If request not found
    /// - If request not in Pending status
    /// - If request is expired
    pub async fn approve(&self, request_id: &str, approved_by: String) -> Result<()> {
        let mut request = self.get_required(request_id).await?;

        request
            .approve(approved_by)
            .map_err(|e| anyhow::anyhow!("Failed to approve request: {}", e))?;

        self.update(request).await
    }

    /// Reject an approval request
    ///
    /// Convenience method that fetches, rejects, and updates the request.
    ///
    /// ## Errors
    /// - If request not found
    /// - If request not in Pending status
    pub async fn reject(
        &self,
        request_id: &str,
        rejected_by: String,
        reason: Option<String>,
    ) -> Result<()> {
        let mut request = self.get_required(request_id).await?;

        request
            .reject(rejected_by, reason)
            .map_err(|e| anyhow::anyhow!("Failed to reject request: {}", e))?;

        self.update(request).await
    }

    /// Cancel an approval request (workflow aborted)
    ///
    /// Convenience method that fetches, cancels, and updates the request.
    ///
    /// ## Errors
    /// - If request not found
    pub async fn cancel(&self, request_id: &str) -> Result<()> {
        let mut request = self.get_required(request_id).await?;

        request
            .cancel()
            .map_err(|e| anyhow::anyhow!("Failed to cancel request: {}", e))?;

        self.update(request).await
    }

    /// Mark an approval request as expired
    ///
    /// Convenience method that fetches, expires, and updates the request.
    /// Used by timeout handler.
    ///
    /// ## Errors
    /// - If request not found
    /// - If request not in Pending status
    pub async fn expire(&self, request_id: &str) -> Result<()> {
        let mut request = self.get_required(request_id).await?;

        request
            .expire()
            .map_err(|e| anyhow::anyhow!("Failed to expire request: {}", e))?;

        self.update(request).await
    }

    /// List all approval requests for a specific execution
    pub async fn list_by_execution(&self, execution_id: &str) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .list_by_execution(execution_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// List all approval requests for a specific workflow
    pub async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .list_by_workflow(workflow_id, limit, offset)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// List approval requests by status
    pub async fn list_by_status(
        &self,
        status: ApprovalStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .list_by_status(status, limit, offset)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// List all pending approval requests
    ///
    /// Returns requests sorted by urgency (expiring soonest first).
    pub async fn list_pending(&self, limit: usize) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .list_pending(limit)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Find expired approval requests
    ///
    /// Returns all pending requests where expires_at < as_of.
    pub async fn find_expired(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .find_expired(as_of, limit)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Find urgent approval requests
    ///
    /// Returns pending requests expiring within the given timeframe.
    pub async fn find_urgent(
        &self,
        within_secs: u64,
        limit: usize,
    ) -> Result<Vec<ApprovalRequest>> {
        self.backend
            .find_urgent(within_secs, limit)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Count total approval requests
    pub async fn count_total(&self) -> Result<usize> {
        self.backend
            .count_total()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Count approval requests by status
    pub async fn count_by_status(&self, status: ApprovalStatus) -> Result<usize> {
        self.backend
            .count_by_status(status)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Health check - verify storage backend is responsive
    pub async fn health_check(&self) -> bool {
        self.backend.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_request(id: &str) -> ApprovalRequest {
        ApprovalRequest::new(
            id.to_string(),
            "ddl_execution".to_string(),
            "exec_123".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        )
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request.clone()).await.unwrap();

        let retrieved = store.get("req_1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().request_id, "req_1");
    }

    #[tokio::test]
    async fn test_get_required() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();

        // Should succeed
        let retrieved = store.get_required("req_1").await.unwrap();
        assert_eq!(retrieved.request_id, "req_1");

        // Should fail
        let result = store.get_required("req_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_approve() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();

        // Approve
        store
            .approve("req_1", "user_alice".to_string())
            .await
            .unwrap();

        // Verify
        let retrieved = store.get_required("req_1").await.unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Approved);
        assert_eq!(retrieved.approved_by, Some("user_alice".to_string()));
    }

    #[tokio::test]
    async fn test_approve_twice_fails() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();
        store
            .approve("req_1", "user_alice".to_string())
            .await
            .unwrap();

        // Try to approve again
        let result = store.approve("req_1", "user_bob".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reject() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();

        // Reject
        store
            .reject(
                "req_1",
                "user_bob".to_string(),
                Some("Too dangerous".to_string()),
            )
            .await
            .unwrap();

        // Verify
        let retrieved = store.get_required("req_1").await.unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Rejected);
        assert_eq!(retrieved.rejected_by, Some("user_bob".to_string()));
        assert_eq!(
            retrieved.rejection_reason,
            Some("Too dangerous".to_string())
        );
    }

    #[tokio::test]
    async fn test_cancel() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();

        // Cancel
        store.cancel("req_1").await.unwrap();

        // Verify
        let retrieved = store.get_required("req_1").await.unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_expire() {
        let store = ApprovalStore::new();
        let request = create_test_request("req_1");

        store.save(request).await.unwrap();

        // Expire
        store.expire("req_1").await.unwrap();

        // Verify
        let retrieved = store.get_required("req_1").await.unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Expired);
    }

    #[tokio::test]
    async fn test_list_by_execution() {
        let store = ApprovalStore::new();

        let mut req1 = create_test_request("req_1");
        req1.execution_id = "exec_123".to_string();

        let mut req2 = create_test_request("req_2");
        req2.execution_id = "exec_123".to_string();

        let mut req3 = create_test_request("req_3");
        req3.execution_id = "exec_456".to_string();

        store.save(req1).await.unwrap();
        store.save(req2).await.unwrap();
        store.save(req3).await.unwrap();

        let results = store.list_by_execution("exec_123").await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_list_pending() {
        let store = ApprovalStore::new();

        let mut req1 = create_test_request("req_1");
        let req2 = create_test_request("req_2");

        req1.approve("user_alice".to_string()).unwrap();

        store.save(req1).await.unwrap();
        store.save(req2).await.unwrap();

        let pending = store.list_pending(10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, "req_2");
    }

    #[tokio::test]
    async fn test_count() {
        let store = ApprovalStore::new();

        assert_eq!(store.count_total().await.unwrap(), 0);

        store.save(create_test_request("req_1")).await.unwrap();
        store.save(create_test_request("req_2")).await.unwrap();

        assert_eq!(store.count_total().await.unwrap(), 2);
        assert_eq!(
            store
                .count_by_status(ApprovalStatus::Pending)
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn test_health_check() {
        let store = ApprovalStore::new();
        assert!(store.health_check().await);
    }
}
