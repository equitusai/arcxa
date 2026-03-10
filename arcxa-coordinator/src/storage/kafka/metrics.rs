//! Centralized Prometheus metrics for Kafka durability components
//!
//! This module provides a unified metrics interface for all Kafka durability
//! components including the durable sink, acknowledgment tracker, circuit breaker,
//! and replay manager.
//!
//! # Metrics Exposed
//!
//! ## Durable Sink Metrics
//! - `kafka_lineage_writes_total` - Total lineage events written
//! - `kafka_lineage_write_errors_total` - Total write errors
//! - `kafka_wal_writes_total` - Total WAL writes
//! - `kafka_wal_write_duration_seconds` - WAL write latency
//! - `kafka_send_duration_seconds` - Kafka send latency
//!
//! ## Acknowledgment Tracker Metrics
//! - `kafka_pending_acks` - Current pending acknowledgments
//! - `kafka_acknowledged_total` - Total acknowledged events
//! - `kafka_acks_failed_total` - Total failed acknowledgments
//! - `kafka_oldest_pending_age_seconds` - Age of oldest pending event
//!
//! ## Circuit Breaker Metrics
//! - `kafka_circuit_state` - Current circuit state (0=CLOSED, 1=HALF_OPEN, 2=OPEN)
//! - `kafka_circuit_failures_total` - Total failures recorded
//! - `kafka_circuit_successes_total` - Total successes recorded
//! - `kafka_circuit_state_transitions_total` - Total state transitions
//!
//! ## Replay Manager Metrics
//! - `kafka_recovery_events_total` - Events replayed during recovery
//! - `kafka_recovery_duration_seconds` - Recovery operation duration
//! - `kafka_recovery_failures_total` - Failed recovery attempts
//! - `kafka_recovery_success_rate` - Success rate of last recovery

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;

use super::circuit_breaker::CircuitState;

/// Centralized Kafka metrics collector
#[derive(Clone)]
pub struct KafkaMetrics {
    // Durable Sink Metrics
    pub lineage_writes_total: IntCounter,
    pub lineage_write_errors_total: IntCounter,
    pub wal_writes_total: IntCounter,
    pub wal_write_duration: Histogram,
    pub kafka_send_duration: Histogram,
    pub kafka_sends_total: IntCounterVec,

    // Acknowledgment Tracker Metrics
    pub pending_acks: IntGauge,
    pub acknowledged_total: IntCounter,
    pub acks_failed_total: IntCounter,
    pub oldest_pending_age: Gauge,
    pub ack_cleanup_runs_total: IntCounter,
    pub ack_cleanup_removed_total: IntCounter,

    // Circuit Breaker Metrics
    pub circuit_state: IntGauge,
    pub circuit_failures_total: IntCounter,
    pub circuit_successes_total: IntCounter,
    pub circuit_state_transitions_total: IntCounter,
    pub circuit_open_duration: Gauge,

    // Replay Manager Metrics
    pub recovery_events_total: IntCounter,
    pub recovery_duration: Histogram,
    pub recovery_failures_total: IntCounter,
    pub recovery_success_rate: Gauge,
    pub recovery_batches_processed: IntCounter,
    pub recovery_retry_attempts: IntCounter,

    // Registry reference
    registry: Arc<Registry>,
}

