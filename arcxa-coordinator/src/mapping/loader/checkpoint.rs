//! Checkpoint and Recovery System
//!
//! Provides checkpoint/restart capability for ETL operations to ensure fault tolerance.
//!
//! ## Features
//!
//! - **Checkpoint Management**: Save progress at regular intervals
//! - **Crash Recovery**: Resume from last successful checkpoint
//! - **Error Tracking**: Record and categorize errors during processing
//! - **Batch Coordination**: Track batch boundaries for transactional loads
//! - **Progress Persistence**: Survive process crashes and restarts
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::checkpoint::{CheckpointManager, CheckpointConfig};
//!
//! let config = CheckpointConfig {
//!     checkpoint_dir: PathBuf::from("/var/lib/graphica/checkpoints"),
//!     checkpoint_interval_rows: 10000,
//!     max_retries: 3,
//!     ..Default::default()
//! };
//!
//! let mut manager = CheckpointManager::new("load_123", config)?;
//!
//! // Process rows with automatic checkpointing
//! for (row_num, row) in csv_reader.enumerate() {
//!     match process_row(&row) {
//!         Ok(_) => {
//!             manager.record_success(row_num as u64)?;
//!         }
//!         Err(e) => {
//!             manager.record_error(row_num as u64, e)?;
//!         }
//!     }
//!
//!     // Checkpoint every N rows
//!     if manager.should_checkpoint() {
//!         manager.checkpoint()?;
//!     }
//! }
//!
//! // Final checkpoint
//! manager.finalize()?;
//! ```

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Directory to store checkpoint files
    pub checkpoint_dir: PathBuf,

    /// Checkpoint after this many rows
    pub checkpoint_interval_rows: u64,

    /// Checkpoint after this duration
    pub checkpoint_interval_duration: Duration,

    /// Maximum retries for transient errors
    pub max_retries: usize,

    /// Backoff multiplier for retries
    pub retry_backoff_multiplier: f64,

    /// Initial retry delay
    pub initial_retry_delay: Duration,

    /// Maximum errors before aborting
    pub max_errors: usize,

    /// Whether to automatically resume on startup
    pub auto_resume: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            checkpoint_dir: PathBuf::from("/tmp/graphica/checkpoints"),
            checkpoint_interval_rows: 10000,
            checkpoint_interval_duration: Duration::from_secs(300), // 5 minutes
            max_retries: 3,
            retry_backoff_multiplier: 2.0,
            initial_retry_delay: Duration::from_millis(100),
            max_errors: 100,
            auto_resume: true,
        }
    }
}

/// Checkpoint data persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Load job ID
    pub job_id: String,

    /// Current row number being processed
    pub current_row: u64,

    /// Current file byte offset (for CSV reading)
    pub file_offset: u64,

    /// Current batch ID
    pub batch_id: usize,

    /// Total rows processed successfully
    pub rows_processed: u64,

    /// Total rows failed
    pub rows_failed: u64,

    /// Total rows skipped
    pub rows_skipped: u64,

    /// Timestamp of checkpoint
    pub timestamp: DateTime<Utc>,

    /// Load state
    pub state: LoadState,

    /// Error summary
    pub error_summary: ErrorSummary,

    /// Batch progress
    pub batch_progress: Vec<BatchProgress>,
}

/// Load state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadState {
    /// Not started
    NotStarted,

    /// Currently running
    Running,

    /// Completed successfully
    Completed,

    /// Failed (unrecoverable error)
    Failed,

    /// Aborted by user
    Aborted,

    /// Paused at checkpoint
    Paused,
}

/// Batch progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// Batch ID
    pub batch_id: usize,

    /// Start row (inclusive)
    pub start_row: u64,

    /// End row (inclusive)
    pub end_row: u64,

    /// Rows loaded in this batch
    pub rows_loaded: u64,

    /// Batch state
    pub state: BatchState,

    /// Start time
    pub start_time: DateTime<Utc>,

    /// End time (if complete)
    pub end_time: Option<DateTime<Utc>>,
}

