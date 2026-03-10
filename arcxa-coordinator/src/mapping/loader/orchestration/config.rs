//! Loader Configuration
//!
//! Configuration structures for ETL loader orchestration, including
//! job manager settings, worker parameters, and runtime tuning.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// DML Execution Mode
// ============================================================================

/// DML execution mode for SQL-based loads
///
/// Controls whether to use INSERT statements or MERGE statements for
/// loading data. MERGE enables idempotent loads and handles duplicate keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmlMode {
    /// Use INSERT statements (append-only, fails on duplicate keys)
    Insert,

    /// Use MERGE statements (INSERT or UPDATE based on primary key match)
    ///
    /// Enables idempotent loads:
    /// - New rows are inserted
    /// - Existing rows (matching primary key) are updated
    /// - No errors on duplicate keys
    Merge,
}

impl Default for DmlMode {
    fn default() -> Self {
        DmlMode::Insert
    }
}

impl DmlMode {
    /// Check if this mode requires primary keys
    pub fn requires_primary_keys(&self) -> bool {
        matches!(self, DmlMode::Merge)
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            DmlMode::Insert => "INSERT statements (append-only)",
            DmlMode::Merge => "MERGE statements (insert or update)",
        }
    }
}

// ============================================================================
// Loader Job Manager Configuration
// ============================================================================

/// Configuration for LoaderJobManager
///
/// Controls global orchestration behavior including concurrency limits,
/// checkpoint/DLQ directories, and auto-resume on restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderJobConfig {
    /// DML execution mode (INSERT vs MERGE)
    ///
    /// - INSERT: Append-only, fails on duplicate keys
    /// - MERGE: Idempotent, handles duplicates via UPDATE
    #[serde(default)]
    pub dml_mode: DmlMode,

    /// Maximum number of concurrent loader jobs
    ///
    /// Jobs beyond this limit will be queued in Pending state.
    /// Recommended: 5-10 for typical coordinator workloads.
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,

    /// Number of rows to process per batch
    ///
    /// Larger batches = better throughput, higher memory usage.
    /// Smaller batches = lower memory, more frequent checkpoints.
    /// Recommended: 1000-5000 depending on row width.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Checkpoint interval in number of rows
    ///
    /// Checkpoints save current position for resume on failure.
    /// More frequent = lower replay cost, higher I/O overhead.
    /// Recommended: 10,000 rows.
    #[serde(default = "default_checkpoint_interval_rows")]
    pub checkpoint_interval_rows: u64,

    /// Maximum time between checkpoints
    ///
    /// Even if checkpoint_interval_rows not reached, checkpoint
    /// after this duration to prevent excessive replay on failure.
    #[serde(default = "default_checkpoint_interval_duration")]
    #[serde(with = "humantime_serde")]
    pub checkpoint_interval_duration: Duration,

    /// Base directory for checkpoint files
    ///
    /// Each job creates a checkpoint file: `{checkpoint_dir}/{job_id}.checkpoint.json`
    pub checkpoint_dir: PathBuf,

    /// Base directory for dead letter queue files
    ///
    /// Failed rows written to: `{dlq_dir}/{date}/{category}/{job_id}_{index}.jsonl`
    pub dlq_dir: PathBuf,

    /// Automatically resume paused jobs on coordinator restart
    ///
    /// If true, LoaderJobManager scans checkpoint_dir on startup
    /// and recreates job state for all paused jobs.
    #[serde(default = "default_auto_resume")]
    pub auto_resume: bool,

    /// Maximum number of completed jobs to retain in memory
    ///
    /// Older completed jobs are evicted (LRU). Prevents unbounded
    /// memory growth for long-running coordinators with many jobs.
    #[serde(default = "default_max_completed_jobs")]
    pub max_completed_jobs: usize,

    /// Graceful shutdown timeout
    ///
    /// Maximum time to wait for running jobs to checkpoint and exit
    /// when coordinator is shutting down. Jobs still running after
    /// timeout will be aborted.
    #[serde(default = "default_shutdown_timeout")]
    #[serde(with = "humantime_serde")]
    pub shutdown_timeout: Duration,
}