impl KafkaMetrics {
    /// Create new metrics collector and register with Prometheus
    pub fn new(registry: &Registry) -> prometheus::Result<Self> {
        // Durable Sink Metrics
        let lineage_writes_total = IntCounter::with_opts(Opts::new(
            "kafka_lineage_writes_total",
            "Total lineage events written to Kafka (attempted)",
        ))?;

        let lineage_write_errors_total = IntCounter::with_opts(Opts::new(
            "kafka_lineage_write_errors_total",
            "Total lineage write errors (WAL or Kafka failures)",
        ))?;

        let wal_writes_total = IntCounter::with_opts(Opts::new(
            "kafka_wal_writes_total",
            "Total writes to Kafka WAL",
        ))?;

        let wal_write_duration = Histogram::with_opts(
            HistogramOpts::new(
                "kafka_wal_write_duration_seconds",
                "WAL write latency in seconds",
            )
            .buckets(vec![
                0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
            ]),
        )?;

        let kafka_send_duration = Histogram::with_opts(
            HistogramOpts::new(
                "kafka_send_duration_seconds",
                "Kafka send latency in seconds",
            )
            .buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        )?;

        let kafka_sends_total = IntCounterVec::new(
            Opts::new("kafka_sends_total", "Total Kafka send attempts by result"),
            &["result"], // success, failure, circuit_open
        )?;

        // Acknowledgment Tracker Metrics
        let pending_acks = IntGauge::with_opts(Opts::new(
            "kafka_pending_acks",
            "Current number of pending acknowledgments",
        ))?;

        let acknowledged_total = IntCounter::with_opts(Opts::new(
            "kafka_acknowledged_total",
            "Total events acknowledged by Kafka",
        ))?;

        let acks_failed_total = IntCounter::with_opts(Opts::new(
            "kafka_acks_failed_total",
            "Total acknowledgments that failed",
        ))?;

        let oldest_pending_age = Gauge::with_opts(Opts::new(
            "kafka_oldest_pending_age_seconds",
            "Age of oldest pending acknowledgment in seconds",
        ))?;

        let ack_cleanup_runs_total = IntCounter::with_opts(Opts::new(
            "kafka_ack_cleanup_runs_total",
            "Total acknowledgment cleanup runs",
        ))?;

        let ack_cleanup_removed_total = IntCounter::with_opts(Opts::new(
            "kafka_ack_cleanup_removed_total",
            "Total acknowledgments removed by cleanup",
        ))?;

        // Circuit Breaker Metrics
        let circuit_state = IntGauge::with_opts(Opts::new(
            "kafka_circuit_state",
            "Circuit breaker state (0=CLOSED, 1=HALF_OPEN, 2=OPEN)",
        ))?;

        let circuit_failures_total = IntCounter::with_opts(Opts::new(
            "kafka_circuit_failures_total",
            "Total failures recorded by circuit breaker",
        ))?;

        let circuit_successes_total = IntCounter::with_opts(Opts::new(
            "kafka_circuit_successes_total",
            "Total successes recorded by circuit breaker",
        ))?;

        let circuit_state_transitions_total = IntCounter::with_opts(Opts::new(
            "kafka_circuit_state_transitions_total",
            "Total circuit breaker state transitions",
        ))?;

        let circuit_open_duration = Gauge::with_opts(Opts::new(
            "kafka_circuit_open_duration_seconds",
            "Total time circuit breaker has been in OPEN state",
        ))?;

        // Replay Manager Metrics
        let recovery_events_total = IntCounter::with_opts(Opts::new(
            "kafka_recovery_events_total",
            "Total events replayed during recovery",
        ))?;

        let recovery_duration = Histogram::with_opts(
            HistogramOpts::new(
                "kafka_recovery_duration_seconds",
                "Recovery operation duration in seconds",
            )
            .buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0]),
        )?;

        let recovery_failures_total = IntCounter::with_opts(Opts::new(
            "kafka_recovery_failures_total",
            "Total failed recovery attempts",
        ))?;

        let recovery_success_rate = Gauge::with_opts(Opts::new(
            "kafka_recovery_success_rate",
            "Success rate of last recovery operation (0.0-1.0)",
        ))?;

        let recovery_batches_processed = IntCounter::with_opts(Opts::new(
            "kafka_recovery_batches_processed_total",
            "Total batches processed during recovery",
        ))?;

        let recovery_retry_attempts = IntCounter::with_opts(Opts::new(
            "kafka_recovery_retry_attempts_total",
            "Total retry attempts during recovery",
        ))?;

        // Register all metrics
        registry.register(Box::new(lineage_writes_total.clone()))?;
        registry.register(Box::new(lineage_write_errors_total.clone()))?;
        registry.register(Box::new(wal_writes_total.clone()))?;
        registry.register(Box::new(wal_write_duration.clone()))?;
        registry.register(Box::new(kafka_send_duration.clone()))?;
        registry.register(Box::new(kafka_sends_total.clone()))?;

        registry.register(Box::new(pending_acks.clone()))?;
        registry.register(Box::new(acknowledged_total.clone()))?;
        registry.register(Box::new(acks_failed_total.clone()))?;
        registry.register(Box::new(oldest_pending_age.clone()))?;
        registry.register(Box::new(ack_cleanup_runs_total.clone()))?;
        registry.register(Box::new(ack_cleanup_removed_total.clone()))?;

        registry.register(Box::new(circuit_state.clone()))?;
        registry.register(Box::new(circuit_failures_total.clone()))?;
        registry.register(Box::new(circuit_successes_total.clone()))?;
        registry.register(Box::new(circuit_state_transitions_total.clone()))?;
        registry.register(Box::new(circuit_open_duration.clone()))?;

        registry.register(Box::new(recovery_events_total.clone()))?;
        registry.register(Box::new(recovery_duration.clone()))?;
        registry.register(Box::new(recovery_failures_total.clone()))?;
        registry.register(Box::new(recovery_success_rate.clone()))?;
        registry.register(Box::new(recovery_batches_processed.clone()))?;
        registry.register(Box::new(recovery_retry_attempts.clone()))?;

        Ok(Self {
            lineage_writes_total,
            lineage_write_errors_total,
            wal_writes_total,
            wal_write_duration,
            kafka_send_duration,
            kafka_sends_total,
            pending_acks,
            acknowledged_total,
            acks_failed_total,
            oldest_pending_age,
            ack_cleanup_runs_total,
            ack_cleanup_removed_total,
            circuit_state,
            circuit_failures_total,
            circuit_successes_total,
            circuit_state_transitions_total,
            circuit_open_duration,
            recovery_events_total,
            recovery_duration,
            recovery_failures_total,
            recovery_success_rate,
            recovery_batches_processed,
            recovery_retry_attempts,
            registry: Arc::new(registry.clone()),
        })
    }

    /// Record a lineage write attempt
    pub fn record_write(&self) {
        self.lineage_writes_total.inc();
    }

    /// Record a lineage write error
    pub fn record_write_error(&self) {
        self.lineage_write_errors_total.inc();
    }

    /// Record WAL write with duration
    pub fn record_wal_write(&self, duration_secs: f64) {
        self.wal_writes_total.inc();
        self.wal_write_duration.observe(duration_secs);
    }

    /// Record Kafka send with duration and result
    pub fn record_kafka_send(&self, duration_secs: f64, result: KafkaSendResult) {
        self.kafka_send_duration.observe(duration_secs);
        self.kafka_sends_total
            .with_label_values(&[result.as_str()])
            .inc();
    }

    /// Update pending acknowledgments count
    pub fn set_pending_acks(&self, count: i64) {
        self.pending_acks.set(count);
    }

    /// Record acknowledgment
    pub fn record_acknowledgment(&self) {
        self.acknowledged_total.inc();
    }

    /// Record failed acknowledgment
    pub fn record_ack_failure(&self) {
        self.acks_failed_total.inc();
    }

    /// Update oldest pending age
    pub fn set_oldest_pending_age(&self, age_secs: f64) {
        self.oldest_pending_age.set(age_secs);
    }

    /// Record cleanup run
    pub fn record_cleanup_run(&self, removed: usize) {
        self.ack_cleanup_runs_total.inc();
        self.ack_cleanup_removed_total.inc_by(removed as u64);
    }

    /// Update circuit breaker state
    pub fn set_circuit_state(&self, state: CircuitState) {
        let state_value = match state {
            CircuitState::Closed => 0,
            CircuitState::HalfOpen => 1,
            CircuitState::Open => 2,
        };
        self.circuit_state.set(state_value);
    }

    /// Record circuit breaker failure
    pub fn record_circuit_failure(&self) {
        self.circuit_failures_total.inc();
    }

    /// Record circuit breaker success
    pub fn record_circuit_success(&self) {
        self.circuit_successes_total.inc();
    }

    /// Record circuit state transition
    pub fn record_circuit_transition(&self) {
        self.circuit_state_transitions_total.inc();
    }

    /// Update circuit open duration
    pub fn set_circuit_open_duration(&self, duration_secs: f64) {
        self.circuit_open_duration.set(duration_secs);
    }

    /// Record recovery operation
    pub fn record_recovery(
        &self,
        events_replayed: usize,
        duration_secs: f64,
        batches: usize,
        retries: usize,
        success_rate: f64,
    ) {
        self.recovery_events_total.inc_by(events_replayed as u64);
        self.recovery_duration.observe(duration_secs);
        self.recovery_batches_processed.inc_by(batches as u64);
        self.recovery_retry_attempts.inc_by(retries as u64);
        self.recovery_success_rate.set(success_rate);
    }

    /// Record recovery failure
    pub fn record_recovery_failure(&self) {
        self.recovery_failures_total.inc();
    }

    /// Get registry reference
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Kafka send result for metrics labeling
#[derive(Debug, Clone, Copy)]
pub enum KafkaSendResult {
    Success,
    Failure,
    CircuitOpen,
}

