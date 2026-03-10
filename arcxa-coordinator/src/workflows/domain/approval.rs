//! Approval Request Domain Types
//!
//! Models for human-in-the-loop approval workflows with timeout support,
//! audit trails, and status tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

/// Approval request for human-in-the-loop workflows
///
/// Represents a paused workflow execution waiting for human approval.
/// Supports timeouts, rejection reasons, and full audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalRequest {
    /// Unique identifier for this approval request
    pub request_id: String,

    /// Type of approval (ddl_execution, data_deletion, config_change, etc.)
    pub approval_type: String,

    /// Associated workflow execution ID
    pub execution_id: String,

    /// Workflow ID that initiated this request
    pub workflow_id: String,

    /// Action index in workflow where execution paused
    pub action_index: usize,

    /// Payload data for approval review (e.g., DDL statements, deletion scope)
    pub payload: JsonValue,

    /// Current approval status
    pub status: ApprovalStatus,

    /// When this approval request was created
    pub created_at: DateTime<Utc>,

    /// When this approval request will expire (if not approved/rejected)
    pub expires_at: DateTime<Utc>,

    /// User who approved (if approved)
    pub approved_by: Option<String>,

    /// When approval was granted
    pub approved_at: Option<DateTime<Utc>>,

    /// User who rejected (if rejected)
    pub rejected_by: Option<String>,

    /// When approval was rejected
    pub rejected_at: Option<DateTime<Utc>>,

    /// Reason for rejection (if rejected)
    pub rejection_reason: Option<String>,

    /// Additional metadata for tracking
    pub metadata: Option<JsonValue>,
}

/// Approval status lifecycle
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// Waiting for human approval
    Pending,

    /// Approved by human reviewer
    Approved,

    /// Rejected by human reviewer
    Rejected,

    /// Expired due to timeout (no decision made)
    Expired,

    /// Cancelled (workflow aborted before approval decision)
    Cancelled,
}

impl ApprovalRequest {
    /// Create a new approval request
    ///
    /// ## Arguments
    /// * `request_id` - Unique identifier
    /// * `approval_type` - Type of approval (ddl_execution, etc.)
    /// * `execution_id` - Associated workflow execution
    /// * `workflow_id` - Workflow that initiated request
    /// * `action_index` - Index of action where workflow paused
    /// * `payload` - Data for approval review
    /// * `timeout_secs` - Seconds until expiration
    pub fn new(
        request_id: String,
        approval_type: String,
        execution_id: String,
        workflow_id: String,
        action_index: usize,
        payload: JsonValue,
        timeout_secs: u64,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(timeout_secs as i64);

        Self {
            request_id,
            approval_type,
            execution_id,
            workflow_id,
            action_index,
            payload,
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            metadata: None,
        }
    }

    /// Approve this request
    ///
    /// ## Returns
    /// - Ok(()) if approval succeeded
    /// - Err if not in Pending status
    pub fn approve(&mut self, approved_by: String) -> Result<(), String> {
        if self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot approve request in status {:?}, must be Pending",
                self.status
            ));
        }

        if self.is_expired() {
            return Err("Cannot approve expired request".to_string());
        }

        self.status = ApprovalStatus::Approved;
        self.approved_by = Some(approved_by);
        self.approved_at = Some(Utc::now());

        Ok(())
    }

    /// Reject this request
    ///
    /// ## Returns
    /// - Ok(()) if rejection succeeded
    /// - Err if not in Pending status
    pub fn reject(&mut self, rejected_by: String, reason: Option<String>) -> Result<(), String> {
        if self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot reject request in status {:?}, must be Pending",
                self.status
            ));
        }

        self.status = ApprovalStatus::Rejected;
        self.rejected_by = Some(rejected_by);
        self.rejected_at = Some(Utc::now());
        self.rejection_reason = reason;

        Ok(())
    }

    /// Cancel this request (workflow aborted)
    ///
    /// ## Returns
    /// - Ok(()) if cancellation succeeded
    /// - Err if already in terminal status (Approved/Rejected/Expired)
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.status.is_terminal() && self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot cancel request in terminal status {:?}",
                self.status
            ));
        }

        self.status = ApprovalStatus::Cancelled;

        Ok(())
    }

    /// Mark this request as expired
    ///
    /// Called by timeout handler when approval deadline passes.
    ///
    /// ## Returns
    /// - Ok(()) if expiration succeeded
    /// - Err if not in Pending status
    pub fn expire(&mut self) -> Result<(), String> {
        if self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot expire request in status {:?}, must be Pending",
                self.status
            ));
        }

        self.status = ApprovalStatus::Expired;

        Ok(())
    }

    /// Check if this request is expired based on current time
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if approval decision can be made
    ///
    /// Returns false if expired or already decided.
    pub fn can_decide(&self) -> bool {
        self.status == ApprovalStatus::Pending && !self.is_expired()
    }

    /// Get time remaining until expiration (in seconds)
    ///
    /// Returns 0 if already expired.
    pub fn time_remaining_secs(&self) -> u64 {
        let now = Utc::now();
        if now > self.expires_at {
            0
        } else {
            (self.expires_at - now).num_seconds() as u64
        }
    }

    /// Check if this request requires immediate attention
    ///
    /// Returns true if expiring within 1 hour.
    pub fn is_urgent(&self) -> bool {
        self.status == ApprovalStatus::Pending && self.time_remaining_secs() < 3600
    }

    /// Add metadata to this request
    pub fn set_metadata(&mut self, metadata: JsonValue) {
        self.metadata = Some(metadata);
    }
}