impl Default for LoaderJobConfig {
    fn default() -> Self {
        Self {
            dml_mode: DmlMode::default(),
            max_concurrent_jobs: default_max_concurrent_jobs(),
            batch_size: default_batch_size(),
            checkpoint_interval_rows: default_checkpoint_interval_rows(),
            checkpoint_interval_duration: default_checkpoint_interval_duration(),
            checkpoint_dir: PathBuf::from("/var/lib/graphica/loader/checkpoints"),
            dlq_dir: PathBuf::from("/var/lib/graphica/loader/dlq"),
            auto_resume: default_auto_resume(),
            max_completed_jobs: default_max_completed_jobs(),
            shutdown_timeout: default_shutdown_timeout(),
        }
    }
}

// Default value functions for serde
fn default_max_concurrent_jobs() -> usize {
    10
}
fn default_batch_size() -> usize {
    1000
}
fn default_checkpoint_interval_rows() -> u64 {
    10_000
}
fn default_checkpoint_interval_duration() -> Duration {
    Duration::from_secs(300)
} // 5 minutes
fn default_auto_resume() -> bool {
    true
}
fn default_max_completed_jobs() -> usize {
    1000
}
fn default_shutdown_timeout() -> Duration {
    Duration::from_secs(30)
}

impl LoaderJobConfig {
    /// Create configuration optimized for high throughput
    ///
    /// Larger batches, less frequent checkpoints, higher concurrency.
    pub fn high_throughput() -> Self {
        Self {
            max_concurrent_jobs: 20,
            batch_size: 5000,
            checkpoint_interval_rows: 50_000,
            checkpoint_interval_duration: Duration::from_secs(600), // 10 minutes
            ..Default::default()
        }
    }

    /// Create configuration optimized for low memory usage
    ///
    /// Smaller batches, more frequent checkpoints, lower concurrency.
    pub fn low_memory() -> Self {
        Self {
            max_concurrent_jobs: 5,
            batch_size: 500,
            checkpoint_interval_rows: 5_000,
            checkpoint_interval_duration: Duration::from_secs(120), // 2 minutes
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    ///
    /// Frequent checkpoints, conservative limits, quick shutdown.
    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_jobs: 5,
            batch_size: 1000,
            checkpoint_interval_rows: 5_000,
            checkpoint_interval_duration: Duration::from_secs(60), // 1 minute
            shutdown_timeout: Duration::from_secs(60),
            ..Default::default()
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.max_concurrent_jobs == 0 {
            anyhow::bail!("max_concurrent_jobs must be > 0");
        }

        if self.batch_size == 0 {
            anyhow::bail!("batch_size must be > 0");
        }

        if self.batch_size > 100_000 {
            tracing::warn!(
                "Very large batch_size ({}), may cause memory issues",
                self.batch_size
            );
        }

        if self.checkpoint_interval_rows == 0 {
            anyhow::bail!("checkpoint_interval_rows must be > 0");
        }

        if !self.checkpoint_dir.exists() {
            tracing::warn!(
                "Checkpoint directory does not exist: {:?}",
                self.checkpoint_dir
            );
        }

        if !self.dlq_dir.exists() {
            tracing::warn!("DLQ directory does not exist: {:?}", self.dlq_dir);
        }

        Ok(())
    }
}

// ============================================================================
// Worker Configuration
// ============================================================================

/// Configuration for individual LoaderWorker instances
///
/// Passed to each worker task on spawn. Derived from LoaderJobConfig
/// plus job-specific parameters from the API request.
#[derive(Debug, Clone)]
pub struct LoaderWorkerConfig {
    /// DML execution mode (INSERT vs MERGE)
    pub dml_mode: DmlMode,

    /// Job identifier
    pub job_id: String,

    /// Source file path
    pub source_file: PathBuf,

    /// Target database table
    pub target_table: String,

    /// Batch processing size
    pub batch_size: usize,

    /// Checkpoint configuration
    pub checkpoint_config: super::super::checkpoint::CheckpointConfig,

    /// DLQ configuration
    pub dlq_config: super::super::dlq::DlqConfig,

    /// CSV reader buffer size (bytes)
    pub csv_buffer_size: usize,

    /// CSV delimiter
    pub csv_delimiter: u8,

    /// CSV has header row
    pub csv_has_header: bool,

    /// Maximum errors before aborting job
    pub max_errors: usize,

    /// Maximum retry attempts for transient errors
    pub max_retries: usize,

