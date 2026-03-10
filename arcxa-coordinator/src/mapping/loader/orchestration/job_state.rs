//! Loader Job State Management
//!
//! Core data structures for tracking ETL loader job lifecycle, progress,
//! and execution state. Used by LoaderJobManager for orchestration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use super::super::dlq::DlqStats;

// ============================================================================
// Job Status
// ============================================================================

/// Current status of a loader job
///
/// State machine transitions:
/// ```text
/// Pending → Running → Completed
///                  ↘ Failed
///                  ↘ Cancelled
///                  ↘ Paused → Running (resume)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderJobStatus {
    /// Job registered but not yet started
    Pending,

    /// Background task actively processing data
    Running,

    /// Job finished successfully, all rows processed
    Completed,

    /// Job encountered unrecoverable error and terminated
    Failed,

    /// Job cancelled by user request
    Cancelled,

    /// Job checkpointed and awaiting resume
    Paused,
}

impl LoaderJobStatus {
    /// Check if job is in a terminal state (no further transitions)
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Check if job is actively running
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Check if job can be resumed
    pub const fn is_resumable(&self) -> bool {
        matches!(self, Self::Paused)
    }

    /// Check if job can be cancelled
    pub const fn is_cancellable(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

impl std::fmt::Display for LoaderJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Paused => write!(f, "paused"),
        }
    }
}

// ============================================================================
// Job Progress
// ============================================================================

/// Real-time progress information for a running job
///
/// Updated periodically by LoaderWorker during batch processing.
/// Used for API status queries and monitoring dashboards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    /// Current row being processed (0-indexed)
    pub current_row: u64,

    /// Total rows in source file (if known from pre-scan)
    pub total_rows: Option<u64>,

    /// Successfully processed rows
    pub rows_processed: u64,

    /// Rows that failed and were written to DLQ
    pub rows_failed: u64,

    /// Rows intentionally skipped (e.g., header, comments)
    pub rows_skipped: u64,

    /// Progress percentage (0.0 - 100.0)
    ///
    /// Calculated from current_row / total_rows if total known,
    /// otherwise from bytes_read / file_size
    pub progress_percent: f64,

    /// Estimated time remaining in seconds (if calculable)
    pub estimated_time_remaining: Option<f64>,

    /// Current throughput in rows/second
    pub rows_per_second: f64,

    /// Current throughput in bytes/second
    pub bytes_per_second: f64,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// Total bytes in source file
    pub total_bytes: u64,
}

impl Default for JobProgress {
    fn default() -> Self {
        Self {
            current_row: 0,
            total_rows: None,
            rows_processed: 0,
            rows_failed: 0,
            rows_skipped: 0,
            progress_percent: 0.0,
            estimated_time_remaining: None,
            rows_per_second: 0.0,
            bytes_per_second: 0.0,
            bytes_processed: 0,
            total_bytes: 0,
        }
    }
}

impl JobProgress {
    /// Create new progress tracker with known totals
    pub fn new(total_rows: Option<u64>, total_bytes: u64) -> Self {
        Self {
            total_rows,
            total_bytes,
            ..Default::default()
        }
    }