/// Batch state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchState {
    /// Batch is being processed
    InProgress,

    /// Batch completed successfully
    Completed,

    /// Batch failed
    Failed,
}

/// Error summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    /// Total errors encountered
    pub total_errors: usize,

    /// Errors by category
    pub errors_by_category: HashMap<String, usize>,

    /// Recent errors (last N)
    pub recent_errors: Vec<ErrorRecord>,
}

impl Default for ErrorSummary {
    fn default() -> Self {
        Self {
            total_errors: 0,
            errors_by_category: HashMap::new(),
            recent_errors: Vec::new(),
        }
    }
}

/// Error record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    /// Row number where error occurred
    pub row_number: u64,

    /// Error category
    pub category: ErrorCategory,

    /// Error message
    pub message: String,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Number of retry attempts
    pub retry_count: usize,

    /// Whether error is transient (retriable)
    pub is_transient: bool,
}

/// Error category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Database connection error
    DatabaseConnection,

    /// Database constraint violation
    DatabaseConstraint,

    /// Data format/parsing error
    DataFormat,

    /// Transformation error
    Transformation,

    /// I/O error (file read/write)
    IO,

    /// Timeout
    Timeout,

    /// Resource exhaustion
    ResourceExhaustion,

    /// Unknown error
    Unknown,
}

impl ErrorCategory {
    /// Check if error category is transient (retriable)
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ErrorCategory::DatabaseConnection
                | ErrorCategory::Timeout
                | ErrorCategory::ResourceExhaustion
        )
    }

    /// Get error category from error message
    pub fn from_error_message(msg: &str) -> Self {
        let msg_lower = msg.to_lowercase();

        if msg_lower.contains("connection") || msg_lower.contains("connect") {
            ErrorCategory::DatabaseConnection
        } else if msg_lower.contains("constraint")
            || msg_lower.contains("duplicate")
            || msg_lower.contains("foreign key")
        {
            ErrorCategory::DatabaseConstraint
        } else if msg_lower.contains("parse") || msg_lower.contains("format") {
            ErrorCategory::DataFormat
        } else if msg_lower.contains("timeout") {
            ErrorCategory::Timeout
        } else if msg_lower.contains("transform") {
            ErrorCategory::Transformation
        } else if msg_lower.contains("io") || msg_lower.contains("file") {
            ErrorCategory::IO
        } else {
            ErrorCategory::Unknown
        }
    }
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::DatabaseConnection => write!(f, "DatabaseConnection"),
            ErrorCategory::DatabaseConstraint => write!(f, "DatabaseConstraint"),
            ErrorCategory::DataFormat => write!(f, "DataFormat"),
            ErrorCategory::Transformation => write!(f, "Transformation"),
            ErrorCategory::IO => write!(f, "IO"),
            ErrorCategory::Timeout => write!(f, "Timeout"),
            ErrorCategory::ResourceExhaustion => write!(f, "ResourceExhaustion"),
            ErrorCategory::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Checkpoint manager
pub struct CheckpointManager {
    /// Job ID
    job_id: String,

    /// Configuration
    config: CheckpointConfig,

    /// Current checkpoint
    checkpoint: Checkpoint,

    /// Last checkpoint time
    last_checkpoint_time: Instant,

    /// Rows since last checkpoint
    rows_since_checkpoint: u64,

    /// Current batch
    current_batch: Option<BatchProgress>,
}

impl CheckpointManager {
    /// Create new checkpoint manager
    pub fn new(job_id: impl Into<String>, config: CheckpointConfig) -> Result<Self> {
        let job_id = job_id.into();

        // Create checkpoint directory if it doesn't exist
        fs::create_dir_all(&config.checkpoint_dir).with_context(|| {
            format!(
                "Failed to create checkpoint directory: {:?}",
                config.checkpoint_dir
            )
        })?;

        let checkpoint = Checkpoint {
            job_id: job_id.clone(),
            current_row: 0,
            file_offset: 0,
            batch_id: 0,
            rows_processed: 0,
            rows_failed: 0,
            rows_skipped: 0,
            timestamp: Utc::now(),
            state: LoadState::NotStarted,
            error_summary: ErrorSummary::default(),
            batch_progress: Vec::new(),
        };

        Ok(Self {
            job_id,
            config,
            checkpoint,
            last_checkpoint_time: Instant::now(),
            rows_since_checkpoint: 0,
            current_batch: None,
        })
    }

