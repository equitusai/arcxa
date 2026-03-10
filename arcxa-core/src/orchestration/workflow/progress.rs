use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use utoipa::ToSchema;

/// Execution status of a workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Progress information for a single workflow step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepProgress {
    pub step_name: String,
    pub step_type: String,
    pub rows_processed: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Overall workflow execution progress
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowProgress {
    pub execution_id: String,
    pub workflow_id: String,
    pub status: ExecutionStatus,
    pub current_step: Option<StepProgress>,
    pub total_steps: usize,
    pub steps_completed: u64,
    pub rows_processed: u64,
    pub total_rows: Option<u64>,
    pub percent_complete: Option<f64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_updated: DateTime<Utc>,
    pub error: Option<String>,
    pub eta_seconds: Option<u64>,
}

/// Thread-safe progress tracker for workflow execution
#[derive(Debug)]
pub struct ProgressTracker {
    execution_id: String,
    workflow_id: String,
    status: Arc<RwLock<ExecutionStatus>>,
    current_step_name: Arc<RwLock<Option<String>>>,
    current_step_type: Arc<RwLock<Option<String>>>,
    rows_processed: Arc<AtomicU64>,
    total_rows: Arc<RwLock<Option<u64>>>,
    total_steps: usize,
    steps_completed: Arc<AtomicU64>,
    started_at: DateTime<Utc>,
    completed_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_updated: Arc<RwLock<DateTime<Utc>>>,
    step_started_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    error: Arc<RwLock<Option<String>>>,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new(execution_id: String, workflow_id: String, total_steps: usize) -> Self {
        let now = Utc::now();
        Self {
            execution_id,
            workflow_id,
            status: Arc::new(RwLock::new(ExecutionStatus::Queued)),
            current_step_name: Arc::new(RwLock::new(None)),
            current_step_type: Arc::new(RwLock::new(None)),
            rows_processed: Arc::new(AtomicU64::new(0)),
            total_rows: Arc::new(RwLock::new(None)),
            total_steps,
            steps_completed: Arc::new(AtomicU64::new(0)),
            started_at: now,
            completed_at: Arc::new(RwLock::new(None)),
            last_updated: Arc::new(RwLock::new(now)),
            step_started_at: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(None)),
        }
    }

    /// Start execution (transition from Queued to Running)
    pub fn start(&self) {
        let mut status = self.status.write().unwrap();
        *status = ExecutionStatus::Running;
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Set the current step being executed
    pub fn set_current_step(&self, step_name: String, step_type: String) {
        let mut current_step_name = self.current_step_name.write().unwrap();
        *current_step_name = Some(step_name);
        let mut current_step_type = self.current_step_type.write().unwrap();
        *current_step_type = Some(step_type);
        let mut step_started_at = self.step_started_at.write().unwrap();
        *step_started_at = Some(Utc::now());
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Complete the current step
    pub fn complete_step(&self) {
        self.steps_completed.fetch_add(1, Ordering::Relaxed);
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Update the number of rows processed (lock-free atomic operation)
    pub fn update_rows_processed(&self, count: u64) {
        self.rows_processed.store(count, Ordering::Relaxed);
        // Note: We don't update last_updated here to avoid lock contention in hot path
        // It will be updated by periodic snapshots or step transitions
    }

    /// Increment rows processed by delta (lock-free atomic operation)
    pub fn increment_rows_processed(&self, delta: u64) {
        self.rows_processed.fetch_add(delta, Ordering::Relaxed);
    }

    /// Set the total number of rows expected
    pub fn set_total_rows(&self, total: u64) {
        let mut total_rows = self.total_rows.write().unwrap();
        *total_rows = Some(total);
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Mark execution as completed successfully
    pub fn complete(&self) {
        let mut status = self.status.write().unwrap();
        *status = ExecutionStatus::Completed;
        let mut completed_at = self.completed_at.write().unwrap();
        *completed_at = Some(Utc::now());
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Mark execution as failed with error message
    pub fn fail(&self, error_msg: String) {
        let mut status = self.status.write().unwrap();
        *status = ExecutionStatus::Failed;
        let mut error = self.error.write().unwrap();
        *error = Some(error_msg);
        let mut completed_at = self.completed_at.write().unwrap();
        *completed_at = Some(Utc::now());
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Mark execution as cancelled
    pub fn cancel(&self) {
        let mut status = self.status.write().unwrap();
        *status = ExecutionStatus::Cancelled;
        let mut completed_at = self.completed_at.write().unwrap();
        *completed_at = Some(Utc::now());
        let mut last_updated = self.last_updated.write().unwrap();
        *last_updated = Utc::now();
    }

    /// Get current execution status
    pub fn get_status(&self) -> ExecutionStatus {
        self.status.read().unwrap().clone()
    }

    /// Calculate percent complete based on rows or steps
    fn calculate_percent_complete(&self) -> Option<f64> {
        // Prefer row-based progress if available
        if let Some(total) = *self.total_rows.read().unwrap() {
            if total > 0 {
                let processed = self.rows_processed.load(Ordering::Relaxed);
                return Some((processed as f64 / total as f64) * 100.0);
            }
        }

        // Fall back to step-based progress
        if self.total_steps > 0 {
            let completed = self.steps_completed.load(Ordering::Relaxed);
            return Some((completed as f64 / self.total_steps as f64) * 100.0);
        }

        None
    }

    /// Estimate time remaining based on current progress
    fn calculate_eta(&self) -> Option<u64> {
        let percent = self.calculate_percent_complete()?;
        if percent <= 0.0 || percent >= 100.0 {
            return None;
        }

        let elapsed = Utc::now().signed_duration_since(self.started_at);
        let elapsed_secs = elapsed.num_seconds() as f64;

        // ETA = (elapsed / percent_complete) * (100 - percent_complete)
        let total_estimated = elapsed_secs / (percent / 100.0);
        let remaining = total_estimated - elapsed_secs;

        Some(remaining.max(0.0) as u64)
    }

    /// Get a snapshot of current progress
    pub fn snapshot(&self) -> WorkflowProgress {
        let status = self.status.read().unwrap().clone();
        let current_step_name = self.current_step_name.read().unwrap().clone();
        let current_step_type = self.current_step_type.read().unwrap().clone();
        let step_started_at = self.step_started_at.read().unwrap().clone();
        let rows_processed = self.rows_processed.load(Ordering::Relaxed);
        let total_rows = *self.total_rows.read().unwrap();
        let steps_completed = self.steps_completed.load(Ordering::Relaxed);
        let completed_at = *self.completed_at.read().unwrap();
        let error = self.error.read().unwrap().clone();

        let current_step = if let (Some(name), Some(step_type), Some(started)) =
            (current_step_name, current_step_type, step_started_at)
        {
            Some(StepProgress {
                step_name: name,
                step_type,
                rows_processed,
                started_at: started,
                completed_at: None,
            })
        } else {
            None
        };

        let percent_complete = self.calculate_percent_complete();
        let eta_seconds = self.calculate_eta();

        // Update last_updated timestamp
        let mut last_updated_guard = self.last_updated.write().unwrap();
        *last_updated_guard = Utc::now();
        let last_updated = *last_updated_guard;

        WorkflowProgress {
            execution_id: self.execution_id.clone(),
            workflow_id: self.workflow_id.clone(),
            status,
            current_step,
            total_steps: self.total_steps,
            steps_completed,
            rows_processed,
            total_rows,
            percent_complete,
            started_at: self.started_at,
            completed_at,
            last_updated,
            error,
            eta_seconds,
        }
    }

    /// Get execution ID
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Get workflow ID
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_progress_tracker_lifecycle() {
        let tracker = ProgressTracker::new("exec-123".to_string(), "workflow-456".to_string(), 3);

        // Initially queued
        assert_eq!(tracker.get_status(), ExecutionStatus::Queued);

        // Start execution
        tracker.start();
        assert_eq!(tracker.get_status(), ExecutionStatus::Running);

        // Set current step
        tracker.set_current_step("step1".to_string(), "csv_source".to_string());

        // Update progress
        tracker.set_total_rows(1000);
        tracker.update_rows_processed(500);

        let progress = tracker.snapshot();
        assert_eq!(progress.rows_processed, 500);
        assert_eq!(progress.total_rows, Some(1000));
        assert!(progress.percent_complete.is_some());
        assert!((progress.percent_complete.unwrap() - 50.0).abs() < 0.1);

        // Complete step
        tracker.complete_step();
        assert_eq!(tracker.snapshot().steps_completed, 1);

        // Complete execution
        tracker.complete();
        assert_eq!(tracker.get_status(), ExecutionStatus::Completed);
        assert!(tracker.snapshot().completed_at.is_some());
    }

    #[test]
    fn test_progress_tracker_failure() {
        let tracker = ProgressTracker::new("exec-123".to_string(), "workflow-456".to_string(), 3);

        tracker.start();
        tracker.fail("Something went wrong".to_string());

        let progress = tracker.snapshot();
        assert_eq!(progress.status, ExecutionStatus::Failed);
        assert_eq!(progress.error, Some("Something went wrong".to_string()));
        assert!(progress.completed_at.is_some());
    }

    #[test]
    fn test_progress_tracker_cancellation() {
        let tracker = ProgressTracker::new("exec-123".to_string(), "workflow-456".to_string(), 3);

        tracker.start();
        tracker.cancel();

        assert_eq!(tracker.get_status(), ExecutionStatus::Cancelled);
        assert!(tracker.snapshot().completed_at.is_some());
    }

    #[test]
    fn test_eta_calculation() {
        let tracker = ProgressTracker::new("exec-123".to_string(), "workflow-456".to_string(), 4);

        tracker.start();
        tracker.set_total_rows(1000);

        // Sleep a bit to get meaningful elapsed time
        thread::sleep(Duration::from_millis(100));

        tracker.update_rows_processed(250); // 25% complete

        let progress = tracker.snapshot();
        assert!(progress.eta_seconds.is_some());
        // ETA should be roughly 3x elapsed time (since we're 25% done)
        // We don't assert exact value due to timing variability
    }

    #[test]
    fn test_atomic_row_updates() {
        let tracker = Arc::new(ProgressTracker::new(
            "exec-123".to_string(),
            "workflow-456".to_string(),
            1,
        ));

        tracker.start();

        // Spawn multiple threads updating rows
        let mut handles = vec![];
        for _ in 0..10 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    tracker_clone.increment_rows_processed(1);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have processed exactly 1000 rows (10 threads * 100 increments)
        let progress = tracker.snapshot();
        assert_eq!(progress.rows_processed, 1000);
    }
}
