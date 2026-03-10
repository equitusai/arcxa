//! Approval Timeout Handler
//!
//! Background task that periodically checks for expired approval requests
//! and performs cleanup operations:
//!
//! 1. Marks expired requests as Expired status
//! 2. Fails associated workflow executions
//! 3. Logs timeout events for auditing
//!
//! ## Architecture
//!
//! ```text
//! ApprovalTimeoutHandler
//!   ├─ Periodic Checker (every N seconds)
//!   ├─ Expiration Detector (checks expires_at)
//!   ├─ Status Updater (Pending → Expired)
//!   └─ Execution Failure (updates WorkflowExecution)
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use graphica_coordinator::workflows::governance::ApprovalTimeoutHandler;
//! use graphica_coordinator::workflows::storage::{ApprovalStore, ExecutionStore};
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! # let approval_store = Arc::new(ApprovalStore::new());
//! # let execution_store = Arc::new(ExecutionStore::new());
//! let handler = ApprovalTimeoutHandler::new(
//!     approval_store,
//!     execution_store,
//!     Duration::from_secs(60), // Check every 60 seconds
//! );
//!
//! // Start background task
//! let handle = handler.start();
//!
//! // Later: stop the handler
//! handle.stop().await?;
//! # Ok(())
//! # }
//! ```

use crate::workflows::domain::{ApprovalStatus, ExecutionStatus};
use crate::workflows::storage::{ApprovalStore, ExecutionStore};
use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Background task handler for approval timeouts
pub struct ApprovalTimeoutHandler {
    /// Approval storage
    approval_store: Arc<ApprovalStore>,

    /// Execution storage for failing timed-out workflows
    execution_store: Arc<ExecutionStore>,

    /// Check interval (how often to scan for expired approvals)
    check_interval: Duration,

    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
}

/// Handle for managing the timeout handler background task
pub struct TimeoutHandlerHandle {
    /// Task handle
    task_handle: JoinHandle<()>,

    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
}

