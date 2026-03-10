//! Workflow Execution Domain Model
//!
//! Tracks workflow execution state, history, and logs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

/// Persisted result for a graph-native workflow step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedStepResult {
    pub step_id: String,
    pub success: bool,
    pub output: JsonValue,
    pub confidence: f64,
    pub duration_ms: u64,
}

/// Workflow execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecution {
    /// Unique execution ID
    pub execution_id: String,

    /// Workflow that was executed
    pub workflow_id: String,

    /// Workflow name (cached for display)
    pub workflow_name: String,

    /// Current execution status
    pub status: ExecutionStatus,

    /// Input data provided to workflow
    pub input: JsonValue,

    /// Output data (if completed)
    pub output: Option<JsonValue>,

    /// Final workflow confidence (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    /// Per-step results for graph-native workflow executions.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub step_results: Vec<PersistedStepResult>,

    /// Matched route ID (if any)
    pub matched_route: Option<String>,

    /// Matched route name (cached for display)
    pub matched_route_name: Option<String>,

    /// When execution started
    pub started_at: DateTime<Utc>,

    /// Last status update time
    pub updated_at: DateTime<Utc>,

    /// When execution completed/failed/stopped
    pub completed_at: Option<DateTime<Utc>>,

    /// Total execution duration in milliseconds
    pub duration_ms: Option<u64>,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Execution logs
    pub logs: Vec<ExecutionLog>,

    /// Current step being executed (for pause/resume)
    pub current_step: Option<String>,

    /// Total number of actions executed
    pub actions_executed: usize,

    /// User who triggered the execution
    pub triggered_by: Option<String>,

    /// Checkpoint: Index of action where execution paused (for resume)
    ///
    /// When a workflow pauses (e.g., waiting for approval), this stores the index
    /// of the action that caused the pause. This allows the workflow to resume
    /// from the exact point where it left off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_action_index: Option<usize>,

    /// Checkpoint: Intermediate data at pause point (for resume)
    ///
    /// When a workflow pauses, this stores any intermediate data needed to resume
    /// execution. For example, transformed data, computed values, or approval context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intermediate_data: Option<JsonValue>,

    /// Per-action execution results with timing information
    ///
    /// Stores the result of each action that was executed, including:
    /// - Action type (Transform, Validate, etc.)
    /// - Execution status (Success, Failed, Skipped, Paused)
    /// - Duration in milliseconds
    /// - Error message (if failed)
    /// - Output data (if any)
    ///
    /// This allows detailed analysis of workflow performance and debugging.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub action_results: Vec<super::ActionResult>,
}

impl WorkflowExecution {
    /// Create a new execution record
    pub fn new(
        execution_id: String,
        workflow_id: String,
        workflow_name: String,
        input: JsonValue,
        triggered_by: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            execution_id,
            workflow_id,
            workflow_name,
            status: ExecutionStatus::Pending,
            input,
            output: None,
            confidence: None,
            step_results: Vec::new(),
            matched_route: None,
            matched_route_name: None,
            started_at: now,
            updated_at: now,
            completed_at: None,
            duration_ms: None,
            error: None,
            logs: Vec::new(),
            current_step: None,
            actions_executed: 0,
            triggered_by,
            checkpoint_action_index: None,
            intermediate_data: None,
            action_results: Vec::new(),
        }
    }

    /// Update execution status
    pub fn update_status(&mut self, status: ExecutionStatus) {
        self.status = status;
        self.updated_at = Utc::now();

        // Set completion time for terminal states
        if status.is_terminal() && self.completed_at.is_none() {
            self.completed_at = Some(Utc::now());
            self.duration_ms =
                Some((self.completed_at.unwrap() - self.started_at).num_milliseconds() as u64);
        }
    }

    /// Add a log entry
    pub fn add_log(&mut self, log: ExecutionLog) {
        self.logs.push(log);
        self.updated_at = Utc::now();
    }

    /// Set matched route
    pub fn set_matched_route(&mut self, route_id: String, route_name: String) {
        self.matched_route = Some(route_id);
        self.matched_route_name = Some(route_name);
    }

    /// Set output data
    pub fn set_output(&mut self, output: JsonValue) {
        self.output = Some(output);
    }

    /// Set error
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.update_status(ExecutionStatus::Failed);
    }

    /// Check if execution can be stopped
    pub fn can_stop(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Pending | ExecutionStatus::Running | ExecutionStatus::Paused
        )
    }

    /// Check if execution can be paused
    pub fn can_pause(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Running | ExecutionStatus::Pending
        )
    }

    /// Check if execution can be resumed
    pub fn can_resume(&self) -> bool {
        self.status == ExecutionStatus::Paused
    }

    /// Save checkpoint for pause/resume
    ///
    /// Records the action index and intermediate data at the point where execution
    /// paused. This allows the workflow to resume from the exact point it left off.
    ///
    /// # Parameters
    /// - `action_index`: Index of the action that caused the pause (e.g., WaitForApproval)
    /// - `data`: Intermediate data to preserve (transformed data, context, etc.)
    pub fn checkpoint(&mut self, action_index: usize, data: JsonValue) {
        self.checkpoint_action_index = Some(action_index);
        self.intermediate_data = Some(data);
        self.updated_at = Utc::now();
    }

    /// Check if execution has a checkpoint
    ///
    /// Returns true if checkpoint data is present (action index and/or intermediate data).
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint_action_index.is_some()
    }

    /// Clear checkpoint data
    ///
    /// Removes checkpoint information, typically called after successful resume
    /// or when execution completes/fails.
    pub fn clear_checkpoint(&mut self) {
        self.checkpoint_action_index = None;
        self.intermediate_data = None;
        self.updated_at = Utc::now();
    }

    /// Get checkpoint action index
    ///
    /// Returns the action index where execution paused, if available.
    pub fn checkpoint_action_index(&self) -> Option<usize> {
        self.checkpoint_action_index
    }

    /// Get intermediate data from checkpoint
    ///
    /// Returns the intermediate data saved at checkpoint, if available.
    pub fn checkpoint_data(&self) -> Option<&JsonValue> {
        self.intermediate_data.as_ref()
    }
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    /// Execution created but not started
    Pending,

    /// Currently executing
    Running,

    /// Temporarily paused
    Paused,

    /// Completed successfully
    Completed,

    /// Failed with error
    Failed,

    /// Stopped by user
    Stopped,

    /// Force-killed/aborted
    Aborted,
}

