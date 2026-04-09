//! Workflow Storage Layer
//!
//! Persistence for workflows, executions, schedules, and approval requests with support for CRUD operations.

mod approval_store;
mod batch_job_store;
mod execution_store;
mod schedule_store;
mod workflow_definition_store;
mod workflow_store;

// Production-grade persistence components
mod checkpoint_manager;
mod progress_store;
mod rocks_store;
mod shard_client;

// Persistence abstraction layer (Phase 5)
pub mod persistence;

// Metrics module (Phase 5)
pub mod metrics;

// RocksDB tuning and optimization
pub mod tuning;

pub use approval_store::ApprovalStore;
pub use batch_job_store::BatchJobStore;
pub use execution_store::ExecutionStore;
pub use schedule_store::ScheduleStore;
pub use workflow_definition_store::{restore_persisted_workflows, WorkflowDefinitionStore};
pub use workflow_store::WorkflowStore;

// Export production components
pub use checkpoint_manager::{
    CheckpointConfig, CheckpointManager, RecoveryReport, RecoverySource, RecoveryStats,
};
pub use progress_store::{ProgressStatistics, ProgressStore};
pub use rocks_store::{ExecutionEvent, RocksExecutionStore};
pub use shard_client::WorkflowShardClient;
