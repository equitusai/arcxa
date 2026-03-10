//! Integration test for approval workflow system
//!
//! Tests the approval system components:
//! - Approval request creation and storage
//! - Approval state transitions (pending → approved/rejected/expired)
//! - Listing and querying approvals
//! - Timeout handling

use graphica_coordinator::workflows::{
    domain::{ApprovalRequest, ApprovalStatus},
    storage::ApprovalStore,
};
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_approval_request_creation() {
    let store = Arc::new(ApprovalStore::new());

    // Create an approval request
    let request = ApprovalRequest::new(
        "test_approval_001".to_string(),
        "ddl_execution".to_string(),
        "exec_123".to_string(),
        "workflow_456".to_string(),
        2, // action_index
        json!({
            "operation": "CREATE_TABLE",
            "table": "CUSTOMERS"
        }),
        3600, // 1 hour timeout
    );

    // Save to store
    store.save(request.clone()).await.unwrap();

    // Verify it was saved
    let retrieved = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(retrieved.request_id, "test_approval_001");
    assert_eq!(retrieved.approval_type, "ddl_execution");
    assert_eq!(retrieved.execution_id, "exec_123");
    assert_eq!(retrieved.workflow_id, "workflow_456");
    assert_eq!(retrieved.status, ApprovalStatus::Pending);
    assert!(retrieved.approved_by.is_none());
    assert!(retrieved.approved_at.is_none());
}

#[tokio::test]
async fn test_approval_transition_to_approved() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_002".to_string(),
        "data_deletion".to_string(),
        "exec_124".to_string(),
        "workflow_457".to_string(),
        2, // action_index
        json!({"operation": "DELETE"}),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Approve the request
    store
        .approve(&request.request_id, "admin@example.com".to_string())
        .await
        .unwrap();

    // Verify approval
    let approved = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(approved.status, ApprovalStatus::Approved);
    assert_eq!(approved.approved_by, Some("admin@example.com".to_string()));
    assert!(approved.approved_at.is_some());
}

#[tokio::test]
async fn test_approval_transition_to_rejected() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_003".to_string(),
        "schema_change".to_string(),
        "exec_125".to_string(),
        "workflow_458".to_string(),
        2, // action_index
        json!({"operation": "ALTER_TABLE"}),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Reject the request
    store
        .reject(
            &request.request_id,
            "admin@example.com".to_string(),
            Some("Incorrect table name".to_string()),
        )
        .await
        .unwrap();

    // Verify rejection
    let rejected = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(rejected.status, ApprovalStatus::Rejected);
    assert_eq!(rejected.rejected_by, Some("admin@example.com".to_string()));
    assert_eq!(
        rejected.rejection_reason,
        Some("Incorrect table name".to_string())
    );
}

#[tokio::test]
async fn test_approval_transition_to_expired() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_004".to_string(),
        "test".to_string(),
        "exec_126".to_string(),
        "workflow_459".to_string(),
        2, // action_index
        json!({}),
        2, // Very short timeout for testing
    );

    store.save(request.clone()).await.unwrap();

    // Wait for timeout to pass
    sleep(Duration::from_secs(3)).await;

    // Manually expire the request (simulating timeout handler)
    store.expire(&request.request_id).await.unwrap();

    // Verify expiration
    let expired = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(expired.status, ApprovalStatus::Expired);
}

#[tokio::test]
async fn test_approval_cancel() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_005".to_string(),
        "test".to_string(),
        "exec_127".to_string(),
        "workflow_460".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Cancel the request
    store.cancel(&request.request_id).await.unwrap();

    // Verify cancellation
    let cancelled = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(cancelled.status, ApprovalStatus::Cancelled);
}

#[tokio::test]
async fn test_list_pending_approvals() {
    let store = Arc::new(ApprovalStore::new());

    // Create multiple approval requests
    for i in 0..5 {
        let request = ApprovalRequest::new(
            format!("test_approval_00{}", i + 6),
            "test".to_string(),
            format!("exec_12{}", i + 8),
            "workflow_list_test".to_string(),
            2, // action_index
            json!({}),
            3600,
        );
        store.save(request).await.unwrap();
    }

    // Approve one of them
    store
        .approve("test_approval_006", "admin@example.com".to_string())
        .await
        .unwrap();

    // List pending approvals
    let pending = store.list_pending(100).await.unwrap();

    // Should be 4 pending (5 created - 1 approved)
    assert_eq!(pending.len(), 4);

    // All should be in Pending status
    for approval in &pending {
        assert_eq!(approval.status, ApprovalStatus::Pending);
    }
}

