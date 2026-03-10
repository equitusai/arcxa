// WAL Metrics Collection and Observability
//
// Production-grade metrics using Prometheus-compatible collectors

use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, register_int_counter_vec,
    register_int_gauge_vec, CounterVec, GaugeVec, HistogramVec, IntCounterVec, IntGaugeVec,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, trace};

use super::{WalError, WalMetricsSnapshot};

/// WAL metrics collector
#[derive(Debug)]
pub struct WalMetricsCollector {
    prefix: String,

    // Write metrics
    write_count: IntCounterVec,
    write_bytes: IntCounterVec,
    write_latency: HistogramVec,
    batch_write_count: IntCounterVec,
    batch_write_size: HistogramVec,

    // Sync metrics
    sync_count: IntCounterVec,
    sync_latency: HistogramVec,
    fsync_count: IntCounterVec,

    // Transaction metrics
    tx_begin: IntCounterVec,
    tx_commit: IntCounterVec,
    tx_abort: IntCounterVec,
    tx_active: IntGaugeVec,
    tx_duration: HistogramVec,

    // Recovery metrics
    recovery_count: IntCounterVec,
    recovery_duration: HistogramVec,
    recovery_entries: IntCounterVec,
    corruption_count: IntCounterVec,

    // Rotation/Compaction metrics
    rotation_count: IntCounterVec,
    compaction_count: IntCounterVec,
    compaction_bytes_reclaimed: IntCounterVec,
    segment_count: IntGaugeVec,
    segment_size_bytes: IntGaugeVec,

    // State metrics
    uncommitted_entries: IntGaugeVec,
    uncommitted_bytes: IntGaugeVec,
    current_lsn: IntGaugeVec,
    committed_lsn: IntGaugeVec,

    // Error metrics
    error_count: IntCounterVec,
    error_by_type: IntCounterVec,
}