impl ApprovalStatus {
    /// Check if this status is terminal (no further transitions possible)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ApprovalStatus::Approved
                | ApprovalStatus::Rejected
                | ApprovalStatus::Expired
                | ApprovalStatus::Cancelled
        )
    }

    /// Check if this status represents a positive decision
    pub fn is_approved(&self) -> bool {
        matches!(self, ApprovalStatus::Approved)
    }

    /// Check if this status represents a negative decision
    pub fn is_rejected(&self) -> bool {
        matches!(
            self,
            ApprovalStatus::Rejected | ApprovalStatus::Expired | ApprovalStatus::Cancelled
        )
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "Awaiting approval",
            ApprovalStatus::Approved => "Approved",
            ApprovalStatus::Rejected => "Rejected",
            ApprovalStatus::Expired => "Expired (timeout)",
            ApprovalStatus::Cancelled => "Cancelled",
        }
    }
}

impl fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApprovalStatus::Pending => write!(f, "Pending"),
            ApprovalStatus::Approved => write!(f, "Approved"),
            ApprovalStatus::Rejected => write!(f, "Rejected"),
            ApprovalStatus::Expired => write!(f, "Expired"),
            ApprovalStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_approval_request_creation() {
        let request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        assert_eq!(request.request_id, "req_123");
        assert_eq!(request.approval_type, "ddl_execution");
        assert_eq!(request.execution_id, "exec_456");
        assert_eq!(request.workflow_id, "wf_789");
        assert_eq!(request.action_index, 2);
        assert_eq!(request.status, ApprovalStatus::Pending);
        assert!(request.can_decide());
        assert!(!request.is_expired());
    }

    #[test]
    fn test_approve_flow() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        // Approve
        let result = request.approve("user_alice".to_string());
        assert!(result.is_ok());
        assert_eq!(request.status, ApprovalStatus::Approved);
        assert_eq!(request.approved_by, Some("user_alice".to_string()));
        assert!(request.approved_at.is_some());

        // Cannot approve twice
        let result2 = request.approve("user_bob".to_string());
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("must be Pending"));
    }

    #[test]
    fn test_reject_flow() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "DROP TABLE production"}),
            3600,
        );

        // Reject with reason
        let result = request.reject(
            "user_bob".to_string(),
            Some("Too dangerous - production table".to_string()),
        );
        assert!(result.is_ok());
        assert_eq!(request.status, ApprovalStatus::Rejected);
        assert_eq!(request.rejected_by, Some("user_bob".to_string()));
        assert!(request.rejected_at.is_some());
        assert_eq!(
            request.rejection_reason,
            Some("Too dangerous - production table".to_string())
        );

        // Cannot reject twice
        let result2 = request.reject("user_alice".to_string(), None);
        assert!(result2.is_err());
    }

    #[test]
    fn test_cancel_flow() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        // Cancel
        let result = request.cancel();
        assert!(result.is_ok());
        assert_eq!(request.status, ApprovalStatus::Cancelled);

        // Cannot approve after cancel
        let result2 = request.approve("user_alice".to_string());
        assert!(result2.is_err());
    }

    #[test]
    fn test_expire_flow() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        // Expire
        let result = request.expire();
        assert!(result.is_ok());
        assert_eq!(request.status, ApprovalStatus::Expired);

        // Cannot approve after expiration
        let result2 = request.approve("user_alice".to_string());
        assert!(result2.is_err());
    }

    #[test]
    fn test_status_terminal() {
        assert!(!ApprovalStatus::Pending.is_terminal());
        assert!(ApprovalStatus::Approved.is_terminal());
        assert!(ApprovalStatus::Rejected.is_terminal());
        assert!(ApprovalStatus::Expired.is_terminal());
        assert!(ApprovalStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_status_approved_rejected() {
        assert!(ApprovalStatus::Approved.is_approved());
        assert!(!ApprovalStatus::Rejected.is_approved());
        assert!(!ApprovalStatus::Pending.is_approved());

        assert!(ApprovalStatus::Rejected.is_rejected());
        assert!(ApprovalStatus::Expired.is_rejected());
        assert!(ApprovalStatus::Cancelled.is_rejected());
        assert!(!ApprovalStatus::Approved.is_rejected());
    }

    #[test]
    fn test_time_remaining() {
        let request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        let remaining = request.time_remaining_secs();
        // Should be close to 3600, allow 5 second margin for test execution
        assert!(remaining >= 3595 && remaining <= 3600);
    }

    #[test]
    fn test_is_urgent() {
        // Not urgent (1 day timeout)
        let request1 = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            86400,
        );
        assert!(!request1.is_urgent());

        // Urgent (30 minute timeout)
        let request2 = ApprovalRequest::new(
            "req_124".to_string(),
            "ddl_execution".to_string(),
            "exec_457".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            1800,
        );
        assert!(request2.is_urgent());
    }

    #[test]
    fn test_can_decide() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        // Initially can decide
        assert!(request.can_decide());

        // After approval, cannot decide
        request.approve("user_alice".to_string()).unwrap();
        assert!(!request.can_decide());
    }

    #[test]
    fn test_metadata() {
        let mut request = ApprovalRequest::new(
            "req_123".to_string(),
            "ddl_execution".to_string(),
            "exec_456".to_string(),
            "wf_789".to_string(),
            2,
            json!({"ddl": "CREATE TABLE test (id INT)"}),
            3600,
        );

        assert!(request.metadata.is_none());

        request.set_metadata(json!({"user_ip": "10.0.0.1", "client": "web_ui"}));
        assert!(request.metadata.is_some());
    }
}