impl ExecutionStatus {
    /// Check if this is a terminal state (cannot transition further)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Completed
                | ExecutionStatus::Failed
                | ExecutionStatus::Stopped
                | ExecutionStatus::Aborted
        )
    }

    /// Check if this is an error state
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Failed | ExecutionStatus::Stopped | ExecutionStatus::Aborted
        )
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Stopped => write!(f, "stopped"),
            Self::Aborted => write!(f, "aborted"),
        }
    }
}

/// Execution log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    /// When the log was created
    pub timestamp: DateTime<Utc>,

    /// Log level
    pub level: LogLevel,

    /// Step ID (if associated with a specific action)
    pub step_id: Option<String>,

    /// Log message
    pub message: String,

    /// Additional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
}

impl ExecutionLog {
    /// Create a new log entry
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            step_id: None,
            message: message.into(),
            details: None,
        }
    }

    /// Create log with step ID
    pub fn with_step(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }

    /// Create log with details
    pub fn with_details(mut self, details: JsonValue) -> Self {
        self.details = Some(details);
        self
    }

    /// Convenience: Create INFO log
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    /// Convenience: Create WARNING log
    pub fn warn(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warning, message)
    }

    /// Convenience: Create ERROR log
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }

    /// Convenience: Create DEBUG log
    pub fn debug(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Debug, message)
    }
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// Filters for querying executions
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilters {
    /// Filter by workflow ID
    pub workflow_id: Option<String>,

    /// Filter by status
    pub status: Option<ExecutionStatus>,

    /// Filter by start date (inclusive)
    pub start_date: Option<DateTime<Utc>>,

    /// Filter by end date (inclusive)
    pub end_date: Option<DateTime<Utc>>,

    /// Search in workflow name, execution ID, or error message
    pub search: Option<String>,
}