    /// Update progress from current processing state
    pub fn update(
        &mut self,
        current_row: u64,
        rows_processed: u64,
        rows_failed: u64,
        bytes_processed: u64,
        elapsed: Duration,
    ) {
        self.current_row = current_row;
        self.rows_processed = rows_processed;
        self.rows_failed = rows_failed;
        self.bytes_processed = bytes_processed;

        // Calculate throughput
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs > 0.0 {
            self.rows_per_second = rows_processed as f64 / elapsed_secs;
            self.bytes_per_second = bytes_processed as f64 / elapsed_secs;
        }

        // Calculate progress percentage
        self.progress_percent = if let Some(total) = self.total_rows {
            if total > 0 {
                (current_row as f64 / total as f64 * 100.0).min(100.0)
            } else {
                0.0
            }
        } else if self.total_bytes > 0 {
            (bytes_processed as f64 / self.total_bytes as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        // Estimate time remaining
        self.estimated_time_remaining = self.calculate_eta(elapsed);
    }

    /// Calculate estimated time remaining
    fn calculate_eta(&self, elapsed: Duration) -> Option<f64> {
        if self.progress_percent >= 100.0 {
            return Some(0.0);
        }

        if self.progress_percent <= 0.0 {
            return None;
        }

        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs <= 0.0 {
            return None;
        }

        // ETA = (elapsed / progress) * (100 - progress)
        let eta = (elapsed_secs / self.progress_percent) * (100.0 - self.progress_percent);
        Some(eta)
    }

    /// Get human-readable progress summary
    pub fn summary(&self) -> String {
        if let Some(total) = self.total_rows {
            format!(
                "{}/{} rows ({:.1}%, {:.0} rows/sec)",
                self.current_row, total, self.progress_percent, self.rows_per_second
            )
        } else {
            format!(
                "{} rows ({:.1}%, {:.0} rows/sec)",
                self.current_row, self.progress_percent, self.rows_per_second
            )
        }
    }
}

// ============================================================================
// Job State
// ============================================================================

/// Complete state of a loader job
///
/// Stored in LoaderJobManager's DashMap for thread-safe access.
/// Contains all information needed to track, monitor, and control a job.
///
/// Note: Task handles are stored separately in LoaderJobManager to allow
/// this struct to be cloneable for efficient status queries.
#[derive(Debug, Clone)]
pub struct LoaderJobState {
    /// Unique job identifier (format: "load_{uuid}")
    pub job_id: String,

    /// Human-readable job name
    pub name: String,

    /// Current job status
    pub status: LoaderJobStatus,

    /// Real-time progress information
    pub progress: JobProgress,

    /// Cancellation token for graceful shutdown
    ///
    /// Worker polls this token in processing loop.
    /// Set via cancel_job() to request termination.
    pub cancel_token: CancellationToken,

    /// Path to checkpoint file (if exists)
    ///
    /// Used for resume operations. Points to most recent checkpoint JSON.
    pub checkpoint_path: Option<PathBuf>,

    /// Dead letter queue statistics
    pub dlq_stats: DlqStats,

    /// Source file path
    pub source_file: PathBuf,

    /// Target database table
    pub target_table: String,

    /// Timestamp when job was created
    pub created_at: DateTime<Utc>,

    /// Timestamp when job started execution
    pub started_at: Option<DateTime<Utc>>,

    /// Timestamp when job completed/failed/cancelled
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if job failed
    pub error_message: Option<String>,
}

impl LoaderJobState {
    /// Create new job state in Pending status
    pub fn new(job_id: String, name: String, source_file: PathBuf, target_table: String) -> Self {
        Self {
            job_id,
            name,
            status: LoaderJobStatus::Pending,
            progress: JobProgress::default(),
            cancel_token: CancellationToken::new(),
            checkpoint_path: None,
            dlq_stats: DlqStats::default(),
            source_file,
            target_table,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        }
    }

    /// Mark job as started (transition to Running)
    pub fn mark_started(&mut self) {
        self.status = LoaderJobStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark job as completed successfully
    pub fn mark_completed(&mut self) {
        self.status = LoaderJobStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Mark job as failed with error message
    pub fn mark_failed(&mut self, error: impl std::fmt::Display) {
        self.status = LoaderJobStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error.to_string());
    }

    /// Mark job as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = LoaderJobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Mark job as paused (checkpoint saved)
    pub fn mark_paused(&mut self, checkpoint_path: PathBuf) {
        self.status = LoaderJobStatus::Paused;
        self.checkpoint_path = Some(checkpoint_path);
    }

    /// Get job duration (elapsed time from start to completion/current)
    pub fn duration(&self) -> Option<Duration> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Utc::now);

        end.signed_duration_since(start).to_std().ok()
    }

    /// Get elapsed time since job started
    pub fn elapsed(&self) -> Option<Duration> {
        let start = self.started_at?;
        Utc::now().signed_duration_since(start).to_std().ok()
    }

    /// Check if job can be cancelled
    pub fn can_cancel(&self) -> bool {
        self.status.is_cancellable()
    }

    /// Check if job can be resumed
    pub fn can_resume(&self) -> bool {
        self.status.is_resumable() && self.checkpoint_path.is_some()
    }

    /// Check if job can be started
    pub fn can_start(&self) -> bool {
        self.status == LoaderJobStatus::Pending
    }
}

// ============================================================================
// Job Result
// ============================================================================

/// Final result of a completed loader job
///
/// Returned by LoaderWorker::run() when job finishes.
/// Contains statistics and metadata about the execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// Total rows successfully processed
    pub rows_processed: u64,

    /// Total rows failed (written to DLQ)
    pub rows_failed: u64,

    /// Total rows skipped
    pub rows_skipped: u64,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// DLQ statistics
    pub dlq_stats: DlqStats,

    /// Total duration of job execution
    pub duration: Duration,

    /// Number of checkpoints created
    pub checkpoints_created: u64,

    /// Number of batches processed
    pub batches_processed: u64,

    /// Average batch size
    pub avg_batch_size: f64,

    /// Peak throughput (rows/sec)
    pub peak_throughput: f64,

    /// Whether job was cancelled
    pub cancelled: bool,

    /// Error message if job failed
    pub error: Option<String>,
}

impl JobResult {
    /// Calculate success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.rows_processed + self.rows_failed;
        if total > 0 {
            self.rows_processed as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Calculate overall throughput (rows/sec)
    pub fn throughput(&self) -> f64 {
        let secs = self.duration.as_secs_f64();
        if secs > 0.0 {
            self.rows_processed as f64 / secs
        } else {
            0.0
        }
    }

    /// Get human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Processed {} rows ({} failed) in {:.1}s at {:.0} rows/sec",
            self.rows_processed,
            self.rows_failed,
            self.duration.as_secs_f64(),
            self.throughput()
        )
    }
}

