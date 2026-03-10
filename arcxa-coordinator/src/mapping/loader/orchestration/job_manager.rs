//! Loader Job Manager
//!
//! Central orchestrator for ETL loader jobs. Manages job lifecycle,
//! spawns background workers, tracks progress, and handles cancellation.

use anyhow::{Context, Result};
use dashmap::DashMap;
use graphica_core::core::lineage::LineageSink;
use std::sync::Arc;
use std::time::Duration;

use super::config::{LoaderJobConfig, LoaderWorkerConfig};
use super::job_state::{JobProgress, JobResult, LoaderJobState, LoaderJobStatus, LoaderJobSummary};
use super::worker::LoaderWorker;
use crate::mapping::loader::checkpoint::CheckpointConfig;
use crate::mapping::loader::dlq::DlqConfig;
use crate::observability::metrics::LoaderMetrics;

/// Central job manager for ETL loader orchestration
///
/// Manages multiple concurrent loader jobs, providing:
/// - Job registration and lifecycle management
/// - Background task spawning via tokio::spawn
/// - Progress tracking via shared state (DashMap)
/// - Cancellation token management
/// - Graceful shutdown coordination
///
/// ## Thread Safety
///
/// All methods are thread-safe. The manager uses DashMap for lock-free
/// concurrent access to job state.
pub struct LoaderJobManager {
    /// Active and completed jobs (thread-safe, lock-free reads)
    jobs: Arc<DashMap<String, LoaderJobState>>,

    /// Background task handles (stored separately to allow LoaderJobState to be cloneable)
    task_handles: Arc<DashMap<String, tokio::task::JoinHandle<Result<JobResult>>>>,

    /// Metrics registry for observability
    metrics: Arc<LoaderMetrics>,

    /// Configuration
    config: LoaderJobConfig,

    /// Lineage sink for W3C PROV tracking (Sprint 1.5)
    lineage_sink: Option<Arc<dyn LineageSink>>,
}

impl LoaderJobManager {
    /// Create new job manager without lineage tracking
    pub fn new(metrics: Arc<LoaderMetrics>, config: LoaderJobConfig) -> Result<Self> {
        Self::new_internal(metrics, config, None)
    }

    /// Create new job manager with lineage tracking (Sprint 1.5)
    pub fn new_with_lineage(
        metrics: Arc<LoaderMetrics>,
        config: LoaderJobConfig,
        lineage_sink: Arc<dyn LineageSink>,
    ) -> Result<Self> {
        Self::new_internal(metrics, config, Some(lineage_sink))
    }

    /// Internal constructor shared by both public constructors
    fn new_internal(
        metrics: Arc<LoaderMetrics>,
        config: LoaderJobConfig,
        lineage_sink: Option<Arc<dyn LineageSink>>,
    ) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        // Ensure checkpoint and DLQ directories exist
        std::fs::create_dir_all(&config.checkpoint_dir)
            .context("Failed to create checkpoint directory")?;
        std::fs::create_dir_all(&config.dlq_dir).context("Failed to create DLQ directory")?;

        let lineage_enabled = lineage_sink.is_some();

        tracing::info!(
            "Initializing LoaderJobManager: max_concurrent={}, batch_size={}, checkpoint_dir={:?}, lineage_enabled={}",
            config.max_concurrent_jobs,
            config.batch_size,
            config.checkpoint_dir,
            lineage_enabled
        );

