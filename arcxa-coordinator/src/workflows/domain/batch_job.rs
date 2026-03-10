//! Batch Job Domain Model
//!
//! Coordinates execution of multiple workflow instances as a single logical unit.
//!
//! ## Use Cases
//!
//! - Import dozens of CSV files to database in parallel
//! - Process multiple data sources with dependencies
//! - Track progress across related workflows
//! - Coordinate retries and error handling
//!
//! ## Architecture
//!
//! ```text
//! BatchJob
//!   ├─ WorkflowExecution 1 (customers.csv)
//!   ├─ WorkflowExecution 2 (orders.csv) [depends on #1]
//!   ├─ WorkflowExecution 3 (products.csv)
//!   └─ WorkflowExecution 4 (inventory.csv) [depends on #3]
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use super::DataSource;

/// Batch job that coordinates multiple workflow executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    /// Unique batch job ID
    pub job_id: String,

    /// Human-readable name
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Workflow template to execute for each file
    pub workflow_id: String,

    /// Current batch job status
    pub status: BatchJobStatus,

    /// Progress tracking
    pub progress: BatchJobProgress,

    /// Batch configuration
    pub config: BatchJobConfig,

    /// Workflow executions in this batch
    pub workflow_executions: Vec<WorkflowExecutionRef>,

    /// Metadata key-value pairs
    pub metadata: HashMap<String, String>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,

    /// Started timestamp
    pub started_at: Option<DateTime<Utc>>,

    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,

    /// User who created the batch job
    pub created_by: String,

    /// Dead letter queue file paths (for failed rows)
    #[serde(default)]
    pub dlq_files: Vec<PathBuf>,

    /// Total rows failed and sent to DLQ
    #[serde(default)]
    pub dlq_row_count: u64,

    /// Transaction summary (populated after completion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_summary: Option<TransactionSummaryInfo>,
}

/// Transaction summary info (simplified for storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSummaryInfo {
    pub total_transactions: usize,
    pub committed: usize,
    pub rolled_back: usize,
}

impl BatchJob {
    /// Create a new batch job
    pub fn new(
        name: String,
        workflow_id: String,
        config: BatchJobConfig,
        created_by: String,
    ) -> Self {
        let now = Utc::now();
        let job_id = format!("batch_{}", Uuid::new_v4());

        Self {
            job_id,
            name,
            description: None,
            workflow_id,
            status: BatchJobStatus::Pending,
            progress: BatchJobProgress::new(),
            config,
            workflow_executions: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            created_by,
            dlq_files: Vec::new(),
            dlq_row_count: 0,
            transaction_summary: None,
        }
    }

    /// Add a workflow execution to the batch
    pub fn add_execution(&mut self, execution: WorkflowExecutionRef) {
        self.workflow_executions.push(execution);
        self.progress.total_files = self.workflow_executions.len();
        self.progress.pending = self.workflow_executions.len();
        self.updated_at = Utc::now();
    }

    /// Update batch job status
    pub fn update_status(&mut self, status: BatchJobStatus) {
        self.status = status;
        self.updated_at = Utc::now();

        // Set timestamps for state transitions
        match status {
            BatchJobStatus::Running | BatchJobStatus::Validating => {
                if self.started_at.is_none() {
                    self.started_at = Some(Utc::now());
                }
            }
            BatchJobStatus::Completed
            | BatchJobStatus::PartiallyCompleted
            | BatchJobStatus::Failed
            | BatchJobStatus::Cancelled => {
                if self.completed_at.is_none() {
                    self.completed_at = Some(Utc::now());
                }
            }
            _ => {}
        }
    }

    /// Recalculate progress from workflow execution states
    pub fn recalculate_progress(&mut self) {
        let mut progress = BatchJobProgress::new();
        progress.total_files = self.workflow_executions.len();

        for exec in &self.workflow_executions {
            match exec.status {
                WorkflowExecutionStatus::Pending => progress.pending += 1,
                WorkflowExecutionStatus::Running => progress.in_progress += 1,
                WorkflowExecutionStatus::Completed => progress.completed += 1,
                WorkflowExecutionStatus::Failed => progress.failed += 1,
                WorkflowExecutionStatus::Retrying => progress.retrying += 1,
                WorkflowExecutionStatus::Cancelled => progress.failed += 1,
            }
        }

        // Calculate percentage (completed + failed = done)
        let done = progress.completed + progress.failed;
        progress.progress_percent = if progress.total_files > 0 {
            (done as f64 / progress.total_files as f64) * 100.0
        } else {
            0.0
        };

        self.progress = progress;
        self.updated_at = Utc::now();
    }