impl WalMetricsCollector {
    /// Create new metrics collector
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),

            write_count: register_int_counter_vec!(
                format!("{}_write_total", prefix),
                "Total number of WAL writes",
                &["type"]
            )
            .unwrap(),

            write_bytes: register_int_counter_vec!(
                format!("{}_write_bytes_total", prefix),
                "Total bytes written to WAL",
                &["type"]
            )
            .unwrap(),

            write_latency: register_histogram_vec!(
                format!("{}_write_latency_seconds", prefix),
                "WAL write latency distribution",
                &["type"],
                vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
            )
            .unwrap(),

            batch_write_count: register_int_counter_vec!(
                format!("{}_batch_write_total", prefix),
                "Total number of batch writes",
                &["status"]
            )
            .unwrap(),

            batch_write_size: register_histogram_vec!(
                format!("{}_batch_write_size", prefix),
                "Batch write size distribution",
                &["type"],
                vec![1.0, 10.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0]
            )
            .unwrap(),

            sync_count: register_int_counter_vec!(
                format!("{}_sync_total", prefix),
                "Total number of sync operations",
                &["type"]
            )
            .unwrap(),

            sync_latency: register_histogram_vec!(
                format!("{}_sync_latency_seconds", prefix),
                "Sync operation latency",
                &["type"],
                vec![0.001, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
            )
            .unwrap(),

            fsync_count: register_int_counter_vec!(
                format!("{}_fsync_total", prefix),
                "Total number of fsync calls",
                &["mode"]
            )
            .unwrap(),

            tx_begin: register_int_counter_vec!(
                format!("{}_transaction_begin_total", prefix),
                "Total transactions started",
                &["type"]
            )
            .unwrap(),

            tx_commit: register_int_counter_vec!(
                format!("{}_transaction_commit_total", prefix),
                "Total transactions committed",
                &["type"]
            )
            .unwrap(),

            tx_abort: register_int_counter_vec!(
                format!("{}_transaction_abort_total", prefix),
                "Total transactions aborted",
                &["reason"]
            )
            .unwrap(),

            tx_active: register_int_gauge_vec!(
                format!("{}_transaction_active", prefix),
                "Number of active transactions",
                &["state"]
            )
            .unwrap(),

            tx_duration: register_histogram_vec!(
                format!("{}_transaction_duration_seconds", prefix),
                "Transaction duration distribution",
                &["outcome"],
                vec![0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0]
            )
            .unwrap(),

            recovery_count: register_int_counter_vec!(
                format!("{}_recovery_total", prefix),
                "Total recovery operations",
                &["status"]
            )
            .unwrap(),

            recovery_duration: register_histogram_vec!(
                format!("{}_recovery_duration_seconds", prefix),
                "Recovery operation duration",
                &["type"],
                vec![0.1, 1.0, 10.0, 60.0, 300.0, 600.0]
            )
            .unwrap(),

            recovery_entries: register_int_counter_vec!(
                format!("{}_recovery_entries_total", prefix),
                "Total entries recovered",
                &["status"]
            )
            .unwrap(),

            corruption_count: register_int_counter_vec!(
                format!("{}_corruption_total", prefix),
                "Total corruption events detected",
                &["severity"]
            )
            .unwrap(),

            rotation_count: register_int_counter_vec!(
                format!("{}_rotation_total", prefix),
                "Total segment rotations",
                &["trigger"]
            )
            .unwrap(),

            compaction_count: register_int_counter_vec!(
                format!("{}_compaction_total", prefix),
                "Total compaction operations",
                &["status"]
            )
            .unwrap(),

            compaction_bytes_reclaimed: register_int_counter_vec!(
                format!("{}_compaction_bytes_reclaimed_total", prefix),
                "Total bytes reclaimed by compaction",
                &["type"]
            )
            .unwrap(),

            segment_count: register_int_gauge_vec!(
                format!("{}_segment_count", prefix),
                "Current number of segments",
                &["state"]
            )
            .unwrap(),

            segment_size_bytes: register_int_gauge_vec!(
                format!("{}_segment_size_bytes", prefix),
                "Current segment sizes",
                &["segment_id"]
            )
            .unwrap(),

            uncommitted_entries: register_int_gauge_vec!(
                format!("{}_uncommitted_entries", prefix),
                "Number of uncommitted entries",
                &["priority"]
            )
            .unwrap(),

            uncommitted_bytes: register_int_gauge_vec!(
                format!("{}_uncommitted_bytes", prefix),
                "Bytes of uncommitted data",
                &["priority"]
            )
            .unwrap(),

            current_lsn: register_int_gauge_vec!(
                format!("{}_current_lsn", prefix),
                "Current log sequence number",
                &["type"]
            )
            .unwrap(),

            committed_lsn: register_int_gauge_vec!(
                format!("{}_committed_lsn", prefix),
                "Last committed log sequence number",
                &["type"]
            )
            .unwrap(),

            error_count: register_int_counter_vec!(
                format!("{}_error_total", prefix),
                "Total errors encountered",
                &["severity"]
            )
            .unwrap(),

            error_by_type: register_int_counter_vec!(
                format!("{}_error_by_type_total", prefix),
                "Errors by type",
                &["type", "retryable"]
            )
            .unwrap(),
        }
    }

    /// Record a write operation
    pub fn record_write(&self, bytes: usize, latency: Duration) {
        self.write_count.with_label_values(&["single"]).inc();
        self.write_bytes
            .with_label_values(&["single"])
            .inc_by(bytes as u64);
        self.write_latency
            .with_label_values(&["single"])
            .observe(latency.as_secs_f64());

        trace!(
            bytes = bytes,
            latency_us = latency.as_micros(),
            "WAL write recorded"
        );
    }

    /// Record a batch write operation
    pub fn record_batch_write(&self, count: usize, bytes: usize, latency: Duration) {
        self.batch_write_count.with_label_values(&["success"]).inc();
        self.batch_write_size
            .with_label_values(&["entries"])
            .observe(count as f64);
        self.write_bytes
            .with_label_values(&["batch"])
            .inc_by(bytes as u64);
        self.write_latency
            .with_label_values(&["batch"])
            .observe(latency.as_secs_f64());

        debug!(
            entries = count,
            bytes = bytes,
            latency_us = latency.as_micros(),
            "WAL batch write recorded"
        );
    }

    /// Record a sync operation
    pub fn record_sync_latency(&self, latency: Duration) {
        self.sync_count.with_label_values(&["manual"]).inc();
        self.sync_latency
            .with_label_values(&["manual"])
            .observe(latency.as_secs_f64());
    }

    /// Record fsync
    pub fn record_fsync(&self, mode: &str) {
        self.fsync_count.with_label_values(&[mode]).inc();
    }

    /// Record transaction begin
    pub fn record_tx_begin(&self) {
        self.tx_begin.with_label_values(&["user"]).inc();
        self.tx_active.with_label_values(&["active"]).inc();
    }

    /// Record transaction commit
    pub fn record_tx_commit(&self, duration: Duration) {
        self.tx_commit.with_label_values(&["success"]).inc();
        self.tx_active.with_label_values(&["active"]).dec();
        self.tx_duration
            .with_label_values(&["commit"])
            .observe(duration.as_secs_f64());
    }

    /// Record transaction abort
    pub fn record_tx_abort(&self, reason: &str, duration: Duration) {
        self.tx_abort.with_label_values(&[reason]).inc();
        self.tx_active.with_label_values(&["active"]).dec();
        self.tx_duration
            .with_label_values(&["abort"])
            .observe(duration.as_secs_f64());
    }

    /// Record commit
    pub fn record_commit(&self) {
        self.sync_count.with_label_values(&["commit"]).inc();
    }

    /// Record batch commit
    pub fn record_commit_batch(&self, count: usize) {
        self.batch_write_count.with_label_values(&["commit"]).inc();
        self.batch_write_size
            .with_label_values(&["commits"])
            .observe(count as f64);
    }

    /// Record recovery
    pub fn record_recovery(&self, entries: u64, corrupted: u64, duration: Duration) {
        self.recovery_count
            .with_label_values(&[if corrupted > 0 { "partial" } else { "full" }])
            .inc();
        self.recovery_entries
            .with_label_values(&["valid"])
            .inc_by(entries);
        self.recovery_entries
            .with_label_values(&["corrupted"])
            .inc_by(corrupted);
        self.recovery_duration
            .with_label_values(&["full"])
            .observe(duration.as_secs_f64());

        if corrupted > 0 {
            self.corruption_count
                .with_label_values(&["recoverable"])
                .inc_by(corrupted);
        }
    }

    /// Record rotation
    pub fn record_rotation(&self) {
        self.rotation_count.with_label_values(&["automatic"]).inc();
    }

    /// Record compaction
    pub fn record_compaction(&self, bytes_reclaimed: u64, duration: Duration) {
        self.compaction_count.with_label_values(&["success"]).inc();
        self.compaction_bytes_reclaimed
            .with_label_values(&["automatic"])
            .inc_by(bytes_reclaimed);
        self.sync_latency
            .with_label_values(&["compaction"])
            .observe(duration.as_secs_f64());
    }

    /// Record error
    pub fn record_error(&self, operation: &str, error: &WalError) {
        let labels = error.to_metrics_labels();
        let error_type = labels
            .iter()
            .find(|(k, _)| *k == "error_type")
            .map(|(_, v)| v.as_str())
            .unwrap_or("unknown");
        let retryable = labels
            .iter()
            .find(|(k, _)| *k == "retryable")
            .map(|(_, v)| v.as_str())
            .unwrap_or("false");

        self.error_count.with_label_values(&[operation]).inc();
        self.error_by_type
            .with_label_values(&[error_type, retryable])
            .inc();

        debug!(
            operation = operation,
            error_type = error_type,
            retryable = retryable,
            "WAL error recorded"
        );
    }

    /// Update LSN gauges
    pub fn update_lsn(&self, current: u64, committed: u64) {
        self.current_lsn
            .with_label_values(&["tail"])
            .set(current as i64);
        self.committed_lsn
            .with_label_values(&["head"])
            .set(committed as i64);

        let uncommitted = current.saturating_sub(committed);
        self.uncommitted_entries
            .with_label_values(&["normal"])
            .set(uncommitted as i64);
    }

    /// Update segment metrics
    pub fn update_segments(&self, active: usize, archived: usize, total_bytes: u64) {
        self.segment_count
            .with_label_values(&["active"])
            .set(active as i64);
        self.segment_count
            .with_label_values(&["archived"])
            .set(archived as i64);
        self.segment_size_bytes
            .with_label_values(&["total"])
            .set(total_bytes as i64);
    }

    /// Get metrics snapshot
    pub fn snapshot(&self) -> WalMetricsSnapshot {
        // In a real implementation, would query Prometheus metrics
        WalMetricsSnapshot {
            total_writes: 0,
            total_bytes: 0,
            uncommitted_entries: 0,
            uncommitted_bytes: 0,
            active_transactions: 0,
            rotation_count: 0,
            compaction_count: 0,
            recovery_count: 0,
            corruption_events: 0,
            avg_write_latency_us: 0,
            p99_write_latency_us: 0,
            sync_count: 0,
            avg_sync_latency_ms: 0,
        }
    }
}

/// WAL metrics for monitoring
#[derive(Debug, Clone)]
pub struct WalMetrics {
    collector: Arc<WalMetricsCollector>,
}

impl WalMetrics {
    pub fn new(prefix: &str) -> Self {
        Self {
            collector: Arc::new(WalMetricsCollector::new(prefix)),
        }
    }

    pub fn collector(&self) -> &WalMetricsCollector {
        &self.collector
    }
}