impl ExecutionFilters {
    /// Check if an execution matches these filters
    pub fn matches(&self, execution: &WorkflowExecution) -> bool {
        // Workflow ID filter
        if let Some(ref workflow_id) = self.workflow_id {
            if execution.workflow_id != *workflow_id {
                return false;
            }
        }

        // Status filter
        if let Some(status) = self.status {
            if execution.status != status {
                return false;
            }
        }

        // Date range filter
        if let Some(start) = self.start_date {
            if execution.started_at < start {
                return false;
            }
        }

        if let Some(end) = self.end_date {
            if execution.started_at > end {
                return false;
            }
        }

        // Search filter
        if let Some(ref search) = self.search {
            let search_lower = search.to_lowercase();
            let matches_id = execution
                .execution_id
                .to_lowercase()
                .contains(&search_lower);
            let matches_workflow = execution
                .workflow_name
                .to_lowercase()
                .contains(&search_lower);
            let matches_error = execution
                .error
                .as_ref()
                .map(|e| e.to_lowercase().contains(&search_lower))
                .unwrap_or(false);

            if !matches_id && !matches_workflow && !matches_error {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_execution() {
        let execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test Workflow".to_string(),
            json!({"test": "data"}),
            Some("user@example.com".to_string()),
        );

        assert_eq!(execution.execution_id, "exec_123");
        assert_eq!(execution.status, ExecutionStatus::Pending);
        assert_eq!(execution.logs.len(), 0);
        assert!(execution.output.is_none());
    }

    #[test]
    fn test_status_transitions() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        assert_eq!(execution.status, ExecutionStatus::Pending);
        assert!(!execution.status.is_terminal());

        execution.update_status(ExecutionStatus::Running);
        assert_eq!(execution.status, ExecutionStatus::Running);
        assert!(execution.completed_at.is_none());

        execution.update_status(ExecutionStatus::Completed);
        assert_eq!(execution.status, ExecutionStatus::Completed);
        assert!(execution.status.is_terminal());
        assert!(execution.completed_at.is_some());
        assert!(execution.duration_ms.is_some());
    }

    #[test]
    fn test_can_stop_pause_resume() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // Pending can be stopped or paused (paused before starting)
        assert!(execution.can_stop());
        assert!(execution.can_pause());
        assert!(!execution.can_resume());

        // Running can be stopped or paused
        execution.update_status(ExecutionStatus::Running);
        assert!(execution.can_stop());
        assert!(execution.can_pause());
        assert!(!execution.can_resume());

        // Paused can be stopped or resumed
        execution.update_status(ExecutionStatus::Paused);
        assert!(execution.can_stop());
        assert!(!execution.can_pause());
        assert!(execution.can_resume());

        // Completed cannot be controlled
        execution.update_status(ExecutionStatus::Completed);
        assert!(!execution.can_stop());
        assert!(!execution.can_pause());
        assert!(!execution.can_resume());
    }

    #[test]
    fn test_add_logs() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        let log1 = ExecutionLog::info("Starting execution");
        let log2 = ExecutionLog::error("Something failed").with_details(json!({"code": 500}));

        execution.add_log(log1);
        execution.add_log(log2);