    /// Resume from existing checkpoint
    pub fn resume(job_id: impl Into<String>, config: CheckpointConfig) -> Result<Self> {
        let job_id = job_id.into();
        let checkpoint_path = Self::checkpoint_path(&config.checkpoint_dir, &job_id);

        if !checkpoint_path.exists() {
            return Err(anyhow!("No checkpoint found for job: {}", job_id));
        }

        let checkpoint = Self::load_checkpoint(&checkpoint_path)?;

        if checkpoint.state == LoadState::Completed {
            return Err(anyhow!("Job already completed: {}", job_id));
        }

        Ok(Self {
            job_id,
            config,
            checkpoint,
            last_checkpoint_time: Instant::now(),
            rows_since_checkpoint: 0,
            current_batch: None,
        })
    }

    /// Try to resume, or create new if no checkpoint exists
    pub fn resume_or_create(job_id: impl Into<String>, config: CheckpointConfig) -> Result<Self> {
        let job_id = job_id.into();
        let checkpoint_path = Self::checkpoint_path(&config.checkpoint_dir, &job_id);

        if checkpoint_path.exists() && config.auto_resume {
            Self::resume(job_id, config)
        } else {
            Self::new(job_id, config)
        }
    }

    /// Start a new batch
    pub fn start_batch(&mut self, start_row: u64, batch_size: u64) {
        self.checkpoint.batch_id += 1;
        self.current_batch = Some(BatchProgress {
            batch_id: self.checkpoint.batch_id,
            start_row,
            end_row: start_row + batch_size - 1,
            rows_loaded: 0,
            state: BatchState::InProgress,
            start_time: Utc::now(),
            end_time: None,
        });
    }

    /// Complete current batch
    pub fn complete_batch(&mut self, rows_loaded: u64) -> Result<()> {
        if let Some(mut batch) = self.current_batch.take() {
            batch.rows_loaded = rows_loaded;
            batch.state = BatchState::Completed;
            batch.end_time = Some(Utc::now());
            self.checkpoint.batch_progress.push(batch);
        }
        Ok(())
    }

    /// Fail current batch
    pub fn fail_batch(&mut self) -> Result<()> {
        if let Some(mut batch) = self.current_batch.take() {
            batch.state = BatchState::Failed;
            batch.end_time = Some(Utc::now());
            self.checkpoint.batch_progress.push(batch);
        }
        Ok(())
    }

    /// Record successful row processing
    pub fn record_success(&mut self, row_number: u64) -> Result<()> {
        self.checkpoint.current_row = row_number;
        self.checkpoint.rows_processed += 1;
        self.rows_since_checkpoint += 1;

        if let Some(batch) = &mut self.current_batch {
            batch.rows_loaded += 1;
        }

        Ok(())
    }

    /// Record error during row processing
    pub fn record_error(&mut self, row_number: u64, error: anyhow::Error) -> Result<()> {
        let message = error.to_string();
        let category = ErrorCategory::from_error_message(&message);

        let error_record = ErrorRecord {
            row_number,
            category,
            message: message.clone(),
            timestamp: Utc::now(),
            retry_count: 0,
            is_transient: category.is_transient(),
        };

        // Update error summary
        self.checkpoint.error_summary.total_errors += 1;
        *self
            .checkpoint
            .error_summary
            .errors_by_category
            .entry(category.to_string())
            .or_insert(0) += 1;

        // Keep last 100 errors
        self.checkpoint
            .error_summary
            .recent_errors
            .push(error_record);
        if self.checkpoint.error_summary.recent_errors.len() > 100 {
            self.checkpoint.error_summary.recent_errors.remove(0);
        }

        self.checkpoint.rows_failed += 1;

        // Check if we've exceeded max errors
        if self.checkpoint.error_summary.total_errors >= self.config.max_errors {
            return Err(anyhow!(
                "Maximum error threshold exceeded: {} errors",
                self.checkpoint.error_summary.total_errors
            ));
        }

        Ok(())
    }