    /// Base delay for exponential backoff (milliseconds)
    pub retry_base_delay_ms: u64,
}

impl Default for LoaderWorkerConfig {
    fn default() -> Self {
        Self {
            dml_mode: DmlMode::default(),
            job_id: String::new(),
            source_file: PathBuf::new(),
            target_table: String::new(),
            batch_size: 1000,
            checkpoint_config: super::super::checkpoint::CheckpointConfig::default(),
            dlq_config: super::super::dlq::DlqConfig::default(),
            csv_buffer_size: 8 * 1024 * 1024, // 8MB
            csv_delimiter: b',',
            csv_has_header: true,
            max_errors: 10_000,
            max_retries: 3,
            retry_base_delay_ms: 100,
        }
    }
}

impl LoaderWorkerConfig {
    /// Calculate exponential backoff delay for retry attempt
    pub fn retry_delay(&self, attempt: usize) -> Duration {
        let delay_ms = self.retry_base_delay_ms * 2_u64.pow(attempt as u32);
        let max_delay_ms = 60_000; // Cap at 1 minute
        Duration::from_millis(delay_ms.min(max_delay_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dml_mode_default() {
        assert_eq!(DmlMode::default(), DmlMode::Insert);
    }

    #[test]
    fn test_dml_mode_requires_primary_keys() {
        assert!(!DmlMode::Insert.requires_primary_keys());
        assert!(DmlMode::Merge.requires_primary_keys());
    }

    #[test]
    fn test_dml_mode_description() {
        assert_eq!(
            DmlMode::Insert.description(),
            "INSERT statements (append-only)"
        );
        assert_eq!(
            DmlMode::Merge.description(),
            "MERGE statements (insert or update)"
        );
    }

    #[test]
    fn test_dml_mode_serde() {
        let insert = DmlMode::Insert;
        let json = serde_json::to_string(&insert).unwrap();
        assert_eq!(json, "\"Insert\"");

        let merge = DmlMode::Merge;
        let json = serde_json::to_string(&merge).unwrap();
        assert_eq!(json, "\"Merge\"");

        let deserialized: DmlMode = serde_json::from_str("\"Insert\"").unwrap();
        assert_eq!(deserialized, DmlMode::Insert);

        let deserialized: DmlMode = serde_json::from_str("\"Merge\"").unwrap();
        assert_eq!(deserialized, DmlMode::Merge);
    }

    #[test]
    fn test_default_config() {
        let config = LoaderJobConfig::default();
        assert_eq!(config.dml_mode, DmlMode::Insert);
        assert_eq!(config.max_concurrent_jobs, 10);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.checkpoint_interval_rows, 10_000);
        assert!(config.auto_resume);
    }

    #[test]
    fn test_worker_config_default() {
        let config = LoaderWorkerConfig::default();
        assert_eq!(config.dml_mode, DmlMode::Insert);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.max_errors, 10_000);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_high_throughput_config() {
        let config = LoaderJobConfig::high_throughput();
        assert_eq!(config.max_concurrent_jobs, 20);
        assert_eq!(config.batch_size, 5000);
        assert_eq!(config.checkpoint_interval_rows, 50_000);
    }

    #[test]
    fn test_low_memory_config() {
        let config = LoaderJobConfig::low_memory();
        assert_eq!(config.max_concurrent_jobs, 5);
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.checkpoint_interval_rows, 5_000);
    }

    #[test]
    fn test_config_validation() {
        let mut config = LoaderJobConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_jobs = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_jobs = 10;
        config.batch_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_retry_delay_exponential_backoff() {
        let config = LoaderWorkerConfig {
            retry_base_delay_ms: 100,
            ..Default::default()
        };

        assert_eq!(config.retry_delay(0), Duration::from_millis(100)); // 100 * 2^0
        assert_eq!(config.retry_delay(1), Duration::from_millis(200)); // 100 * 2^1
        assert_eq!(config.retry_delay(2), Duration::from_millis(400)); // 100 * 2^2
        assert_eq!(config.retry_delay(3), Duration::from_millis(800)); // 100 * 2^3

        // Should cap at 60 seconds
        assert_eq!(config.retry_delay(20), Duration::from_millis(60_000));
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = LoaderJobConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LoaderJobConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.max_concurrent_jobs, deserialized.max_concurrent_jobs);
        assert_eq!(config.batch_size, deserialized.batch_size);
        assert_eq!(config.auto_resume, deserialized.auto_resume);
    }
}
