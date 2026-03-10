//! Workflow Storage Metrics
//!
//! Prometheus metrics for monitoring workflow execution storage performance:
//! - ExecutionStore operation latency and throughput
//! - Backend-specific metrics (InMemory, RocksDB)
//! - Storage size and capacity tracking
//! - Error rates and failure modes

use anyhow::Result;
use prometheus::{
    exponential_buckets, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec,
    IntGauge, IntGaugeVec, Opts, Registry,
};

/// Workflow storage metrics
///
/// Monitors ExecutionStore operations, backend performance, and storage health.
pub struct WorkflowStorageMetrics {
    // ExecutionStore operation metrics
    store_operations_total: IntCounterVec,
    store_operation_duration_seconds: HistogramVec,
    store_operations_in_flight: IntGaugeVec,

    // Storage size metrics
    executions_stored_total: IntGaugeVec,
    execution_logs_total: IntGaugeVec,
    storage_size_bytes: IntGaugeVec,

    // Error metrics
    store_errors_total: IntCounterVec,
    store_lock_contentions_total: IntCounterVec,

    // Performance metrics
    list_query_size: HistogramVec,
    execution_size_bytes: Histogram,
    log_batch_size: Histogram,

    // Backend health
    backend_health_status: IntGaugeVec,
}

