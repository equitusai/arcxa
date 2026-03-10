//! Storage Abstraction Traits
//!
//! Defines trait interfaces for workflow execution storage backends,
//! enabling multiple storage implementations (in-memory, RocksDB, etc.)
//! with consistent APIs.

use super::error::Result;
use crate::workflows::domain::{
    ApprovalRequest, ApprovalStatus, ExecutionLog, ExecutionStatus, WorkflowExecution,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Core storage backend trait for workflow execution persistence
///
/// This trait defines the contract for storing and retrieving workflow
/// execution state. Implementations can use different backends (in-memory,
/// RocksDB, PostgreSQL, etc.) while providing the same interface.
///
/// # Design Principles
///
/// - Async-first: All operations are async to support I/O-bound backends
/// - Error handling: All operations return Result for proper error propagation
/// - Immutability: Updates create new versions rather than mutating state
/// - Query support: Efficient queries by various dimensions
///
/// # Example Implementation
///
/// ```ignore
/// pub struct RocksDbBackend {
///     db: Arc<rocksdb::DB>,
/// }
///
/// #[async_trait]
/// impl ExecutionStoreBackend for RocksDbBackend {
///     async fn save(&self, execution: WorkflowExecution) -> Result<()> {
///         // Serialize and store in RocksDB
///     }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait ExecutionStoreBackend: Send + Sync {
    /// Save a new workflow execution
    ///
    /// Creates a new execution record. Fails if an execution with the same
    /// ID already exists.
    ///
    /// # Arguments
    ///
    /// * `execution` - The workflow execution to save
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Execution already exists (AlreadyExists)
    /// - Serialization fails
    /// - Storage backend unavailable
    async fn save(&self, execution: WorkflowExecution) -> Result<()>;

    /// Retrieve a workflow execution by ID
    ///
    /// Returns the current state of the execution, or None if not found.
    ///
    /// # Arguments
    ///
    /// * `id` - The execution ID to retrieve
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Deserialization fails
    /// - Storage backend unavailable
    async fn get(&self, id: &str) -> Result<Option<WorkflowExecution>>;

    /// Update an existing workflow execution
    ///
    /// Updates the execution state. In event-sourced implementations,
    /// this appends a new event rather than mutating the record.
    ///
    /// # Arguments
    ///
    /// * `execution` - The updated execution state
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Execution not found (NotFound)
    /// - Version conflict (optimistic locking failure)
    /// - Serialization fails
    /// - Storage backend unavailable
    async fn update(&self, execution: WorkflowExecution) -> Result<()>;

    /// Delete a workflow execution
    ///
    /// Removes the execution from storage. In event-sourced implementations,
    /// this may be a soft delete (tombstone marker).
    ///
    /// # Arguments
    ///
    /// * `id` - The execution ID to delete
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Execution not found (NotFound)
    /// - Storage backend unavailable
    async fn delete(&self, id: &str) -> Result<()>;

    /// List all executions for a specific workflow
    ///
    /// Returns executions in reverse chronological order (newest first).
    ///
    /// # Arguments
    ///
    /// * `workflow_id` - The workflow ID to filter by
    /// * `limit` - Maximum number of results (for pagination)
    /// * `offset` - Number of results to skip (for pagination)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>>;

    /// List executions by status
    ///
    /// Returns all executions matching the given status.
    ///
    /// # Arguments
    ///
    /// * `status` - The execution status to filter by
    /// * `limit` - Maximum number of results
    /// * `offset` - Number of results to skip
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_status(
        &self,
        status: ExecutionStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>>;

    /// List executions within a time range
    ///
    /// Returns executions started between the given timestamps.
    ///
    /// # Arguments
    ///
    /// * `start_time` - Beginning of time range (inclusive)
    /// * `end_time` - End of time range (inclusive)
    /// * `limit` - Maximum number of results
    /// * `offset` - Number of results to skip
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_time_range(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowExecution>>;

    /// Append a log entry to an execution
    ///
    /// Adds a new log entry to the execution's log history.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution to add the log to
    /// * `log` - The log entry to append
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Execution not found
    /// - Storage backend unavailable
    async fn add_log(&self, execution_id: &str, log: ExecutionLog) -> Result<()>;

    /// Get all log entries for an execution
    ///
    /// Returns logs in chronological order (oldest first).
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Execution not found
    /// - Storage backend unavailable
    async fn get_logs(&self, execution_id: &str) -> Result<Vec<ExecutionLog>>;

    /// Count total executions (for statistics)
    ///
    /// Returns the total number of executions in storage.
    async fn count_total(&self) -> Result<usize>;

    /// Count executions by status (for statistics)
    ///
    /// Returns the number of executions in the given status.
    async fn count_by_status(&self, status: ExecutionStatus) -> Result<usize>;

    /// Health check - verify storage backend is responsive
    ///
    /// Performs a lightweight operation to verify the backend is available.
    /// Returns true if healthy, false otherwise.
    async fn health_check(&self) -> bool;
}

/// Checkpoint storage trait
///
/// Defines operations for storing and retrieving execution checkpoints.
/// Checkpoints are periodic snapshots of execution state used for recovery.
///
/// # Checkpoint Strategy
///
/// - Periodic: Checkpoints taken at regular intervals (e.g., 30 seconds)
/// - Incremental: Only changed executions since last checkpoint
/// - Retention: Keep last N checkpoints, delete older ones
/// - Compression: Checkpoints may be compressed to save space
///
/// # Example
///
/// ```ignore
/// let checkpoint = Checkpoint {
///     id: "ckpt_123".to_string(),
///     timestamp: Utc::now(),
///     execution_count: 100,
///     data_size: 1024 * 1024,  // 1MB
/// };
/// checkpoint_store.save_checkpoint(&checkpoint).await?;
/// ```
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Save a checkpoint
    ///
    /// Persists a checkpoint to storage. Older checkpoints may be
    /// automatically cleaned up based on retention policy.
    ///
    /// # Arguments
    ///
    /// * `checkpoint` - The checkpoint metadata
    /// * `data` - The serialized checkpoint data
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Serialization fails
    /// - Storage quota exceeded
    /// - Storage backend unavailable
    async fn save_checkpoint(&self, checkpoint: &Checkpoint, data: Vec<u8>) -> Result<()>;

    /// Load the latest checkpoint
    ///
    /// Returns the most recent checkpoint, or None if no checkpoints exist.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Deserialization fails
    /// - Checkpoint corrupted
    /// - Storage backend unavailable
    async fn load_latest_checkpoint(&self) -> Result<Option<(Checkpoint, Vec<u8>)>>;

    /// Load a specific checkpoint by ID
    ///
    /// Returns the checkpoint with the given ID, or None if not found.
    ///
    /// # Arguments
    ///
    /// * `checkpoint_id` - The checkpoint ID to load
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Deserialization fails
    /// - Checkpoint corrupted
    /// - Storage backend unavailable
    async fn load_checkpoint(&self, checkpoint_id: &str) -> Result<Option<(Checkpoint, Vec<u8>)>>;

    /// List all available checkpoints
    ///
    /// Returns checkpoints in reverse chronological order (newest first).
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of checkpoints to return
    ///
    /// # Errors
    ///
    /// Returns error if storage backend unavailable
    async fn list_checkpoints(&self, limit: usize) -> Result<Vec<Checkpoint>>;

    /// Delete old checkpoints beyond retention policy
    ///
    /// Removes checkpoints older than the retention count, keeping only
    /// the most recent N checkpoints.
    ///
    /// # Arguments
    ///
    /// * `retain_count` - Number of recent checkpoints to keep
    ///
    /// # Returns
    ///
    /// Number of checkpoints deleted
    ///
    /// # Errors
    ///
    /// Returns error if storage backend unavailable
    async fn cleanup_old_checkpoints(&self, retain_count: usize) -> Result<usize>;
}

/// Checkpoint metadata
///
/// Contains information about a checkpoint without the actual data.
/// Used for listing and querying checkpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint identifier
    pub id: String,

    /// When the checkpoint was created
    pub timestamp: DateTime<Utc>,

    /// Number of executions in this checkpoint
    pub execution_count: usize,

    /// Compressed data size in bytes
    pub data_size: u64,

    /// Checksum for integrity verification (SHA-256 hex)
    pub checksum: Option<String>,

    /// Optional shard storage URL (for distributed backup)
    pub shard_url: Option<String>,
}