#[tokio::test]
async fn test_list_approvals_by_execution() {
    let store = Arc::new(ApprovalStore::new());

    // Create approvals for different executions
    let request1 = ApprovalRequest::new(
        "test_approval_011".to_string(),
        "test".to_string(),
        "exec_target".to_string(),
        "workflow_exec_test".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    let request2 = ApprovalRequest::new(
        "test_approval_012".to_string(),
        "test".to_string(),
        "exec_target".to_string(),
        "workflow_exec_test".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    let request3 = ApprovalRequest::new(
        "test_approval_013".to_string(),
        "test".to_string(),
        "exec_other".to_string(),
        "workflow_exec_test".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    store.save(request1).await.unwrap();
    store.save(request2).await.unwrap();
    store.save(request3).await.unwrap();

    // List approvals for specific execution
    let exec_approvals = store.list_by_execution("exec_target").await.unwrap();

    // Should find 2 approvals for exec_target
    assert_eq!(exec_approvals.len(), 2);

    for approval in &exec_approvals {
        assert_eq!(approval.execution_id, "exec_target");
    }
}

#[tokio::test]
async fn test_list_approvals_by_workflow() {
    let store = Arc::new(ApprovalStore::new());

    // Create approvals for different workflows
    for i in 0..3 {
        let request = ApprovalRequest::new(
            format!("test_approval_01{}", i + 4),
            "test".to_string(),
            format!("exec_{}", i),
            "workflow_target".to_string(),
            2, // action_index
            json!({}),
            3600,
        );
        store.save(request).await.unwrap();
    }

    // List approvals for specific workflow
    let workflow_approvals = store
        .list_by_workflow("workflow_target", 100, 0)
        .await
        .unwrap();

    // Should find 3 approvals for workflow_target
    assert_eq!(workflow_approvals.len(), 3);

    for approval in &workflow_approvals {
        assert_eq!(approval.workflow_id, "workflow_target");
    }
}

#[tokio::test]
async fn test_list_approvals_by_status() {
    let store = Arc::new(ApprovalStore::new());

    // Create multiple approvals
    for i in 0..3 {
        let request = ApprovalRequest::new(
            format!("test_approval_status_{}", i),
            "test".to_string(),
            format!("exec_{}", i),
            "workflow_status_test".to_string(),
            2, // action_index
            json!({}),
            3600,
        );
        store.save(request).await.unwrap();
    }

    // Approve one
    store
        .approve("test_approval_status_0", "admin@example.com".to_string())
        .await
        .unwrap();

    // Reject one
    store
        .reject(
            "test_approval_status_1",
            "admin@example.com".to_string(),
            None,
        )
        .await
        .unwrap();

    // List by status
    let approved = store
        .list_by_status(ApprovalStatus::Approved, 100, 0)
        .await
        .unwrap();
    let rejected = store
        .list_by_status(ApprovalStatus::Rejected, 100, 0)
        .await
        .unwrap();
    let pending = store
        .list_by_status(ApprovalStatus::Pending, 100, 0)
        .await
        .unwrap();

    assert!(!approved.is_empty());
    assert!(!rejected.is_empty());
    assert!(!pending.is_empty());
}

#[tokio::test]
async fn test_approval_deletion() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_delete".to_string(),
        "test".to_string(),
        "exec_delete".to_string(),
        "workflow_delete_test".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Verify it exists
    assert!(store.get(&request.request_id).await.unwrap().is_some());

    // Delete it
    store.delete(&request.request_id).await.unwrap();

    // Verify it's gone
    assert!(store.get(&request.request_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_approval_invalid_state_transition() {
    let store = Arc::new(ApprovalStore::new());

    let request = ApprovalRequest::new(
        "test_approval_invalid".to_string(),
        "test".to_string(),
        "exec_invalid".to_string(),
        "workflow_invalid".to_string(),
        2, // action_index
        json!({}),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Approve it
    store
        .approve(&request.request_id, "admin@example.com".to_string())
        .await
        .unwrap();

    // Try to reject an already-approved request - should fail
    let result = store
        .reject(&request.request_id, "admin2@example.com".to_string(), None)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_approval_payload_preservation() {
    let store = Arc::new(ApprovalStore::new());

    let complex_payload = json!({
        "operation": "CREATE_TABLE",
        "ddl": "CREATE TABLE customers (id INT PRIMARY KEY, name VARCHAR(100))",
        "schema": "production",
        "estimated_rows": 1000000,
        "risk_level": "high",
        "metadata": {
            "requested_by": "user@example.com",
            "reason": "New customer dimension table",
            "ticket": "JIRA-12345"
        }
    });

    let request = ApprovalRequest::new(
        "test_approval_payload".to_string(),
        "ddl_execution".to_string(),
        "exec_payload".to_string(),
        "workflow_payload".to_string(),
        2, // action_index
        complex_payload.clone(),
        3600,
    );

    store.save(request.clone()).await.unwrap();

    // Retrieve and verify payload is preserved
    let retrieved = store.get(&request.request_id).await.unwrap().unwrap();
    assert_eq!(retrieved.payload, complex_payload);

    // Verify nested fields
    assert_eq!(retrieved.payload["operation"], "CREATE_TABLE");
    assert_eq!(retrieved.payload["estimated_rows"], 1000000);
    assert_eq!(retrieved.payload["metadata"]["ticket"], "JIRA-12345");
}