impl ApprovalTimeoutHandler {
    /// Create a new approval timeout handler
    ///
    /// ## Arguments
    /// * `approval_store` - Approval storage
    /// * `execution_store` - Execution storage for updating workflow status
    /// * `check_interval` - How often to check for expired approvals
    pub fn new(
        approval_store: Arc<ApprovalStore>,
        execution_store: Arc<ExecutionStore>,
        check_interval: Duration,
    ) -> Self {
        Self {
            approval_store,
            execution_store,
            check_interval,
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the background timeout handler task
    ///
    /// Returns a handle that can be used to stop the task later.
    pub fn start(self) -> TimeoutHandlerHandle {
        let shutdown = self.shutdown.clone();

        let task_handle = tokio::spawn(async move {
            info!(
                "Starting approval timeout handler (check interval: {:?})",
                self.check_interval
            );

            loop {
                // Check shutdown signal
                {
                    let should_shutdown = *self.shutdown.read().await;
                    if should_shutdown {
                        info!("Approval timeout handler shutting down");
                        break;
                    }
                }

                // Process expired approvals
                match self.process_expired_approvals().await {
                    Ok(count) => {
                        if count > 0 {
                            info!("Processed {} expired approvals", count);
                        } else {
                            debug!("No expired approvals found");
                        }
                    }
                    Err(e) => {
                        error!("Error processing expired approvals: {}", e);
                    }
                }

                // Sleep until next check
                tokio::time::sleep(self.check_interval).await;
            }
        });

        TimeoutHandlerHandle {
            task_handle,
            shutdown,
        }
    }

    /// Process all expired approvals
    ///
    /// Scans for approval requests that have passed their expires_at timestamp
    /// and are still in Pending status. For each expired request:
    /// 1. Updates approval status to Expired
    /// 2. Fails the associated workflow execution
    /// 3. Logs the timeout event
    ///
    /// Returns the number of expired approvals processed.
    async fn process_expired_approvals(&self) -> Result<usize> {
        let now = Utc::now();

        // Fetch all pending approvals
        let pending_requests = self
            .approval_store
            .list_by_status(ApprovalStatus::Pending, 1000, 0)
            .await
            .context("Failed to list pending approvals")?;

        let mut expired_count = 0;

        for request in pending_requests {
            // Check if expired
            if request.expires_at <= now {
                info!(
                    "Processing expired approval: {} (expired at: {}, workflow: {})",
                    request.request_id, request.expires_at, request.workflow_id
                );

                // Mark approval as expired
                match self.approval_store.expire(&request.request_id).await {
                    Ok(_) => {
                        debug!("Marked approval {} as expired", request.request_id);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to mark approval {} as expired: {}",
                            request.request_id, e
                        );
                        continue; // Skip to next approval
                    }
                }

                // Fail the associated workflow execution if it exists
                if !request.execution_id.is_empty() {
                    match self.fail_workflow_execution(&request.execution_id).await {
                        Ok(_) => {
                            info!(
                                "Failed workflow execution {} due to approval timeout",
                                request.execution_id
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to update execution {} status: {}",
                                request.execution_id, e
                            );
                        }
                    }
                }

                expired_count += 1;
            }
        }

        Ok(expired_count)
    }

    /// Fail a workflow execution due to approval timeout
    ///
    /// Updates the execution status to Failed and clears any checkpoint data.
    async fn fail_workflow_execution(&self, execution_id: &str) -> Result<()> {
        // Load execution
        let mut execution = self
            .execution_store
            .get(execution_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Execution not found: {}", execution_id))?;

        // Only fail if currently paused (waiting for approval)
        if execution.status == ExecutionStatus::Paused {
            execution.update_status(ExecutionStatus::Failed);
            execution.clear_checkpoint();
            self.execution_store.update(execution).await?;

            info!(
                "Workflow execution {} failed due to approval timeout",
                execution_id
            );
        } else {
            debug!(
                "Skipping execution {} - status is {:?}, not Paused",
                execution_id, execution.status
            );
        }

        Ok(())
    }
}

impl TimeoutHandlerHandle {
    /// Stop the timeout handler and wait for it to finish
    pub async fn stop(self) -> Result<()> {
        info!("Stopping approval timeout handler");

        // Set shutdown signal
        {
            let mut shutdown = self.shutdown.write().await;
            *shutdown = true;
        }

        // Wait for task to complete
        self.task_handle
            .await
            .context("Failed to join timeout handler task")?;

        info!("Approval timeout handler stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{ApprovalRequest, WorkflowExecution};
    use chrono::Duration as ChronoDuration;
    use serde_json::json;

    #[tokio::test]
    async fn test_timeout_handler_detects_expired_approvals() -> Result<()> {
        // Create stores
        let approval_store = Arc::new(ApprovalStore::new());
        let execution_store = Arc::new(ExecutionStore::new());

        // Create an expired approval
        let expired_request = ApprovalRequest {
            request_id: "appr_expired".to_string(),
            approval_type: "ddl_execution".to_string(),
            execution_id: "exec_123".to_string(),
            workflow_id: "wf_456".to_string(),
            action_index: 0,
            payload: json!({"table": "customers"}),
            status: ApprovalStatus::Pending,
            created_at: Utc::now() - ChronoDuration::hours(2),
            expires_at: Utc::now() - ChronoDuration::hours(1), // Expired 1 hour ago
            approved_by: None,
            rejected_by: None,
            approved_at: None,
            rejected_at: None,
            rejection_reason: None,
            metadata: None,
        };

        approval_store.save(expired_request).await?;

        // Create associated execution in Paused state
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test Workflow".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Paused);
        execution.checkpoint(0, json!({"data": "test"}));
        execution_store.save(execution.clone()).await?;

        // Create handler with short interval
        let handler = ApprovalTimeoutHandler::new(
            approval_store.clone(),
            execution_store.clone(),
            Duration::from_millis(100),
        );

        // Process expired approvals manually (instead of starting background task)
        let count = handler.process_expired_approvals().await?;
        assert_eq!(count, 1, "Should process 1 expired approval");

        // Verify approval status updated
        let updated_approval = approval_store.get_required("appr_expired").await?;
        assert_eq!(updated_approval.status, ApprovalStatus::Expired);

        // Verify execution status updated
        let updated_execution = execution_store.get("exec_123").await?.unwrap();
        assert_eq!(updated_execution.status, ExecutionStatus::Failed);
        assert!(updated_execution.checkpoint_action_index().is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_timeout_handler_ignores_non_expired_approvals() -> Result<()> {
        let approval_store = Arc::new(ApprovalStore::new());
        let execution_store = Arc::new(ExecutionStore::new());

        // Create a non-expired approval
        let active_request = ApprovalRequest {
            request_id: "appr_active".to_string(),
            approval_type: "ddl_execution".to_string(),
            execution_id: "exec_789".to_string(),
            workflow_id: "wf_456".to_string(),
            action_index: 0,
            payload: json!({"table": "products"}),
            status: ApprovalStatus::Pending,
            created_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::hours(1), // Expires in 1 hour
            approved_by: None,
            rejected_by: None,
            approved_at: None,
            rejected_at: None,
            rejection_reason: None,
            metadata: None,
        };

        approval_store.save(active_request).await?;

        let handler = ApprovalTimeoutHandler::new(
            approval_store.clone(),
            execution_store.clone(),
            Duration::from_millis(100),
        );

        // Process - should not affect active approval
        let count = handler.process_expired_approvals().await?;
        assert_eq!(count, 0, "Should not process any approvals");

        // Verify approval still pending
        let approval = approval_store.get_required("appr_active").await?;
        assert_eq!(approval.status, ApprovalStatus::Pending);

        Ok(())
    }
}
