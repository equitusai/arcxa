//! ETL Loader metrics
//!
//! Tracks ETL loader performance and reliability:
//! - Job lifecycle (created, started, completed, failed)
//! - Data throughput (rows/sec, bytes/sec)
//! - Error rates by category
//! - Checkpoint operations
//! - DLQ statistics
//! - Transformation performance

use anyhow::Result;
use prometheus::{
    exponential_buckets, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry,
};

/// ETL Loader metrics
///
/// Monitors loader job execution, throughput, errors, and reliability.
pub struct LoaderMetrics {
    // Job lifecycle
    jobs_total: IntCounterVec,
    jobs_active: IntGauge,
    job_duration_seconds: HistogramVec,

    // Data throughput
    rows_processed_total: IntCounterVec,
    rows_failed_total: IntCounterVec,
    rows_per_second: HistogramVec,
    bytes_processed_total: IntCounterVec,

    // Errors
    errors_total: IntCounterVec,
    errors_retried_total: IntCounterVec,
    circuit_breaker_state: IntGaugeVec,

    // Checkpoints
    checkpoints_total: IntCounterVec,
    checkpoint_duration_seconds: HistogramVec,

    // DLQ
    dlq_rows_total: IntCounterVec,
    dlq_files_total: IntGaugeVec,

    // Transformations
    transformations_total: IntCounterVec,
    transformation_duration_seconds: HistogramVec,

    // DB2 LOAD utility
    load_operations_total: IntCounterVec,
    load_duration_seconds: HistogramVec,
    load_rows_per_second: HistogramVec,
}