    /// Check if batch job is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            BatchJobStatus::Completed
                | BatchJobStatus::PartiallyCompleted
                | BatchJobStatus::Failed
                | BatchJobStatus::Cancelled
        )
    }

    /// Check if batch job can be cancelled
    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        if let (Some(started), Some(completed)) = (self.started_at, self.completed_at) {
            Some((completed - started).num_milliseconds())
        } else {
            None
        }
    }

    /// Validate batch job configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Batch job name cannot be empty".to_string());
        }

        if self.workflow_id.is_empty() {
            return Err("Workflow ID cannot be empty".to_string());
        }

        if self.workflow_executions.is_empty() {
            return Err("Batch job must have at least one workflow execution".to_string());
        }

        if self.config.max_parallel == 0 {
            return Err("max_parallel must be at least 1".to_string());
        }

        // Validate dependency references
        let execution_ids: std::collections::HashSet<_> = self
            .workflow_executions
            .iter()
            .map(|e| &e.execution_id)
            .collect();

        for exec in &self.workflow_executions {
            for dep in &exec.dependencies {
                if !execution_ids.contains(dep) {
                    return Err(format!(
                        "Workflow {} has invalid dependency: {}",
                        exec.execution_id, dep
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Reference to a workflow execution within a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionRef {
    /// Workflow execution ID
    pub execution_id: String,

    /// Data source for this execution
    pub source: DataSource,

    /// Target table name
    pub target_table: String,

    /// Current status
    pub status: WorkflowExecutionStatus,

    /// Execution IDs this workflow depends on
    pub dependencies: Vec<String>,

    /// Current retry attempt (0 = first attempt)
    pub attempt_number: usize,

    /// Started timestamp
    pub started_at: Option<DateTime<Utc>>,

    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if failed
    pub error: Option<String>,

    /// Number of rows processed (if available)
    pub rows_processed: Option<usize>,

    /// Execution duration in milliseconds
    pub duration_ms: Option<i64>,
}

impl WorkflowExecutionRef {
    /// Create a new workflow execution reference
    pub fn new(source: DataSource, target_table: String) -> Self {
        let execution_id = format!("exec_{}", Uuid::new_v4());

        Self {
            execution_id,
            source,
            target_table,
            status: WorkflowExecutionStatus::Pending,
            dependencies: Vec::new(),
            attempt_number: 0,
            started_at: None,
            completed_at: None,
            error: None,
            rows_processed: None,
            duration_ms: None,
        }
    }

    /// Create from file (backward compatibility helper)
    #[deprecated(since = "0.2.0", note = "Use new() with DataSource instead")]
    pub fn from_file(file_id: String, file_name: String, target_table: String) -> Self {
        let source = DataSource::CsvFile {
            file_id: file_id.clone(),
            file_path: PathBuf::from(&file_name),
            encoding: Some("UTF-8".to_string()),
            delimiter: Some(','),
            has_header: true,
        };
        Self::new(source, target_table)
    }

    /// Get source identifier
    pub fn get_source_identifier(&self) -> String {
        self.source.get_identifier()
    }

    /// Get display name for this execution
    pub fn display_name(&self) -> String {
        format!("{} → {}", self.source.display_name(), self.target_table)
    }

    /// Add a dependency
    pub fn with_dependency(mut self, dependency_id: String) -> Self {
        self.dependencies.push(dependency_id);
        self
    }

    /// Check if dependencies are satisfied
    pub fn dependencies_satisfied(
        &self,
        completed_ids: &std::collections::HashSet<String>,
    ) -> bool {
        self.dependencies
            .iter()
            .all(|dep| completed_ids.contains(dep))
    }
}

/// Workflow execution status within a batch
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    Cancelled,
}

/// Batch job progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobProgress {
    pub total_files: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
    pub retrying: usize,
    pub progress_percent: f64,
}

impl BatchJobProgress {
    pub fn new() -> Self {
        Self {
            total_files: 0,
            pending: 0,
            in_progress: 0,
            completed: 0,
            failed: 0,
            retrying: 0,
            progress_percent: 0.0,
        }
    }
}

impl Default for BatchJobProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch job configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobConfig {
    /// Maximum parallel workflow executions
    pub max_parallel: usize,

    /// Stop entire batch on first error
    pub stop_on_error: bool,

    /// Automatically retry failed workflows
    pub retry_failed: bool,

    /// Maximum retry attempts per workflow
    pub max_retries: usize,

    /// Transaction coordination mode
    pub transaction_mode: TransactionMode,

    /// Resource limits
    pub resource_limits: ResourceLimits,

    /// Timeout for entire batch job (minutes)
    pub timeout_minutes: Option<usize>,

    /// Enable dead letter queue for failed rows
    #[serde(default = "default_enable_dlq")]
    pub enable_dlq: bool,
}

fn default_enable_dlq() -> bool {
    true // Enable DLQ by default
}

impl Default for BatchJobConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            stop_on_error: false,
            retry_failed: true,
            max_retries: 3,
            transaction_mode: TransactionMode::PerFile,
            resource_limits: ResourceLimits::default(),
            timeout_minutes: None,
            enable_dlq: true,
        }
    }
}