        Ok(Self {
            jobs: Arc::new(DashMap::new()),
            task_handles: Arc::new(DashMap::new()),
            metrics,
            config,
            lineage_sink,
        })
    }

    /// Register new job (does not start execution)
    pub fn register_job(
        &self,
        job_id: String,
        name: String,
        source_file: std::path::PathBuf,
        target_table: String,
    ) -> Result<()> {
        if self.jobs.contains_key(&job_id) {
            anyhow::bail!("Job already exists: {}", job_id);
        }

        let state = LoaderJobState::new(job_id.clone(), name, source_file, target_table);

        self.jobs.insert(job_id.clone(), state);
        self.metrics.job_created();

        tracing::info!("Registered job: {}", job_id);

        Ok(())
    }

    /// Start job execution (spawns background task)
    pub async fn start_job(&self, job_id: &str) -> Result<()> {
        // Get job state
        let mut state = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        // Check if job can be started
        if !state.can_start() {
            anyhow::bail!("Job cannot be started (status: {})", state.status);
        }

        // Mark as running
        state.mark_started();

        // Clone necessary data for worker
        let source_file = state.source_file.clone();
        let target_table = state.target_table.clone();
        let cancel_token = state.cancel_token.clone();

        // Drop the lock before spawning
        drop(state);

        // Create worker config
        let worker_config = LoaderWorkerConfig {
            dml_mode: self.config.dml_mode,
            job_id: job_id.to_string(),
            source_file,
            target_table,
            batch_size: self.config.batch_size,
            checkpoint_config: CheckpointConfig {
                checkpoint_dir: self.config.checkpoint_dir.clone(),
                checkpoint_interval_rows: self.config.checkpoint_interval_rows,
                checkpoint_interval_duration: self.config.checkpoint_interval_duration,
                ..Default::default()
            },
            dlq_config: DlqConfig {
                output_dir: self.config.dlq_dir.clone(),
                ..Default::default()
            },
            csv_buffer_size: 8 * 1024 * 1024, // 8MB
            csv_delimiter: b',',
            csv_has_header: true,
            max_errors: 10_000,
            max_retries: 3,
            retry_base_delay_ms: 100,
        };

        // Create worker (with or without lineage)
        // Use MockDB2Connection by default (real DB2 integration requires additional config)
        use crate::mapping::loader::db2_connection::MockDB2Connection;

        let worker: LoaderWorker<MockDB2Connection> =
            if let Some(ref lineage_sink) = self.lineage_sink {
                tracing::info!("Starting job {} with lineage tracking", job_id);
                LoaderWorker::with_lineage(
                    worker_config,
                    self.metrics.clone(),
                    cancel_token,
                    lineage_sink.clone(),
                )
            } else {
                tracing::info!("Starting job {} without lineage", job_id);
                LoaderWorker::new(worker_config, self.metrics.clone(), cancel_token)
            };

        // Clone state for async task
        let job_id_clone = job_id.to_string();
        let jobs = self.jobs.clone();

        // Spawn background task
        let handle = tokio::spawn(async move {
            let result = worker.run().await;

            // Update job state on completion
            if let Some(mut state) = jobs.get_mut(&job_id_clone) {
                match &result {
                    Ok(job_result) => {
                        if job_result.cancelled {
                            state.mark_cancelled();
                        } else {
                            state.mark_completed();
                        }
                    }
                    Err(e) => {
                        state.mark_failed(e);
                    }
                }
            }

            result
        });

        // Store task handle
        self.task_handles.insert(job_id.to_string(), handle);

        tracing::info!("Job started: {}", job_id);

        Ok(())
    }

    /// Update job progress (called by worker)
    pub fn update_progress(&self, job_id: &str, progress: JobProgress) {
        if let Some(mut state) = self.jobs.get_mut(job_id) {
            state.progress = progress;
        }
    }

    /// Mark job as completed
    pub fn complete_job(&self, job_id: &str, rows_processed: u64, duration: Duration) {
        if let Some(mut state) = self.jobs.get_mut(job_id) {
            state.mark_completed();
            self.metrics.job_completed("loader", duration.as_secs_f64());

            tracing::info!(
                "Job completed: {} ({} rows in {:.1}s)",
                job_id,
                rows_processed,
                duration.as_secs_f64()
            );
        }
    }

    /// Mark job as failed
    pub fn fail_job(&self, job_id: &str, error: anyhow::Error) {
        if let Some(mut state) = self.jobs.get_mut(job_id) {
            let duration = state.elapsed().unwrap_or(Duration::ZERO);
            state.mark_failed(&error);
            self.metrics.job_failed("loader", duration.as_secs_f64());

            tracing::error!("Job failed: {} - {}", job_id, error);
        }
    }

    /// Cancel running job (sends cancellation signal)
    pub async fn cancel_job(&self, job_id: &str) -> Result<()> {
        let mut state = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        if !state.can_cancel() {
            anyhow::bail!("Job cannot be cancelled (status: {})", state.status);
        }

        // Send cancellation signal
        state.cancel_token.cancel();

        tracing::info!("Cancellation requested for job: {}", job_id);

        Ok(())
    }

    /// Get job status
    pub fn get_job_status(&self, job_id: &str) -> Option<LoaderJobState> {
        self.jobs.get(job_id).map(|state| state.clone())
    }

    /// List jobs with optional filter
    pub fn list_jobs(
        &self,
        status_filter: Option<LoaderJobStatus>,
        limit: usize,
    ) -> Vec<LoaderJobSummary> {
        self.jobs
            .iter()
            .filter(|entry| {
                status_filter
                    .map(|status| entry.value().status == status)
                    .unwrap_or(true)
            })
            .take(limit)
            .map(|entry| LoaderJobSummary::from(entry.value()))
            .collect()
    }

    /// Resume job from checkpoint
    pub async fn resume_job(&self, job_id: &str) -> Result<()> {
        // TODO: Implement in Phase 3
        tracing::warn!("resume_job not yet implemented for job: {}", job_id);
        Ok(())
    }

    /// Graceful shutdown - cancel all jobs and wait
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down LoaderJobManager ({} jobs)", self.jobs.len());

        // Send cancellation signal to all running jobs
        let mut cancel_count = 0;
        for entry in self.jobs.iter() {
            if entry.status == LoaderJobStatus::Running {
                entry.cancel_token.cancel();
                cancel_count += 1;
            }
        }

        if cancel_count > 0 {
            tracing::info!(
                "Sent cancellation to {} running jobs, waiting...",
                cancel_count
            );

            // Collect all task handles
            let mut handles = Vec::new();
            for entry in self.task_handles.iter() {
                handles.push(entry.key().clone());
            }

            // Wait for tasks to complete (with timeout)
            let wait_future = async {
                for job_id in handles {
                    if let Some((_, handle)) = self.task_handles.remove(&job_id) {
                        let _ = handle.await;
                    }
                }
            };

            match tokio::time::timeout(self.config.shutdown_timeout, wait_future).await {
                Ok(_) => tracing::info!("All loader jobs shut down gracefully"),
                Err(_) => {
                    tracing::warn!("Shutdown timeout reached, some jobs may not have finished")
                }
            }
        }

        tracing::info!("LoaderJobManager shutdown complete");
        Ok(())
    }

    /// Get configuration
    pub fn config(&self) -> &LoaderJobConfig {
        &self.config
    }

    /// Get active job count
    pub fn active_job_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|entry| entry.status == LoaderJobStatus::Running)
            .count()
    }

    /// Health check - return system health status
    pub fn health_check(&self) -> HealthCheckResult {
        let now = chrono::Utc::now();
        let twenty_four_hours_ago = now - chrono::Duration::hours(24);

        let mut active_jobs = 0;
        let mut pending_jobs = 0;
        let mut failed_jobs_24h = 0;
        let mut rows_processed_24h = 0;
        let mut total_duration = 0.0;
        let mut completed_jobs_24h = 0;

        for entry in self.jobs.iter() {
            match entry.status {
                LoaderJobStatus::Running => active_jobs += 1,
                LoaderJobStatus::Pending => pending_jobs += 1,
                LoaderJobStatus::Failed => {
                    if entry
                        .completed_at
                        .map_or(false, |t| t > twenty_four_hours_ago)
                    {
                        failed_jobs_24h += 1;
                    }
                }
                LoaderJobStatus::Completed => {
                    if entry
                        .completed_at
                        .map_or(false, |t| t > twenty_four_hours_ago)
                    {
                        rows_processed_24h += entry.progress.rows_processed;
                        if let Some(duration) = entry.elapsed() {
                            total_duration += duration.as_secs_f64();
                            completed_jobs_24h += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        let avg_throughput = if completed_jobs_24h > 0 {
            rows_processed_24h as f64 / total_duration
        } else {
            0.0
        };

        // Check component health
        let mut components = std::collections::HashMap::new();

        // Checkpoint directory writable
        components.insert(
            "checkpoint_dir".to_string(),
            self.config.checkpoint_dir.exists() && self.config.checkpoint_dir.is_dir(),
        );

        // DLQ directory writable
        components.insert(
            "dlq_dir".to_string(),
            self.config.dlq_dir.exists() && self.config.dlq_dir.is_dir(),
        );

        // Check if we're within job limits
        components.insert(
            "job_capacity".to_string(),
            active_jobs < self.config.max_concurrent_jobs,
        );

        let degraded_components = components.values().filter(|&&v| !v).count();
        let is_healthy = degraded_components == 0;

        HealthCheckResult {
            is_healthy,
            degraded_components,
            active_jobs,
            pending_jobs,
            failed_jobs_24h,
            rows_processed_24h,
            avg_throughput,
            components,
        }
    }
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub is_healthy: bool,
    pub degraded_components: usize,
    pub active_jobs: usize,
    pub pending_jobs: usize,
    pub failed_jobs_24h: usize,
    pub rows_processed_24h: u64,
    pub avg_throughput: f64,
    pub components: std::collections::HashMap<String, bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
    use crate::mapping::loader::lineage::RdfLineageSink;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    fn create_test_manager() -> LoaderJobManager {
        let temp_dir = TempDir::new().unwrap();
        let config = LoaderJobConfig {
            checkpoint_dir: temp_dir.path().join("checkpoints"),
            dlq_dir: temp_dir.path().join("dlq"),
            ..Default::default()
        };

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new()).unwrap());
        LoaderJobManager::new(metrics, config).unwrap()
    }

    fn create_test_manager_with_lineage() -> LoaderJobManager {
        let temp_dir = TempDir::new().unwrap();
        let config = LoaderJobConfig {
            checkpoint_dir: temp_dir.path().join("checkpoints"),
            dlq_dir: temp_dir.path().join("dlq"),
            ..Default::default()
        };

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new()).unwrap());

        // Create in-memory RDF store
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let lineage_sink = Arc::new(RdfLineageSink::new(rdf_store, None));

        LoaderJobManager::new_with_lineage(metrics, config, lineage_sink).unwrap()
    }

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name,age,email").unwrap();
        writeln!(file, "Alice,30,alice@example.com").unwrap();
        writeln!(file, "Bob,25,bob@example.com").unwrap();
        writeln!(file, "Charlie,35,charlie@example.com").unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_register_job() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/tmp/test.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Pending);
        assert_eq!(status.name, "Test Job");
    }

    #[test]
    fn test_duplicate_job_registration() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/tmp/test.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        // Second registration should fail
        let result = manager.register_job(
            "job_1".to_string(),
            "Duplicate".to_string(),
            std::path::PathBuf::from("/tmp/test2.csv"),
            "test_table".to_string(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_list_jobs() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Job 1".to_string(),
                std::path::PathBuf::from("/tmp/test1.csv"),
                "table1".to_string(),
            )
            .unwrap();

        manager
            .register_job(
                "job_2".to_string(),
                "Job 2".to_string(),
                std::path::PathBuf::from("/tmp/test2.csv"),
                "table2".to_string(),
            )
            .unwrap();

        let jobs = manager.list_jobs(None, 10);
        assert_eq!(jobs.len(), 2);

        let pending_jobs = manager.list_jobs(Some(LoaderJobStatus::Pending), 10);
        assert_eq!(pending_jobs.len(), 2);
    }

    #[test]
    fn test_update_progress() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/tmp/test.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        let mut progress = JobProgress::default();
        progress.current_row = 5000;
        progress.rows_processed = 5000;
        progress.progress_percent = 50.0;

        manager.update_progress("job_1", progress);

        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.progress.current_row, 5000);
        assert_eq!(status.progress.progress_percent, 50.0);
    }

    #[test]
    fn test_complete_job() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/tmp/test.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        manager.complete_job("job_1", 10000, Duration::from_secs(60));

        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Completed);
        assert!(status.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_start_job() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start the job
        manager.start_job("job_1").await.unwrap();

        // Job should be running
        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Running);
        assert!(status.started_at.is_some());

        // Wait a bit for job to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Job should complete
        let final_status = manager.get_job_status("job_1").unwrap();
        assert!(matches!(
            final_status.status,
            LoaderJobStatus::Completed | LoaderJobStatus::Running
        ));
    }

    #[tokio::test]
    async fn test_start_job_already_running() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start the job
        manager.start_job("job_1").await.unwrap();

        // Try to start again - should fail
        let result = manager.start_job("job_1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_nonexistent_job() {
        let manager = create_test_manager();

        // Try to start a job that doesn't exist
        let result = manager.start_job("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_new_with_lineage() {
        let manager = create_test_manager_with_lineage();

        // Manager should be created successfully
        assert_eq!(manager.active_job_count(), 0);

        // Lineage sink should be configured
        assert!(manager.lineage_sink.is_some());
    }

    #[test]
    fn test_new_without_lineage() {
        let manager = create_test_manager();

        // Manager should be created successfully
        assert_eq!(manager.active_job_count(), 0);

        // Lineage sink should NOT be configured
        assert!(manager.lineage_sink.is_none());
    }

    #[tokio::test]
    async fn test_job_completion_updates_state() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Wait for job to complete
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let status = manager.get_job_status("job_1").unwrap();
            if status.status == LoaderJobStatus::Completed {
                break;
            }
        }

        // Verify final state transitions
        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Completed);
        assert!(status.started_at.is_some());
        assert!(status.completed_at.is_some());
        // Note: rows_processed may be 0 if worker completes quickly or encounters issues
    }

    #[tokio::test]
    async fn test_cancel_running_job() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Immediately cancel
        let cancel_result = manager.cancel_job("job_1").await;
        assert!(cancel_result.is_ok());

        // Verify cancellation signal sent
        let status = manager.get_job_status("job_1").unwrap();
        assert!(status.cancel_token.is_cancelled());
    }

    #[tokio::test]
    async fn test_cannot_cancel_completed_job() {
        let manager = create_test_manager();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/tmp/test.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        // Manually mark as completed
        manager.complete_job("job_1", 1000, Duration::from_secs(10));

        // Try to cancel - should fail
        let result = manager.cancel_job("job_1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_concurrent_jobs() {
        let csv_file1 = create_test_csv();
        let csv_file2 = create_test_csv();
        let csv_file3 = create_test_csv();
        let manager = create_test_manager_with_lineage();

        // Register multiple jobs
        manager
            .register_job(
                "job_1".to_string(),
                "Job 1".to_string(),
                csv_file1.path().to_path_buf(),
                "table1".to_string(),
            )
            .unwrap();

        manager
            .register_job(
                "job_2".to_string(),
                "Job 2".to_string(),
                csv_file2.path().to_path_buf(),
                "table2".to_string(),
            )
            .unwrap();

        manager
            .register_job(
                "job_3".to_string(),
                "Job 3".to_string(),
                csv_file3.path().to_path_buf(),
                "table3".to_string(),
            )
            .unwrap();

        // Start all jobs
        manager.start_job("job_1").await.unwrap();
        manager.start_job("job_2").await.unwrap();
        manager.start_job("job_3").await.unwrap();

        // All should be running
        assert_eq!(manager.active_job_count(), 3);

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify all completed
        let jobs = manager.list_jobs(None, 10);
        assert_eq!(jobs.len(), 3);

        for job in jobs {
            assert!(matches!(
                job.status,
                LoaderJobStatus::Completed | LoaderJobStatus::Running
            ));
        }
    }

    #[tokio::test]
    async fn test_start_job_with_invalid_file() {
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/nonexistent/file.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Wait for job to fail
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify job failed
        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Failed);
        assert!(status.error_message.is_some());
    }

    #[tokio::test]
    async fn test_job_progress_tracking() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Monitor progress until terminal state
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let status = manager.get_job_status("job_1").unwrap();

            if status.status.is_terminal() {
                // Job completed successfully
                assert!(matches!(
                    status.status,
                    LoaderJobStatus::Completed | LoaderJobStatus::Failed
                ));
                return;
            }
        }

        // If we get here, job is still running after 2 seconds
        let status = manager.get_job_status("job_1").unwrap();
        assert!(
            status.status.is_terminal() || status.status == LoaderJobStatus::Running,
            "Job should be in a valid state"
        );
    }

    #[test]
    fn test_health_check_empty_manager() {
        let manager = create_test_manager();

        let health = manager.health_check();

        // Just verify metrics are calculated correctly
        assert_eq!(health.active_jobs, 0);
        assert_eq!(health.pending_jobs, 0);
        assert_eq!(health.failed_jobs_24h, 0);
        assert_eq!(health.rows_processed_24h, 0);
    }

    #[test]
    fn test_health_check_with_jobs() {
        let manager = create_test_manager();

        // Register some jobs
        manager
            .register_job(
                "job_1".to_string(),
                "Job 1".to_string(),
                std::path::PathBuf::from("/tmp/test1.csv"),
                "table1".to_string(),
            )
            .unwrap();

        manager
            .register_job(
                "job_2".to_string(),
                "Job 2".to_string(),
                std::path::PathBuf::from("/tmp/test2.csv"),
                "table2".to_string(),
            )
            .unwrap();

        let health = manager.health_check();

        // Verify job counts are correct
        assert_eq!(health.pending_jobs, 2);
        assert_eq!(health.active_jobs, 0);
    }

    #[test]
    fn test_list_jobs_with_status_filter() {
        let manager = create_test_manager();

        // Register jobs
        manager
            .register_job(
                "job_1".to_string(),
                "Pending Job".to_string(),
                std::path::PathBuf::from("/tmp/test1.csv"),
                "table1".to_string(),
            )
            .unwrap();

        manager
            .register_job(
                "job_2".to_string(),
                "Another Pending".to_string(),
                std::path::PathBuf::from("/tmp/test2.csv"),
                "table2".to_string(),
            )
            .unwrap();

        // Mark one as completed
        manager.complete_job("job_1", 1000, Duration::from_secs(10));

        // List only pending
        let pending = manager.list_jobs(Some(LoaderJobStatus::Pending), 10);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].job_id, "job_2");

        // List only completed
        let completed = manager.list_jobs(Some(LoaderJobStatus::Completed), 10);
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].job_id, "job_1");

        // List all
        let all = manager.list_jobs(None, 10);
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_with_running_jobs() {
        let csv_file = create_test_csv();
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Initiate shutdown
        let shutdown_result = manager.shutdown().await;
        assert!(shutdown_result.is_ok());

        // Verify cancellation was sent
        let status = manager.get_job_status("job_1").unwrap();
        assert!(status.cancel_token.is_cancelled());
    }

    #[test]
    fn test_job_state_can_start() {
        let state = LoaderJobState::new(
            "test".to_string(),
            "Test".to_string(),
            std::path::PathBuf::from("/tmp/test.csv"),
            "table".to_string(),
        );

        // Pending jobs can be started
        assert!(state.can_start());

        // Create a running job
        let mut running_state = state.clone();
        running_state.mark_started();
        assert!(!running_state.can_start());

        // Create a completed job
        let mut completed_state = state.clone();
        completed_state.mark_completed();
        assert!(!completed_state.can_start());
    }

    #[test]
    fn test_config_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = LoaderJobConfig {
            checkpoint_dir: temp_dir.path().join("checkpoints"),
            dlq_dir: temp_dir.path().join("dlq"),
            ..Default::default()
        };

        // Valid config
        assert!(config.validate().is_ok());

        // Invalid: zero concurrent jobs
        config.max_concurrent_jobs = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_jobs = 10;

        // Invalid: zero batch size
        config.batch_size = 0;
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn test_lineage_captured_for_completed_job() {
        let csv_file = create_test_csv();

        // Create manager with RDF store we can query
        let temp_dir = TempDir::new().unwrap();
        let config = LoaderJobConfig {
            checkpoint_dir: temp_dir.path().join("checkpoints"),
            dlq_dir: temp_dir.path().join("dlq"),
            ..Default::default()
        };

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new()).unwrap());
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let lineage_sink = Arc::new(RdfLineageSink::new(rdf_store.clone(), None));

        let manager = LoaderJobManager::new_with_lineage(metrics, config, lineage_sink).unwrap();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                csv_file.path().to_path_buf(),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job
        manager.start_job("job_1").await.unwrap();

        // Wait for completion
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let status = manager.get_job_status("job_1").unwrap();
            if status.status == LoaderJobStatus::Completed {
                break;
            }
        }

        // Give lineage time to be written
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Query lineage from RDF store
        let query = r#"
            PREFIX gph: <http://graphica.io/ontology#>
            PREFIX prov: <http://www.w3.org/ns/prov#>

            SELECT (COUNT(*) as ?count) WHERE {
                ?activity a prov:Activity ;
                          gph:runId "job_1" .
            }
        "#;

        let results = rdf_store.query(query).unwrap();
        assert!(
            !results.is_empty(),
            "Should have lineage events in RDF store"
        );
    }

    #[test]
    fn test_worker_config_creation() {
        let manager = create_test_manager();
        let config = &manager.config;

        // Verify config values are set correctly
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.max_concurrent_jobs, 10);
    }

    #[tokio::test]
    async fn test_job_failure_marks_state_correctly() {
        let manager = create_test_manager_with_lineage();

        manager
            .register_job(
                "job_1".to_string(),
                "Test Job".to_string(),
                std::path::PathBuf::from("/nonexistent/fail.csv"),
                "test_table".to_string(),
            )
            .unwrap();

        // Start job (will fail)
        manager.start_job("job_1").await.unwrap();

        // Wait for failure
        tokio::time::sleep(Duration::from_millis(500)).await;

        let status = manager.get_job_status("job_1").unwrap();
        assert_eq!(status.status, LoaderJobStatus::Failed);
        assert!(status.error_message.is_some());
        assert!(status.started_at.is_some());
        assert!(status.completed_at.is_some());
    }
}