impl LoaderMetrics {
    /// Create and register loader metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        // Job lifecycle metrics
        let jobs_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_jobs_total",
                "Total number of loader jobs by status",
            ),
            &["status"], // created, completed, failed, cancelled
        )?;

        let jobs_active = IntGauge::new(
            "graphica_loader_jobs_active",
            "Current number of active loader jobs",
        )?;

        let job_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_job_duration_seconds",
                "Job execution duration in seconds",
            )
            .buckets(exponential_buckets(1.0, 2.0, 15)?), // 1s to ~4.5 hours
            &["job_type", "status"],
        )?;

        // Data throughput metrics
        let rows_processed_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_rows_processed_total",
                "Total rows successfully processed",
            ),
            &["job_id", "table"],
        )?;

        let rows_failed_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_rows_failed_total",
                "Total rows that failed processing",
            ),
            &["job_id", "error_category"],
        )?;

        let rows_per_second = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_rows_per_second",
                "Row processing throughput",
            )
            .buckets(exponential_buckets(10.0, 2.0, 12)?), // 10 to ~40k rows/sec
            &["job_id"],
        )?;

        let bytes_processed_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_bytes_processed_total",
                "Total bytes processed",
            ),
            &["job_id", "table"],
        )?;

        // Error metrics
        let errors_total = IntCounterVec::new(
            Opts::new("graphica_loader_errors_total", "Total errors by category"),
            &["job_id", "category", "is_transient"],
        )?;

        let errors_retried_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_errors_retried_total",
                "Total retry attempts by error category",
            ),
            &["job_id", "category"],
        )?;

        let circuit_breaker_state = IntGaugeVec::new(
            Opts::new(
                "graphica_loader_circuit_breaker_state",
                "Circuit breaker state (0=closed, 1=open, 2=half_open)",
            ),
            &["job_id"],
        )?;

        // Checkpoint metrics
        let checkpoints_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_checkpoints_total",
                "Total checkpoints created",
            ),
            &["job_id"],
        )?;

        let checkpoint_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_checkpoint_duration_seconds",
                "Checkpoint operation duration in seconds",
            )
            .buckets(exponential_buckets(0.01, 2.0, 10)?), // 10ms to ~10s
            &["job_id"],
        )?;

        // DLQ metrics
        let dlq_rows_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_dlq_rows_total",
                "Total rows written to DLQ by category",
            ),
            &["job_id", "error_category"],
        )?;

        let dlq_files_total = IntGaugeVec::new(
            Opts::new(
                "graphica_loader_dlq_files_total",
                "Total number of DLQ files",
            ),
            &["job_id", "format"],
        )?;

        // Transformation metrics
        let transformations_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_transformations_total",
                "Total transformations executed",
            ),
            &["job_id", "function"],
        )?;

        let transformation_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_transformation_duration_seconds",
                "Transformation execution duration in seconds",
            )
            .buckets(exponential_buckets(0.00001, 2.0, 15)?), // 10µs to ~160ms
            &["function"],
        )?;

        // DB2 LOAD utility metrics
        let load_operations_total = IntCounterVec::new(
            Opts::new(
                "graphica_loader_db2_load_operations_total",
                "Total DB2 LOAD operations by status",
            ),
            &["job_id", "status"],
        )?;

        let load_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_db2_load_duration_seconds",
                "DB2 LOAD operation duration in seconds",
            )
            .buckets(exponential_buckets(1.0, 2.0, 15)?), // 1s to ~4.5 hours
            &["job_id"],
        )?;

        let load_rows_per_second = HistogramVec::new(
            HistogramOpts::new(
                "graphica_loader_db2_load_rows_per_second",
                "DB2 LOAD throughput (rows/sec)",
            )
            .buckets(exponential_buckets(100.0, 2.0, 15)?), // 100 to ~3.2M rows/sec
            &["job_id"],
        )?;

        // Register all metrics
        registry.register(Box::new(jobs_total.clone()))?;
        registry.register(Box::new(jobs_active.clone()))?;
        registry.register(Box::new(job_duration_seconds.clone()))?;
        registry.register(Box::new(rows_processed_total.clone()))?;
        registry.register(Box::new(rows_failed_total.clone()))?;
        registry.register(Box::new(rows_per_second.clone()))?;
        registry.register(Box::new(bytes_processed_total.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        registry.register(Box::new(errors_retried_total.clone()))?;
        registry.register(Box::new(circuit_breaker_state.clone()))?;
        registry.register(Box::new(checkpoints_total.clone()))?;
        registry.register(Box::new(checkpoint_duration_seconds.clone()))?;
        registry.register(Box::new(dlq_rows_total.clone()))?;
        registry.register(Box::new(dlq_files_total.clone()))?;
        registry.register(Box::new(transformations_total.clone()))?;
        registry.register(Box::new(transformation_duration_seconds.clone()))?;
        registry.register(Box::new(load_operations_total.clone()))?;
        registry.register(Box::new(load_duration_seconds.clone()))?;
        registry.register(Box::new(load_rows_per_second.clone()))?;

        Ok(Self {
            jobs_total,
            jobs_active,
            job_duration_seconds,
            rows_processed_total,
            rows_failed_total,
            rows_per_second,
            bytes_processed_total,
            errors_total,
            errors_retried_total,
            circuit_breaker_state,
            checkpoints_total,
            checkpoint_duration_seconds,
            dlq_rows_total,
            dlq_files_total,
            transformations_total,
            transformation_duration_seconds,
            load_operations_total,
            load_duration_seconds,
            load_rows_per_second,
        })
    }

    // ========================================================================
    // Job Lifecycle Methods
    // ========================================================================

    /// Record job creation
    pub fn job_created(&self) {
        self.jobs_total.with_label_values(&["created"]).inc();
        self.jobs_active.inc();
    }

    /// Record job start
    pub fn job_started(&self) {
        self.jobs_total.with_label_values(&["started"]).inc();
    }

    /// Record job completion
    pub fn job_completed(&self, job_type: &str, duration_secs: f64) {
        self.jobs_total.with_label_values(&["completed"]).inc();
        self.jobs_active.dec();
        self.job_duration_seconds
            .with_label_values(&[job_type, "completed"])
            .observe(duration_secs);
    }

    /// Record job failure
    pub fn job_failed(&self, job_type: &str, duration_secs: f64) {
        self.jobs_total.with_label_values(&["failed"]).inc();
        self.jobs_active.dec();
        self.job_duration_seconds
            .with_label_values(&[job_type, "failed"])
            .observe(duration_secs);
    }

    /// Record job cancellation
    pub fn job_cancelled(&self) {
        self.jobs_total.with_label_values(&["cancelled"]).inc();
        self.jobs_active.dec();
    }

    // ========================================================================
    // Data Throughput Methods
    // ========================================================================

    /// Record rows processed
    pub fn rows_processed(&self, job_id: &str, table: &str, count: u64) {
        self.rows_processed_total
            .with_label_values(&[job_id, table])
            .inc_by(count);
    }

    /// Record rows failed
    pub fn rows_failed(&self, job_id: &str, error_category: &str, count: u64) {
        self.rows_failed_total
            .with_label_values(&[job_id, error_category])
            .inc_by(count);
    }

    /// Record throughput
    pub fn record_throughput(&self, job_id: &str, rows_per_sec: f64) {
        self.rows_per_second
            .with_label_values(&[job_id])
            .observe(rows_per_sec);
    }

    /// Record bytes processed
    pub fn bytes_processed(&self, job_id: &str, table: &str, bytes: u64) {
        self.bytes_processed_total
            .with_label_values(&[job_id, table])
            .inc_by(bytes);
    }

    // ========================================================================
    // Error Tracking Methods
    // ========================================================================

    /// Record error occurrence
    pub fn error_occurred(&self, job_id: &str, category: &str, is_transient: bool) {
        self.errors_total
            .with_label_values(&[
                job_id,
                category,
                if is_transient { "true" } else { "false" },
            ])
            .inc();
    }

    /// Record retry attempt
    pub fn error_retried(&self, job_id: &str, category: &str) {
        self.errors_retried_total
            .with_label_values(&[job_id, category])
            .inc();
    }

    /// Update circuit breaker state
    /// state: 0=closed, 1=open, 2=half_open
    pub fn set_circuit_breaker_state(&self, job_id: &str, state: i64) {
        self.circuit_breaker_state
            .with_label_values(&[job_id])
            .set(state);
    }

    // ========================================================================
    // Checkpoint Methods
    // ========================================================================

    /// Record checkpoint creation
    pub fn checkpoint_created(&self, job_id: &str, duration_secs: f64) {
        self.checkpoints_total.with_label_values(&[job_id]).inc();
        self.checkpoint_duration_seconds
            .with_label_values(&[job_id])
            .observe(duration_secs);
    }

    // ========================================================================
    // DLQ Methods
    // ========================================================================

    /// Record rows written to DLQ
    pub fn dlq_row_written(&self, job_id: &str, error_category: &str, count: u64) {
        self.dlq_rows_total
            .with_label_values(&[job_id, error_category])
            .inc_by(count);
    }

    /// Update DLQ file count
    pub fn set_dlq_files(&self, job_id: &str, format: &str, count: i64) {
        self.dlq_files_total
            .with_label_values(&[job_id, format])
            .set(count);
    }

    // ========================================================================
    // Transformation Methods
    // ========================================================================

    /// Record transformation execution
    pub fn transformation_executed(&self, job_id: &str, function: &str) {
        self.transformations_total
            .with_label_values(&[job_id, function])
            .inc();
    }

    /// Record transformation duration
    pub fn transformation_duration(&self, function: &str, duration_secs: f64) {
        self.transformation_duration_seconds
            .with_label_values(&[function])
            .observe(duration_secs);
    }

    // ========================================================================
    // DB2 LOAD Methods
    // ========================================================================

    /// Record DB2 LOAD operation
    pub fn load_operation(&self, job_id: &str, status: &str, duration_secs: f64, rows: u64) {
        self.load_operations_total
            .with_label_values(&[job_id, status])
            .inc();

        self.load_duration_seconds
            .with_label_values(&[job_id])
            .observe(duration_secs);

        if duration_secs > 0.0 {
            let rows_per_sec = rows as f64 / duration_secs;
            self.load_rows_per_second
                .with_label_values(&[job_id])
                .observe(rows_per_sec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_loader_metrics_creation() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).expect("Failed to create loader metrics");

        // Verify metrics can be used
        metrics.job_created();
        metrics.job_started();

        // Gather and verify output
        let families = registry.gather();
        assert!(!families.is_empty(), "Should have registered metrics");
    }

    #[test]
    fn test_job_lifecycle_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.job_created();
        metrics.job_started();
        metrics.job_completed("csv_to_db2", 120.5);

        let families = registry.gather();
        let jobs_total = families
            .iter()
            .find(|f| f.name() == "graphica_loader_jobs_total")
            .expect("jobs_total metric should exist");

        assert!(!jobs_total.get_metric().is_empty());
    }

    #[test]
    fn test_throughput_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.rows_processed("job_123", "customers", 10000);
        metrics.record_throughput("job_123", 450.0);
        metrics.bytes_processed("job_123", "customers", 1024000);

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_rows_processed_total"));
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_rows_per_second"));
    }

    #[test]
    fn test_error_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.error_occurred("job_123", "DataFormat", false);
        metrics.error_occurred("job_123", "Timeout", true);
        metrics.error_retried("job_123", "Timeout");

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_errors_total"));
    }

    #[test]
    fn test_checkpoint_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.checkpoint_created("job_123", 0.025);

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_checkpoints_total"));
    }

    #[test]
    fn test_dlq_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.dlq_row_written("job_123", "DataFormat", 50);
        metrics.set_dlq_files("job_123", "jsonlines", 3);

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_dlq_rows_total"));
    }

    #[test]
    fn test_transformation_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.transformation_executed("job_123", "UPPER");
        metrics.transformation_duration("UPPER", 0.000015);

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_transformations_total"));
    }

    #[test]
    fn test_load_operation_metrics() {
        let registry = Registry::new();
        let metrics = LoaderMetrics::new(&registry).unwrap();

        metrics.load_operation("job_123", "success", 45.0, 100000);

        let families = registry.gather();
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_db2_load_operations_total"));
        assert!(families
            .iter()
            .any(|f| f.name() == "graphica_loader_db2_load_rows_per_second"));
    }
}