impl Default for JobResult {
    fn default() -> Self {
        Self {
            rows_processed: 0,
            rows_failed: 0,
            rows_skipped: 0,
            bytes_processed: 0,
            dlq_stats: DlqStats::default(),
            duration: Duration::ZERO,
            checkpoints_created: 0,
            batches_processed: 0,
            avg_batch_size: 0.0,
            peak_throughput: 0.0,
            cancelled: false,
            error: None,
        }
    }
}

// ============================================================================
// Job Summary (for list operations)
// ============================================================================

/// Lightweight job summary for list queries
///
/// Contains only essential fields, omitting large structures
/// like task handles and cancellation tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderJobSummary {
    pub job_id: String,
    pub name: String,
    pub status: LoaderJobStatus,
    pub source_file: PathBuf,
    pub target_table: String,
    pub rows_processed: u64,
    pub rows_failed: u64,
    pub progress_percent: f64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

impl From<&LoaderJobState> for LoaderJobSummary {
    fn from(state: &LoaderJobState) -> Self {
        Self {
            job_id: state.job_id.clone(),
            name: state.name.clone(),
            status: state.status,
            source_file: state.source_file.clone(),
            target_table: state.target_table.clone(),
            rows_processed: state.progress.rows_processed,
            rows_failed: state.progress.rows_failed,
            progress_percent: state.progress.progress_percent,
            created_at: state.created_at,
            started_at: state.started_at,
            completed_at: state.completed_at,
            error_message: state.error_message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_transitions() {
        assert!(LoaderJobStatus::Pending.is_cancellable());
        assert!(LoaderJobStatus::Running.is_cancellable());
        assert!(LoaderJobStatus::Running.is_active());
        assert!(!LoaderJobStatus::Completed.is_cancellable());
        assert!(LoaderJobStatus::Completed.is_terminal());
        assert!(LoaderJobStatus::Paused.is_resumable());
    }

    #[test]
    fn test_progress_calculation() {
        let mut progress = JobProgress::new(Some(10000), 1024 * 1024);

        progress.update(5000, 4950, 50, 512 * 1024, Duration::from_secs(10));

        assert_eq!(progress.current_row, 5000);
        assert_eq!(progress.rows_processed, 4950);
        assert_eq!(progress.rows_failed, 50);
        assert_eq!(progress.progress_percent, 50.0); // 5000/10000
        assert_eq!(progress.rows_per_second, 495.0); // 4950/10
        assert!(progress.estimated_time_remaining.is_some());
    }

    #[test]
    fn test_progress_eta_calculation() {
        let mut progress = JobProgress::new(Some(10000), 0);
        progress.update(2500, 2500, 0, 0, Duration::from_secs(10));

        // At 25% progress after 10s, should estimate ~30s remaining
        let eta = progress.estimated_time_remaining.unwrap();
        assert!((eta - 30.0).abs() < 1.0);
    }

    #[test]
    fn test_job_state_lifecycle() {
        let mut state = LoaderJobState::new(
            "test_job".to_string(),
            "Test Job".to_string(),
            PathBuf::from("/tmp/test.csv"),
            "test_table".to_string(),
        );

        assert_eq!(state.status, LoaderJobStatus::Pending);
        assert!(state.can_cancel());
        assert!(!state.can_resume());

        state.mark_started();
        assert_eq!(state.status, LoaderJobStatus::Running);
        assert!(state.started_at.is_some());

        state.mark_completed();
        assert_eq!(state.status, LoaderJobStatus::Completed);
        assert!(state.completed_at.is_some());
        assert!(!state.can_cancel());

        let duration = state.duration().unwrap();
        assert!(duration.as_secs() < 1); // Should be near-instant in test
    }

    #[test]
    fn test_job_result_metrics() {
        let result = JobResult {
            rows_processed: 9500,
            rows_failed: 500,
            duration: Duration::from_secs(100),
            ..Default::default()
        };

        assert_eq!(result.success_rate(), 0.95); // 9500 / 10000
        assert_eq!(result.throughput(), 95.0); // 9500 / 100
    }

    #[test]
    fn test_job_summary_conversion() {
        let state = LoaderJobState::new(
            "job_123".to_string(),
            "Test Job".to_string(),
            PathBuf::from("/tmp/data.csv"),
            "customers".to_string(),
        );

        let summary = LoaderJobSummary::from(&state);

        assert_eq!(summary.job_id, "job_123");
        assert_eq!(summary.name, "Test Job");
        assert_eq!(summary.status, LoaderJobStatus::Pending);
        assert_eq!(summary.target_table, "customers");
    }
}