    /// Record skipped row
    pub fn record_skip(&mut self, _row_number: u64) -> Result<()> {
        self.checkpoint.rows_skipped += 1;
        Ok(())
    }

    /// Update file offset
    pub fn update_file_offset(&mut self, offset: u64) {
        self.checkpoint.file_offset = offset;
    }

    /// Check if checkpoint should be saved
    pub fn should_checkpoint(&self) -> bool {
        // Checkpoint by row count
        if self.rows_since_checkpoint >= self.config.checkpoint_interval_rows {
            return true;
        }

        // Checkpoint by time
        if self.last_checkpoint_time.elapsed() >= self.config.checkpoint_interval_duration {
            return true;
        }

        false
    }

    /// Save checkpoint
    pub fn checkpoint(&mut self) -> Result<()> {
        self.checkpoint.timestamp = Utc::now();
        self.checkpoint.state = LoadState::Paused;

        let checkpoint_path = Self::checkpoint_path(&self.config.checkpoint_dir, &self.job_id);
        Self::save_checkpoint(&self.checkpoint, &checkpoint_path)?;

        self.last_checkpoint_time = Instant::now();
        self.rows_since_checkpoint = 0;

        Ok(())
    }

    /// Finalize load (mark as completed)
    pub fn finalize(&mut self) -> Result<()> {
        self.checkpoint.state = LoadState::Completed;
        self.checkpoint.timestamp = Utc::now();

        let checkpoint_path = Self::checkpoint_path(&self.config.checkpoint_dir, &self.job_id);
        Self::save_checkpoint(&self.checkpoint, &checkpoint_path)?;

        Ok(())
    }

    /// Mark load as failed
    pub fn mark_failed(&mut self, error: &str) -> Result<()> {
        self.checkpoint.state = LoadState::Failed;
        self.checkpoint.timestamp = Utc::now();

        // Add failure to error summary
        self.checkpoint
            .error_summary
            .recent_errors
            .push(ErrorRecord {
                row_number: self.checkpoint.current_row,
                category: ErrorCategory::Unknown,
                message: error.to_string(),
                timestamp: Utc::now(),
                retry_count: 0,
                is_transient: false,
            });

        let checkpoint_path = Self::checkpoint_path(&self.config.checkpoint_dir, &self.job_id);
        Self::save_checkpoint(&self.checkpoint, &checkpoint_path)?;

        Ok(())
    }

    /// Get current checkpoint (read-only)
    pub fn current_checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// Get job ID
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    /// Get starting row (for resume)
    pub fn starting_row(&self) -> u64 {
        self.checkpoint.current_row
    }

    /// Get starting file offset (for resume)
    pub fn starting_file_offset(&self) -> u64 {
        self.checkpoint.file_offset
    }

    /// Calculate retry delay for transient error
    pub fn calculate_retry_delay(&self, retry_count: usize) -> Duration {
        let multiplier = self
            .config
            .retry_backoff_multiplier
            .powi(retry_count as i32);
        Duration::from_millis(
            (self.config.initial_retry_delay.as_millis() as f64 * multiplier) as u64,
        )
    }

    /// Check if error should be retried
    pub fn should_retry(&self, category: ErrorCategory, retry_count: usize) -> bool {
        category.is_transient() && retry_count < self.config.max_retries
    }

    /// Get checkpoint file path
    fn checkpoint_path(checkpoint_dir: &Path, job_id: &str) -> PathBuf {
        checkpoint_dir.join(format!("{}.checkpoint.json", job_id))
    }