impl KafkaSendResult {
    fn as_str(&self) -> &'static str {
        match self {
            KafkaSendResult::Success => "success",
            KafkaSendResult::Failure => "failure",
            KafkaSendResult::CircuitOpen => "circuit_open",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_metrics_creation() {
        let registry = Registry::new();
        let metrics = KafkaMetrics::new(&registry).unwrap();

        // Verify metrics can be recorded without panics
        metrics.record_write();
        metrics.record_wal_write(0.005);
        metrics.record_kafka_send(0.05, KafkaSendResult::Success);
        metrics.set_pending_acks(10);
        metrics.record_acknowledgment();
        metrics.set_circuit_state(CircuitState::Closed);
        metrics.record_circuit_failure();
        metrics.record_recovery(100, 5.0, 10, 2, 0.95);
    }

    #[test]
    fn test_circuit_state_values() {
        let registry = Registry::new();
        let metrics = KafkaMetrics::new(&registry).unwrap();

        metrics.set_circuit_state(CircuitState::Closed);
        assert_eq!(metrics.circuit_state.get(), 0);

        metrics.set_circuit_state(CircuitState::HalfOpen);
        assert_eq!(metrics.circuit_state.get(), 1);

        metrics.set_circuit_state(CircuitState::Open);
        assert_eq!(metrics.circuit_state.get(), 2);
    }

    #[test]
    fn test_kafka_send_result_labels() {
        assert_eq!(KafkaSendResult::Success.as_str(), "success");
        assert_eq!(KafkaSendResult::Failure.as_str(), "failure");
        assert_eq!(KafkaSendResult::CircuitOpen.as_str(), "circuit_open");
    }

    #[test]
    fn test_metrics_increment() {
        let registry = Registry::new();
        let metrics = KafkaMetrics::new(&registry).unwrap();

        // Record multiple events
        for _ in 0..5 {
            metrics.record_write();
        }
        assert_eq!(metrics.lineage_writes_total.get(), 5);

        for _ in 0..3 {
            metrics.record_acknowledgment();
        }
        assert_eq!(metrics.acknowledged_total.get(), 3);
    }

    #[test]
    fn test_cleanup_metrics() {
        let registry = Registry::new();
        let metrics = KafkaMetrics::new(&registry).unwrap();

        metrics.record_cleanup_run(50);
        assert_eq!(metrics.ack_cleanup_runs_total.get(), 1);
        assert_eq!(metrics.ack_cleanup_removed_total.get(), 50);

        metrics.record_cleanup_run(25);
        assert_eq!(metrics.ack_cleanup_runs_total.get(), 2);
        assert_eq!(metrics.ack_cleanup_removed_total.get(), 75);
    }

    #[test]
    fn test_recovery_metrics() {
        let registry = Registry::new();
        let metrics = KafkaMetrics::new(&registry).unwrap();

        metrics.record_recovery(1000, 30.0, 10, 5, 0.98);

        assert_eq!(metrics.recovery_events_total.get(), 1000);
        assert_eq!(metrics.recovery_batches_processed.get(), 10);
        assert_eq!(metrics.recovery_retry_attempts.get(), 5);
        assert_eq!(metrics.recovery_success_rate.get(), 0.98);
    }
}