/// Transaction coordination mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionMode {
    /// Each file in separate transaction
    PerFile,

    /// All files in single transaction (all-or-nothing)
    AllOrNothing,

    /// Files grouped into transaction batches
    Batched { batch_size: usize },
}

/// Resource limits for batch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage (MB)
    pub max_memory_mb: usize,

    /// Maximum database connections
    pub max_db_connections: usize,

    /// Maximum file size per CSV (MB)
    pub max_file_size_mb: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 2048, // 2GB
            max_db_connections: 10,
            max_file_size_mb: 500, // 500MB per CSV
        }
    }
}

/// Batch job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatchJobStatus {
    /// Batch job created but not started
    Pending,

    /// Running preflight validation
    Validating,

    /// Executing workflows
    Running,

    /// Paused (manual intervention)
    Paused,

    /// All workflows completed successfully
    Completed,

    /// Some workflows completed, some failed
    PartiallyCompleted,

    /// Majority of workflows failed
    Failed,

    /// Batch job cancelled by user
    Cancelled,
}

impl BatchJobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::PartiallyCompleted | Self::Failed | Self::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_job_creation() {
        let config = BatchJobConfig::default();
        let batch = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        assert_eq!(batch.status, BatchJobStatus::Pending);
        assert_eq!(batch.progress.total_files, 0);
        assert!(batch.job_id.starts_with("batch_"));
    }

    #[test]
    fn test_add_execution() {
        let config = BatchJobConfig::default();
        let mut batch = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        let source = DataSource::CsvFile {
            file_id: "file_1".to_string(),
            file_path: PathBuf::from("data.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec = WorkflowExecutionRef::new(source, "target_table".to_string());
        batch.add_execution(exec);

        assert_eq!(batch.workflow_executions.len(), 1);
        assert_eq!(batch.progress.total_files, 1);
        assert_eq!(batch.progress.pending, 1);
    }

    #[test]
    fn test_progress_calculation() {
        let config = BatchJobConfig::default();
        let mut batch = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        // Add 4 executions
        for i in 1..=4 {
            let source = DataSource::CsvFile {
                file_id: format!("file_{}", i),
                file_path: PathBuf::from(format!("data{}.csv", i)),
                encoding: None,
                delimiter: None,
                has_header: true,
            };
            let exec = WorkflowExecutionRef::new(source, format!("table_{}", i));
            batch.add_execution(exec);
        }

        // Mark some as completed
        batch.workflow_executions[0].status = WorkflowExecutionStatus::Completed;
        batch.workflow_executions[1].status = WorkflowExecutionStatus::Completed;
        batch.workflow_executions[2].status = WorkflowExecutionStatus::Failed;
        batch.workflow_executions[3].status = WorkflowExecutionStatus::Running;

        batch.recalculate_progress();

        assert_eq!(batch.progress.completed, 2);
        assert_eq!(batch.progress.failed, 1);
        assert_eq!(batch.progress.in_progress, 1);
        assert_eq!(batch.progress.progress_percent, 75.0); // 3/4 done
    }

    #[test]
    fn test_dependency_validation() {
        let config = BatchJobConfig::default();
        let mut batch = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        let source1 = DataSource::CsvFile {
            file_id: "file_1".to_string(),
            file_path: PathBuf::from("customers.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec1 = WorkflowExecutionRef::new(source1, "customers".to_string());

        let source2 = DataSource::CsvFile {
            file_id: "file_2".to_string(),
            file_path: PathBuf::from("orders.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec2 = WorkflowExecutionRef::new(source2, "orders".to_string())
            .with_dependency(exec1.execution_id.clone());

        batch.add_execution(exec1);
        batch.add_execution(exec2);

        // Should pass validation (dependency exists)
        assert!(batch.validate().is_ok());

        // Add invalid dependency
        let source3 = DataSource::CsvFile {
            file_id: "file_3".to_string(),
            file_path: PathBuf::from("products.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec3 = WorkflowExecutionRef::new(source3, "products".to_string())
            .with_dependency("nonexistent_id".to_string());
        batch.add_execution(exec3);

        // Should fail validation (dependency doesn't exist)
        assert!(batch.validate().is_err());
    }

    #[test]
    fn test_dependencies_satisfied() {
        let source1 = DataSource::CsvFile {
            file_id: "file_1".to_string(),
            file_path: PathBuf::from("data1.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec1 = WorkflowExecutionRef::new(source1, "table1".to_string());

        let source2 = DataSource::CsvFile {
            file_id: "file_2".to_string(),
            file_path: PathBuf::from("data2.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        let exec2 = WorkflowExecutionRef::new(source2, "table2".to_string())
            .with_dependency(exec1.execution_id.clone());

        let mut completed_ids = std::collections::HashSet::new();

        // Dependencies not satisfied yet
        assert!(!exec2.dependencies_satisfied(&completed_ids));

        // Mark dependency as completed
        completed_ids.insert(exec1.execution_id.clone());

        // Now dependencies are satisfied
        assert!(exec2.dependencies_satisfied(&completed_ids));
    }
}