impl Checkpoint {
    /// Create a new checkpoint with ID and timestamp
    pub fn new(id: String) -> Self {
        Self {
            id,
            timestamp: Utc::now(),
            execution_count: 0,
            data_size: 0,
            checksum: None,
            shard_url: None,
        }
    }

    /// Calculate checksum for checkpoint data
    pub fn calculate_checksum(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Verify checkpoint data matches checksum
    pub fn verify(&self, data: &[u8]) -> bool {
        match &self.checksum {
            Some(expected) => {
                let actual = Self::calculate_checksum(data);
                actual == *expected
            }
            None => true, // No checksum, assume valid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint::new("ckpt_test".to_string());
        assert_eq!(checkpoint.id, "ckpt_test");
        assert_eq!(checkpoint.execution_count, 0);
        assert_eq!(checkpoint.data_size, 0);
    }

    #[test]
    fn test_checkpoint_checksum() {
        let data = b"test checkpoint data";
        let checksum = Checkpoint::calculate_checksum(data);
        assert_eq!(checksum.len(), 64); // SHA-256 hex is 64 chars

        let mut checkpoint = Checkpoint::new("ckpt_test".to_string());
        checkpoint.checksum = Some(checksum.clone());
        assert!(checkpoint.verify(data));
        assert!(!checkpoint.verify(b"wrong data"));
    }
}

/// Approval request storage trait
///
/// Defines operations for storing and retrieving human approval requests
/// for workflow execution gates. Supports querying by status, workflow,
/// execution, and expiration time.
///
/// # Design Principles
///
/// - Fast lookups: Indexed by request_id, execution_id, workflow_id, status
/// - Time-based queries: Find expired or urgent approvals
/// - Audit trail: Immutable records with approval/rejection history
/// - Status transitions: Only valid state transitions allowed
///
/// # Example
///
/// ```ignore
/// let approval = ApprovalRequest::new(
///     "req_123".to_string(),
///     "ddl_execution".to_string(),
///     "exec_456".to_string(),
///     "wf_789".to_string(),
///     2,
///     json!({"ddl": "CREATE TABLE..."}),
///     3600,
/// );
/// approval_store.save(approval).await?;
/// ```
#[async_trait]
pub trait ApprovalStoreBackend: Send + Sync {
    /// Save a new approval request
    ///
    /// Creates a new approval request. Fails if a request with the same
    /// ID already exists.
    ///
    /// # Arguments
    ///
    /// * `request` - The approval request to save
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Request already exists (AlreadyExists)
    /// - Serialization fails
    /// - Storage backend unavailable
    async fn save(&self, request: ApprovalRequest) -> Result<()>;

    /// Retrieve an approval request by ID
    ///
    /// Returns the current state of the request, or None if not found.
    ///
    /// # Arguments
    ///
    /// * `request_id` - The request ID to retrieve
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Deserialization fails
    /// - Storage backend unavailable
    async fn get(&self, request_id: &str) -> Result<Option<ApprovalRequest>>;

    /// Update an existing approval request
    ///
    /// Updates the request state (typically status change from approval/rejection).
    ///
    /// # Arguments
    ///
    /// * `request` - The updated request state
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Request not found (NotFound)
    /// - Serialization fails
    /// - Storage backend unavailable
    async fn update(&self, request: ApprovalRequest) -> Result<()>;

    /// Delete an approval request
    ///
    /// Removes the request from storage. Typically used for cleanup
    /// after workflow completion.
    ///
    /// # Arguments
    ///
    /// * `request_id` - The request ID to delete
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Request not found (NotFound)
    /// - Storage backend unavailable
    async fn delete(&self, request_id: &str) -> Result<()>;

    /// List all approval requests for a specific execution
    ///
    /// Returns approvals associated with the given workflow execution.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID to filter by
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_execution(&self, execution_id: &str) -> Result<Vec<ApprovalRequest>>;

    /// List all approval requests for a specific workflow
    ///
    /// Returns all approvals for the given workflow, across all executions.
    ///
    /// # Arguments
    ///
    /// * `workflow_id` - The workflow ID to filter by
    /// * `limit` - Maximum number of results
    /// * `offset` - Number of results to skip
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>>;

    /// List approval requests by status
    ///
    /// Returns all requests matching the given status.
    ///
    /// # Arguments
    ///
    /// * `status` - The approval status to filter by
    /// * `limit` - Maximum number of results
    /// * `offset` - Number of results to skip
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_by_status(
        &self,
        status: ApprovalStatus,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApprovalRequest>>;

    /// List all pending approval requests
    ///
    /// Returns all requests awaiting approval, sorted by urgency
    /// (expiring soonest first).
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of results
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn list_pending(&self, limit: usize) -> Result<Vec<ApprovalRequest>>;

    /// Find expired approval requests
    ///
    /// Returns all pending requests where expires_at < current_time.
    /// Used by timeout handler to mark requests as expired.
    ///
    /// # Arguments
    ///
    /// * `as_of` - The timestamp to check expiration against (usually Utc::now())
    /// * `limit` - Maximum number of results
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn find_expired(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<ApprovalRequest>>;

    /// Find urgent approval requests
    ///
    /// Returns pending requests expiring within the given timeframe.
    ///
    /// # Arguments
    ///
    /// * `within_secs` - Find requests expiring within this many seconds
    /// * `limit` - Maximum number of results
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query fails
    /// - Storage backend unavailable
    async fn find_urgent(&self, within_secs: u64, limit: usize) -> Result<Vec<ApprovalRequest>>;

    /// Count total approval requests (for statistics)
    ///
    /// Returns the total number of approval requests in storage.
    async fn count_total(&self) -> Result<usize>;

    /// Count approval requests by status (for statistics)
    ///
    /// Returns the number of requests in the given status.
    async fn count_by_status(&self, status: ApprovalStatus) -> Result<usize>;

    /// Health check - verify storage backend is responsive
    ///
    /// Performs a lightweight operation to verify the backend is available.
    /// Returns true if healthy, false otherwise.
    async fn health_check(&self) -> bool;
}
