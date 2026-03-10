//! # Prometheus Metrics for Async Governance Brain
//!
//! Exposes ProcessorMetrics as Prometheus metrics for production monitoring.
//! Tracks throughput, latency, errors, and batch efficiency.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec, Opts,
};

lazy_static! {
    /// Events processed counter (by status: success/failed)
    pub static ref GOVERNANCE_EVENTS_PROCESSED: CounterVec = register_counter_vec!(
        Opts::new("governance_events_processed_total", "Total lineage events processed by governance brain"),
        &["processor_id", "status"]  // status: success/failed
    ).unwrap();

    /// Batch processing metrics
    pub static ref GOVERNANCE_BATCHES: CounterVec = register_counter_vec!(
        Opts::new("governance_batches_total", "Total batches processed by governance brain"),
        &["processor_id"]
    ).unwrap();

    pub static ref GOVERNANCE_BATCH_SIZE: HistogramVec = register_histogram_vec!(
        "governance_batch_size",
        "Distribution of batch sizes in governance brain",
        &["processor_id"],
        vec![1.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2000.0]
    ).unwrap();

    /// Latency metrics
    pub static ref GOVERNANCE_BATCH_DURATION: HistogramVec = register_histogram_vec!(
        "governance_batch_duration_seconds",
        "Time to process a batch in governance brain",
        &["processor_id"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    ).unwrap();

    /// Current state gauges
    pub static ref GOVERNANCE_MODE: GaugeVec = register_gauge_vec!(
        "governance_mode",
        "Current governance brain mode (1=async, 0=sync)",
        &["instance"]
    ).unwrap();

    pub static ref GOVERNANCE_QUEUE_DEPTH: GaugeVec = register_gauge_vec!(
        "governance_queue_depth",
        "Current number of events in governance processing queue",
        &["processor_id"]
    ).unwrap();

    /// Success rate gauge (computed)
    pub static ref GOVERNANCE_SUCCESS_RATE: GaugeVec = register_gauge_vec!(
        "governance_success_rate_percent",
        "Current success rate percentage",
        &["processor_id"]
    ).unwrap();
}

/// Update Prometheus metrics from ProcessorMetrics
///
/// Called periodically to export async brain metrics to Prometheus.
pub fn update_from_processor_metrics(
    metrics: &crate::governance::ProcessorMetrics,
    processor_id: &str,
) {
    // Update event counters
    GOVERNANCE_EVENTS_PROCESSED
        .with_label_values(&[processor_id, "success"])
        .inc_by(metrics.processed_events as f64);

    GOVERNANCE_EVENTS_PROCESSED
        .with_label_values(&[processor_id, "failed"])
        .inc_by(metrics.failed_events as f64);

    // Update batch counter
    GOVERNANCE_BATCHES
        .with_label_values(&[processor_id])
        .inc_by(metrics.batches_processed as f64);

    // Update batch size histogram (if we have data)
    if metrics.avg_batch_size > 0.0 {
        GOVERNANCE_BATCH_SIZE
            .with_label_values(&[processor_id])
            .observe(metrics.avg_batch_size);
    }

    // Update latency histogram (convert ms to seconds)
    if metrics.last_flush_ms > 0 {
        GOVERNANCE_BATCH_DURATION
            .with_label_values(&[processor_id])
            .observe(metrics.last_flush_ms as f64 / 1000.0);
    }

    // Calculate and update success rate
    let total = metrics.processed_events + metrics.failed_events;
    if total > 0 {
        let success_rate = (metrics.processed_events as f64 / total as f64) * 100.0;
        GOVERNANCE_SUCCESS_RATE
            .with_label_values(&[processor_id])
            .set(success_rate);
    }
}

/// Set governance mode gauge (1=async, 0=sync)
pub fn set_governance_mode(is_async: bool, instance: &str) {
    let mode_value = if is_async { 1.0 } else { 0.0 };
    GOVERNANCE_MODE
        .with_label_values(&[instance])
        .set(mode_value);
}

/// Update queue depth gauge
pub fn set_queue_depth(depth: usize, processor_id: &str) {
    GOVERNANCE_QUEUE_DEPTH
        .with_label_values(&[processor_id])
        .set(depth as f64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::ProcessorMetrics;

    #[test]
    fn test_metrics_update() {
        let mut metrics = ProcessorMetrics::new();
        metrics.processed_events = 1000;
        metrics.failed_events = 50;
        metrics.batches_processed = 20;
        metrics.avg_batch_size = 52.5;
        metrics.last_flush_ms = 150;

        // Update Prometheus metrics
        update_from_processor_metrics(&metrics, "test_processor");

        // Verify mode setting
        set_governance_mode(true, "test_instance");
        set_governance_mode(false, "test_instance");

        // Verify queue depth
        set_queue_depth(500, "test_processor");
    }
}