        assert_eq!(execution.logs.len(), 2);
        assert_eq!(execution.logs[0].level, LogLevel::Info);
        assert_eq!(execution.logs[1].level, LogLevel::Error);
        assert!(execution.logs[1].details.is_some());
    }

    #[test]
    fn test_execution_filters() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test Workflow".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);

        // Status filter
        let filter = ExecutionFilters {
            status: Some(ExecutionStatus::Running),
            ..Default::default()
        };
        assert!(filter.matches(&execution));

        let filter = ExecutionFilters {
            status: Some(ExecutionStatus::Failed),
            ..Default::default()
        };
        assert!(!filter.matches(&execution));

        // Search filter
        let filter = ExecutionFilters {
            search: Some("Test".to_string()),
            ..Default::default()
        };
        assert!(filter.matches(&execution));

        let filter = ExecutionFilters {
            search: Some("NonExistent".to_string()),
            ..Default::default()
        };
        assert!(!filter.matches(&execution));
    }

    #[test]
    fn test_execution_status_from_str() {
        assert_eq!(
            ExecutionStatus::from_str("running"),
            Some(ExecutionStatus::Running)
        );
        assert_eq!(
            ExecutionStatus::from_str("COMPLETED"),
            Some(ExecutionStatus::Completed)
        );
        assert_eq!(ExecutionStatus::from_str("invalid"), None);
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[test]
    fn test_status_is_error() {
        assert!(!ExecutionStatus::Pending.is_error());
        assert!(!ExecutionStatus::Running.is_error());
        assert!(!ExecutionStatus::Completed.is_error());
        assert!(ExecutionStatus::Failed.is_error());
        assert!(ExecutionStatus::Stopped.is_error());
        assert!(ExecutionStatus::Aborted.is_error());
    }

    #[test]
    fn test_checkpoint_basic() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // Initially no checkpoint
        assert!(!execution.has_checkpoint());
        assert_eq!(execution.checkpoint_action_index(), None);
        assert_eq!(execution.checkpoint_data(), None);

        // Set checkpoint
        let checkpoint_data = json!({"transformed_data": "value", "step": 5});
        execution.checkpoint(3, checkpoint_data.clone());

        // Verify checkpoint saved
        assert!(execution.has_checkpoint());
        assert_eq!(execution.checkpoint_action_index(), Some(3));
        assert_eq!(execution.checkpoint_data(), Some(&checkpoint_data));
    }

    #[test]
    fn test_checkpoint_clear() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // Set checkpoint
        execution.checkpoint(2, json!({"data": "test"}));
        assert!(execution.has_checkpoint());

        // Clear checkpoint
        execution.clear_checkpoint();

        // Verify cleared
        assert!(!execution.has_checkpoint());
        assert_eq!(execution.checkpoint_action_index(), None);
        assert_eq!(execution.checkpoint_data(), None);
    }

    #[test]
    fn test_checkpoint_updates_timestamp() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        let initial_updated_at = execution.updated_at;

        // Wait a small amount to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        execution.checkpoint(1, json!({"test": "data"}));

        // Timestamp should be updated
        assert!(execution.updated_at > initial_updated_at);
    }

    #[test]
    fn test_checkpoint_overwrite() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // First checkpoint
        execution.checkpoint(1, json!({"step": 1}));
        assert_eq!(execution.checkpoint_action_index(), Some(1));

        // Overwrite with second checkpoint
        execution.checkpoint(5, json!({"step": 5}));
        assert_eq!(execution.checkpoint_action_index(), Some(5));
        assert_eq!(execution.checkpoint_data(), Some(&json!({"step": 5})));
    }

    #[test]
    fn test_checkpoint_serialization() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // Set checkpoint
        execution.checkpoint(2, json!({"key": "value"}));

        // Serialize to JSON
        let json_str = serde_json::to_string(&execution).unwrap();

        // Should contain checkpoint fields
        assert!(json_str.contains("checkpoint_action_index"));
        assert!(json_str.contains("intermediate_data"));

        // Deserialize back
        let deserialized: WorkflowExecution = serde_json::from_str(&json_str).unwrap();

        // Verify checkpoint preserved
        assert_eq!(deserialized.checkpoint_action_index(), Some(2));
        assert_eq!(
            deserialized.checkpoint_data(),
            Some(&json!({"key": "value"}))
        );
    }

    #[test]
    fn test_checkpoint_serialization_without_checkpoint() {
        let execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        // Serialize to JSON
        let json_str = serde_json::to_string(&execution).unwrap();

        // Should NOT contain checkpoint fields (skip_serializing_if = None)
        assert!(!json_str.contains("checkpoint_action_index"));
        assert!(!json_str.contains("intermediate_data"));

        // Deserialize back - should still work
        let deserialized: WorkflowExecution = serde_json::from_str(&json_str).unwrap();
        assert!(!deserialized.has_checkpoint());
    }

    #[test]
    fn test_checkpoint_with_pause_resume_workflow() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test Workflow".to_string(),
            json!({"input": "data"}),
            Some("user@test.com".to_string()),
        );

        // Start execution
        execution.update_status(ExecutionStatus::Running);
        assert!(!execution.has_checkpoint());

        // Simulate pausing at action 3 (e.g., WaitForApproval)
        execution.checkpoint(3, json!({"approval_context": "DDL execution"}));
        execution.update_status(ExecutionStatus::Paused);

        // Verify state
        assert!(execution.has_checkpoint());
        assert_eq!(execution.status, ExecutionStatus::Paused);
        assert!(execution.can_resume());

        // Simulate resume
        assert_eq!(execution.checkpoint_action_index(), Some(3));
        let checkpoint_data = execution.checkpoint_data().unwrap().clone();
        assert_eq!(
            checkpoint_data,
            json!({"approval_context": "DDL execution"})
        );

        // After resume, clear checkpoint
        execution.clear_checkpoint();
        execution.update_status(ExecutionStatus::Running);

        assert!(!execution.has_checkpoint());
        assert_eq!(execution.status, ExecutionStatus::Running);
    }

    #[test]
    fn test_checkpoint_data_immutability() {
        let mut execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_456".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );

        let original_data = json!({"value": 100});
        execution.checkpoint(1, original_data.clone());

        // Get reference to checkpoint data
        let data_ref = execution.checkpoint_data().unwrap();
        assert_eq!(data_ref, &original_data);

        // Checkpoint data should be independent
        let new_data = json!({"value": 200});
        execution.checkpoint(1, new_data.clone());

        // Old reference should not be affected (new checkpoint created)
        let new_data_ref = execution.checkpoint_data().unwrap();
        assert_eq!(new_data_ref, &new_data);
        assert_ne!(new_data_ref, &original_data);
    }
}
