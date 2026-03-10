//! Workflow Execution Traits
//!
//! This module defines the core abstractions for workflow execution in Graphica.
//! These traits separate **workflow definitions** (data structures in this crate)
//! from **workflow execution** (runtime behavior in graphica-coordinator).
//!
//! # Architecture
//!
//! - **graphica-core**: Defines traits and data structures (lightweight, minimal deps)
//! - **graphica-coordinator**: Implements traits with production runtime (Timely, RocksDB, Kafka)
//!
//! # Key Traits
//!
//! - [`StepExecutor`]: Execute individual workflow steps
//! - [`LineageCapture`]: Capture lineage events for RDF persistence
//! - [`WorkflowValidator`]: Validate workflow definitions
//! - [`WorkflowRuntime`]: Complete workflow execution runtime
//!
//! # Example
//!
//! ```rust,ignore
//! use graphica_core::orchestration::workflow::traits::*;
//! use graphica_core::orchestration::workflow::*;
//!
//! // graphica-coordinator implements these traits
//! struct StreamingExecutor { /* ... */ }
//!
//! #[async_trait::async_trait]
//! impl StepExecutor for StreamingExecutor {
//!     async fn execute_step(
//!         &self,
//!         step: &WorkflowStep,
//!         context: &ExecutionContext,
//!     ) -> Result<StepResult> {
//!         // Production streaming execution with Timely/Differential
//!         todo!()
//!     }
//! }
//! ```

use super::definition::{WorkflowDefinition, WorkflowStep};
use super::executor::{ExecutionContext, StepResult, WorkflowResult};
use crate::core::lineage::LineageEvent;
use anyhow::Result;
use async_trait::async_trait;

/// Execute individual workflow steps
///
/// This trait abstracts step execution, allowing different implementations:
/// - Mock execution (in graphica-core, for testing)
/// - Streaming execution (in graphica-coordinator, with Timely/Differential)
/// - Batch execution (in graphica-coordinator, parallel processing)
///
/// # Implementation Notes
///
/// Implementations should:
/// - Handle all StepType variants
/// - Provide appropriate error handling
/// - Track execution metrics
/// - Capture lineage when applicable
#[async_trait]
pub trait StepExecutor: Send + Sync {
    /// Execute a single workflow step
    ///
    /// # Arguments
    ///
    /// * `step` - The workflow step definition to execute
    /// * `context` - Execution context with input data and previous step outputs
    ///
    /// # Returns
    ///
    /// Step execution result containing success status, output data, and confidence score
    async fn execute_step(
        &self,
        step: &WorkflowStep,
        context: &ExecutionContext,
    ) -> Result<StepResult>;

    /// Get executor name for logging and debugging
    fn name(&self) -> &str {
        "UnknownExecutor"
    }

    /// Check if executor supports a specific step type
    fn supports_step(&self, step: &WorkflowStep) -> bool {
        // Default: assume executor supports all step types
        // Override for specialized executors
        let _ = step;
        true
    }
}

/// Capture lineage events for RDF persistence
///
/// This trait allows workflow executors to emit lineage events that will be
/// persisted to the RDF knowledge graph for full provenance tracking.
///
/// # Implementation Notes
///
/// - Implementations may buffer events for batch persistence
/// - Events should include all transformation metadata
/// - Failed captures should not fail workflow execution
pub trait LineageCapture: Send + Sync {
    /// Capture a lineage event
    ///
    /// # Arguments
    ///
    /// * `event` - The lineage event to capture
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if capture fails
    fn capture_lineage(&self, event: LineageEvent) -> Result<()>;

    /// Flush any buffered lineage events to persistent storage
    fn flush_lineage(&self) -> Result<()> {
        // Default implementation: no-op for non-buffering implementations
        Ok(())
    }

    /// Get lineage statistics (events captured, buffer size, etc.)
    fn lineage_stats(&self) -> LineageStats {
        LineageStats::default()
    }
}

/// Lineage capture statistics
#[derive(Debug, Default, Clone)]
pub struct LineageStats {
    /// Total lineage events captured
    pub events_captured: u64,
    /// Events currently buffered (not yet persisted)
    pub events_buffered: u64,
    /// Events failed to capture
    pub events_failed: u64,
}

/// Validate workflow definitions
///
/// This trait provides validation logic for workflow definitions, ensuring:
/// - DAG structure (no cycles)
/// - Valid step dependencies
/// - Configuration correctness
/// - Resource availability
pub trait WorkflowValidator: Send + Sync {
    /// Validate a workflow definition
    ///
    /// # Arguments
    ///
    /// * `workflow` - The workflow definition to validate
    ///
    /// # Returns
    ///
    /// Ok(()) if valid, or an error describing validation failures
    fn validate(&self, workflow: &WorkflowDefinition) -> Result<()>;

    /// Check if a specific step configuration is valid
    fn validate_step(&self, step: &WorkflowStep) -> Result<()> {
        step.validate()
    }
}

/// Complete workflow execution runtime
///
/// This trait combines step execution, lineage capture, and validation into
/// a complete runtime for executing workflows end-to-end.
///
/// # Type Parameters
///
/// * `Executor` - The step executor implementation
/// * `Storage` - The state backend for checkpointing (coordinator only)
#[async_trait]
pub trait WorkflowRuntime: Send + Sync {
    /// Execute a complete workflow
    ///
    /// # Arguments
    ///
    /// * `definition` - The workflow definition to execute
    /// * `input` - Input data and execution context
    ///
    /// # Returns
    ///
    /// Complete workflow execution result with all step outputs
    async fn execute_workflow(
        &self,
        definition: WorkflowDefinition,
        input: ExecutionContext,
    ) -> Result<WorkflowResult>;

    /// Get runtime name for logging
    fn runtime_name(&self) -> &str {
        "UnknownRuntime"
    }

    /// Check runtime health and readiness
    fn health_check(&self) -> Result<RuntimeHealth> {
        Ok(RuntimeHealth::Healthy)
    }
}

/// Runtime health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeHealth {
    /// Runtime is healthy and ready to execute workflows
    Healthy,
    /// Runtime is degraded but operational
    Degraded(String),
    /// Runtime is unhealthy and cannot execute workflows
    Unhealthy(String),
}

/// State backend abstraction for checkpointing and recovery
///
/// This trait is implemented by graphica-coordinator for production runtimes
/// with RocksDB-based state management.
pub trait StateBackend: Send + Sync {
    /// Save checkpoint of current execution state
    fn checkpoint(&self, state: &ExecutionState) -> Result<()>;

    /// Restore execution state from latest checkpoint
    fn restore(&self) -> Result<Option<ExecutionState>>;

    /// Clear all checkpoints
    fn clear(&self) -> Result<()>;
}

/// Execution state for checkpointing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionState {
    /// Workflow definition being executed
    pub workflow: WorkflowDefinition,
    /// Current execution context
    pub context: ExecutionContext,
    /// Completed steps
    pub completed_steps: Vec<String>,
    /// Checkpoint timestamp
    pub checkpoint_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_health() {
        assert_eq!(RuntimeHealth::Healthy, RuntimeHealth::Healthy);
        assert_ne!(
            RuntimeHealth::Healthy,
            RuntimeHealth::Degraded("test".to_string())
        );
    }

    #[test]
    fn test_lineage_stats_default() {
        let stats = LineageStats::default();
        assert_eq!(stats.events_captured, 0);
        assert_eq!(stats.events_buffered, 0);
        assert_eq!(stats.events_failed, 0);
    }
}