impl WorkflowStorageMetrics {
    /// Create and register workflow storage metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        // ExecutionStore operation metrics
        let store_operations_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_store_operations_total",
                "Total workflow store operations by type and status",
            ),
            &["operation", "backend", "status"],
        )?;

        let store_operation_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_store_operation_duration_seconds",
                "Workflow store operation latency in seconds",
            )
            .buckets(exponential_buckets(0.0001, 2.0, 14)?), // 0.1ms to ~1.6s
            &["operation", "backend"],
        )?;

        let store_operations_in_flight = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_store_operations_in_flight",
                "Current number of workflow store operations in progress",
            ),
            &["operation", "backend"],
        )?;

        // Storage size metrics
        let executions_stored_total = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_executions_stored_total",
                "Total number of workflow executions stored",
            ),
            &["backend", "status"],
        )?;

        let execution_logs_total = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_execution_logs_total",
                "Total number of execution logs stored",
            ),
            &["backend"],
        )?;

        let storage_size_bytes = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_storage_size_bytes",
                "Estimated storage size in bytes",
            ),
            &["backend", "component"],
        )?;

        // Error metrics
        let store_errors_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_store_errors_total",
                "Total workflow store errors by type",
            ),
            &["operation", "backend", "error_type"],
        )?;

        let store_lock_contentions_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_store_lock_contentions_total",
                "Total lock contentions in workflow storage",
            ),
            &["backend", "resource"],
        )?;

        // Performance metrics
        let list_query_size = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_store_list_query_size",
                "Number of executions returned by list queries",
            )
            .buckets(vec![
                1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0,
            ]),
            &["query_type", "backend"],
        )?;

        let execution_size_bytes = Histogram::with_opts(
            HistogramOpts::new(
                "graphica_workflow_execution_size_bytes",
                "Size of workflow execution records in bytes",
            )
            .buckets(exponential_buckets(100.0, 2.0, 12)?), // 100B to ~400KB
        )?;

        let log_batch_size = Histogram::with_opts(
            HistogramOpts::new(
                "graphica_workflow_log_batch_size",
                "Number of logs in a single append operation",
            )
            .buckets(vec![1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0]),
        )?;

        // Backend health
        let backend_health_status = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_backend_health_status",
                "Workflow storage backend health (1=healthy, 0=unhealthy)",
            ),
            &["backend"],
        )?;

        // Register all metrics
        registry.register(Box::new(store_operations_total.clone()))?;
        registry.register(Box::new(store_operation_duration_seconds.clone()))?;
        registry.register(Box::new(store_operations_in_flight.clone()))?;

        registry.register(Box::new(executions_stored_total.clone()))?;
        registry.register(Box::new(execution_logs_total.clone()))?;
        registry.register(Box::new(storage_size_bytes.clone()))?;

        registry.register(Box::new(store_errors_total.clone()))?;
        registry.register(Box::new(store_lock_contentions_total.clone()))?;

        registry.register(Box::new(list_query_size.clone()))?;
        registry.register(Box::new(execution_size_bytes.clone()))?;
        registry.register(Box::new(log_batch_size.clone()))?;

        registry.register(Box::new(backend_health_status.clone()))?;

        Ok(Self {
            store_operations_total,
            store_operation_duration_seconds,
            store_operations_in_flight,
            executions_stored_total,
            execution_logs_total,
            storage_size_bytes,
            store_errors_total,
            store_lock_contentions_total,
            list_query_size,
            execution_size_bytes,
            log_batch_size,
            backend_health_status,
        })
    }

    // Operation tracking

    /// Record operation start
    pub fn operation_started(&self, operation: &str, backend: &str) {
        self.store_operations_in_flight
            .with_label_values(&[operation, backend])
            .inc();
    }

    /// Record operation completion
    pub fn record_operation(
        &self,
        operation: &str,
        backend: &str,
        status: &str,
        duration_secs: f64,
    ) {
        self.store_operations_in_flight
            .with_label_values(&[operation, backend])
            .dec();

        self.store_operations_total
            .with_label_values(&[operation, backend, status])
            .inc();

        self.store_operation_duration_seconds
            .with_label_values(&[operation, backend])
            .observe(duration_secs);
    }

    /// Record operation error
    pub fn record_error(&self, operation: &str, backend: &str, error_type: &str) {
        self.store_errors_total
            .with_label_values(&[operation, backend, error_type])
            .inc();
    }

    // Storage size tracking

    /// Update executions count gauge
    pub fn update_executions_count(&self, backend: &str, status: &str, count: i64) {
        self.executions_stored_total
            .with_label_values(&[backend, status])
            .set(count);
    }

    /// Update logs count gauge
    pub fn update_logs_count(&self, backend: &str, count: i64) {
        self.execution_logs_total
            .with_label_values(&[backend])
            .set(count);
    }

    /// Update storage size estimate
    pub fn update_storage_size(&self, backend: &str, component: &str, size_bytes: i64) {
        self.storage_size_bytes
            .with_label_values(&[backend, component])
            .set(size_bytes);
    }

    // Performance metrics

    /// Record list query result size
    pub fn record_list_query(&self, query_type: &str, backend: &str, result_count: usize) {
        self.list_query_size
            .with_label_values(&[query_type, backend])
            .observe(result_count as f64);
    }

    /// Record execution record size
    pub fn record_execution_size(&self, size_bytes: usize) {
        self.execution_size_bytes.observe(size_bytes as f64);
    }

    /// Record log batch size
    pub fn record_log_batch(&self, log_count: usize) {
        self.log_batch_size.observe(log_count as f64);
    }

    // Lock contention tracking

    /// Record lock contention event
    pub fn record_lock_contention(&self, backend: &str, resource: &str) {
        self.store_lock_contentions_total
            .with_label_values(&[backend, resource])
            .inc();
    }

    // Backend health

    /// Set backend health status
    pub fn set_backend_health(&self, backend: &str, healthy: bool) {
        self.backend_health_status
            .with_label_values(&[backend])
            .set(if healthy { 1 } else { 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_storage_metrics_creation() {
        let registry = Registry::new();
        let result = WorkflowStorageMetrics::new(&registry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_operation_tracking() {
        let registry = Registry::new();
        let metrics = WorkflowStorageMetrics::new(&registry).unwrap();

        metrics.operation_started("save", "in_memory");
        metrics.record_operation("save", "in_memory", "success", 0.001);
        metrics.record_error("get", "in_memory", "not_found");

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_storage_size_tracking() {
        let registry = Registry::new();
        let metrics = WorkflowStorageMetrics::new(&registry).unwrap();

        metrics.update_executions_count("in_memory", "pending", 42);
        metrics.update_logs_count("in_memory", 156);
        metrics.update_storage_size("in_memory", "executions", 1024 * 1024);

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_performance_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowStorageMetrics::new(&registry).unwrap();

        metrics.record_list_query("by_workflow", "in_memory", 25);
        metrics.record_execution_size(4096);
        metrics.record_log_batch(10);

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_backend_health() {
        let registry = Registry::new();
        let metrics = WorkflowStorageMetrics::new(&registry).unwrap();

        metrics.set_backend_health("in_memory", true);
        metrics.set_backend_health("rocksdb", false);

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_lock_contention() {
        let registry = Registry::new();
        let metrics = WorkflowStorageMetrics::new(&registry).unwrap();

        metrics.record_lock_contention("in_memory", "executions");
        metrics.record_lock_contention("in_memory", "logs");

        // Metrics should be recorded without panics
    }
}