    /// Load checkpoint from file
    fn load_checkpoint(path: &Path) -> Result<Checkpoint> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open checkpoint file: {:?}", path))?;
        let reader = BufReader::new(file);
        let checkpoint: Checkpoint = serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse checkpoint file: {:?}", path))?;
        Ok(checkpoint)
    }

    /// Save checkpoint to file
    fn save_checkpoint(checkpoint: &Checkpoint, path: &Path) -> Result<()> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create checkpoint file: {:?}", path))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, checkpoint)
            .with_context(|| format!("Failed to write checkpoint file: {:?}", path))?;
        writer.flush()?;
        Ok(())
    }

    /// Delete checkpoint file
    pub fn delete_checkpoint(&self) -> Result<()> {
        let checkpoint_path = Self::checkpoint_path(&self.config.checkpoint_dir, &self.job_id);
        if checkpoint_path.exists() {
            fs::remove_file(&checkpoint_path).with_context(|| {
                format!("Failed to delete checkpoint file: {:?}", checkpoint_path)
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> CheckpointConfig {
        CheckpointConfig {
            checkpoint_dir: temp_dir.path().to_path_buf(),
            checkpoint_interval_rows: 100,
            checkpoint_interval_duration: Duration::from_secs(10),
            max_errors: 10,
            ..Default::default()
        }
    }

    #[test]
    fn test_checkpoint_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);

        let manager = CheckpointManager::new("test_job", config)?;

        assert_eq!(manager.job_id(), "test_job");
        assert_eq!(manager.checkpoint.rows_processed, 0);
        assert_eq!(manager.checkpoint.state, LoadState::NotStarted);

        Ok(())
    }

    #[test]
    fn test_record_success() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);
        let mut manager = CheckpointManager::new("test_job", config)?;

        manager.record_success(1)?;
        manager.record_success(2)?;
        manager.record_success(3)?;

        assert_eq!(manager.checkpoint.rows_processed, 3);
        assert_eq!(manager.checkpoint.current_row, 3);

        Ok(())
    }

    #[test]
    fn test_record_error() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);
        let mut manager = CheckpointManager::new("test_job", config)?;

        let error = anyhow!("Operation timeout");
        manager.record_error(5, error)?;

        assert_eq!(manager.checkpoint.rows_failed, 1);
        assert_eq!(manager.checkpoint.error_summary.total_errors, 1);
        assert_eq!(manager.checkpoint.error_summary.recent_errors.len(), 1);

        let error_record = &manager.checkpoint.error_summary.recent_errors[0];
        assert_eq!(error_record.row_number, 5);
        assert_eq!(error_record.category, ErrorCategory::Timeout);
        assert!(error_record.is_transient);

        Ok(())
    }

    #[test]
    fn test_error_category_detection() {
        assert_eq!(
            ErrorCategory::from_error_message("Connection failed"),
            ErrorCategory::DatabaseConnection
        );
        assert_eq!(
            ErrorCategory::from_error_message("Duplicate key constraint"),
            ErrorCategory::DatabaseConstraint
        );
        assert_eq!(
            ErrorCategory::from_error_message("Parse error"),
            ErrorCategory::DataFormat
        );
        assert_eq!(
            ErrorCategory::from_error_message("Operation timeout"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn test_error_category_transient() {
        assert!(ErrorCategory::DatabaseConnection.is_transient());
        assert!(ErrorCategory::Timeout.is_transient());
        assert!(!ErrorCategory::DatabaseConstraint.is_transient());
        assert!(!ErrorCategory::DataFormat.is_transient());
    }

    #[test]
    fn test_should_checkpoint_by_rows() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&temp_dir);
        config.checkpoint_interval_rows = 10;

        let mut manager = CheckpointManager::new("test_job", config)?;

        // Not ready yet
        for i in 1..=9 {
            manager.record_success(i)?;
            assert!(!manager.should_checkpoint());
        }

        // Should checkpoint at row 10
        manager.record_success(10)?;
        assert!(manager.should_checkpoint());

        Ok(())
    }

    #[test]
    fn test_checkpoint_save_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);

        let mut manager = CheckpointManager::new("test_job", config.clone())?;

        // Process 100 rows
        for i in 1..=100 {
            manager.record_success(i)?;
        }
        manager.update_file_offset(1024);
        manager.checkpoint()?;

        // Load checkpoint
        let resumed = CheckpointManager::resume("test_job", config)?;
        assert_eq!(resumed.checkpoint.rows_processed, 100);
        assert_eq!(resumed.checkpoint.current_row, 100);
        assert_eq!(resumed.checkpoint.file_offset, 1024);
        assert_eq!(resumed.checkpoint.state, LoadState::Paused);

        Ok(())
    }

    #[test]
    fn test_batch_tracking() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);
        let mut manager = CheckpointManager::new("test_job", config)?;

        manager.start_batch(0, 100);
        for i in 0..100 {
            manager.record_success(i)?;
        }
        manager.complete_batch(100)?;

        assert_eq!(manager.checkpoint.batch_progress.len(), 1);
        assert_eq!(manager.checkpoint.batch_progress[0].batch_id, 1);
        assert_eq!(manager.checkpoint.batch_progress[0].rows_loaded, 100);
        assert_eq!(
            manager.checkpoint.batch_progress[0].state,
            BatchState::Completed
        );

        Ok(())
    }

    #[test]
    fn test_max_errors() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&temp_dir);
        config.max_errors = 3;

        let mut manager = CheckpointManager::new("test_job", config)?;

        // First 2 errors should be OK
        manager.record_error(1, anyhow!("Error 1"))?;
        manager.record_error(2, anyhow!("Error 2"))?;

        // 3rd error should trigger threshold
        let result = manager.record_error(3, anyhow!("Error 3"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Maximum error threshold"));

        Ok(())
    }

    #[test]
    fn test_finalize() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);
        let mut manager = CheckpointManager::new("test_job", config)?;

        manager.record_success(100)?;
        manager.finalize()?;

        assert_eq!(manager.checkpoint.state, LoadState::Completed);

        Ok(())
    }

    #[test]
    fn test_retry_delay_calculation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);
        let manager = CheckpointManager::new("test_job", config)?;

        let delay0 = manager.calculate_retry_delay(0);
        let delay1 = manager.calculate_retry_delay(1);
        let delay2 = manager.calculate_retry_delay(2);

        assert_eq!(delay0.as_millis(), 100); // 100ms * 2^0 = 100ms
        assert_eq!(delay1.as_millis(), 200); // 100ms * 2^1 = 200ms
        assert_eq!(delay2.as_millis(), 400); // 100ms * 2^2 = 400ms

        Ok(())
    }

    #[test]
    fn test_should_retry() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&temp_dir);
        config.max_retries = 3;

        let manager = CheckpointManager::new("test_job", config)?;

        // Transient error - should retry
        assert!(manager.should_retry(ErrorCategory::Timeout, 0));
        assert!(manager.should_retry(ErrorCategory::Timeout, 2));
        assert!(!manager.should_retry(ErrorCategory::Timeout, 3)); // Max retries

        // Non-transient error - should not retry
        assert!(!manager.should_retry(ErrorCategory::DatabaseConstraint, 0));

        Ok(())
    }

    #[test]
    fn test_resume_or_create_new() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);

        // First call - should create new
        let mut manager = CheckpointManager::resume_or_create("test_job", config.clone())?;
        assert_eq!(manager.checkpoint.rows_processed, 0);

        // Save checkpoint after processing 50 rows
        for i in 1..=50 {
            manager.record_success(i)?;
        }
        manager.checkpoint()?;

        // Second call - should resume
        let resumed = CheckpointManager::resume_or_create("test_job", config)?;
        assert_eq!(resumed.checkpoint.rows_processed, 50);

        Ok(())
    }

    #[test]
    fn test_delete_checkpoint() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&temp_dir);

        let mut manager = CheckpointManager::new("test_job", config)?;
        manager.checkpoint()?;

        let checkpoint_path = temp_dir.path().join("test_job.checkpoint.json");
        assert!(checkpoint_path.exists());

        manager.delete_checkpoint()?;
        assert!(!checkpoint_path.exists());

        Ok(())
    }
}
