//! Distributed tracing support for Kafka durability operations
//!
//! This module provides OpenTelemetry-compatible tracing spans for all
//! Kafka durability operations, enabling end-to-end visibility across:
//! - WAL writes
//! - Kafka sends
//! - Acknowledgment tracking
//! - Circuit breaker decisions
//! - Recovery operations
//!
//! # Trace Hierarchy
//!
//! ```text
//! write_lineage_event
//!   ├─ wal_write
//!   ├─ track_acknowledgment
//!   ├─ circuit_breaker_check
//!   └─ kafka_send
//!      └─ wait_for_acknowledgment
//!
//! recover_on_startup
//!   ├─ find_unacknowledged_events
//!   └─ replay_batch (repeated)
//!      ├─ circuit_breaker_check
//!      └─ kafka_send_batch
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::storage::kafka::kafka_tracing::KafkaTracing;
//! use uuid::Uuid;
//!
//! let tracer = KafkaTracing::new();
//!
//! // Trace a lineage write
//! let event_id = Uuid::new_v4();
//! let span = tracer.write_lineage_event(event_id, "record-123");
//! // ... perform write ...
//! drop(span); // End span
//! ```

use tracing::{debug, error, info, span, warn, Level, Span};
use uuid::Uuid;

/// Kafka tracing utility
#[derive(Clone)]
pub struct KafkaTracing;

impl KafkaTracing {
    /// Create new tracing utility
    pub fn new() -> Self {
        Self
    }

    /// Create span for lineage event write
    pub fn write_lineage_event(&self, event_id: Uuid, record_id: &str) -> Span {
        span!(
            Level::INFO,
            "write_lineage_event",
            event_id = %event_id,
            record_id = %record_id,
            component = "kafka_durable_sink"
        )
    }

    /// Create span for WAL write
    pub fn wal_write(&self, event_id: Uuid, lsn: u64) -> Span {
        span!(
            Level::DEBUG,
            "wal_write",
            event_id = %event_id,
            lsn = lsn,
            component = "kafka_wal"
        )
    }

    /// Create span for acknowledgment tracking
    pub fn track_acknowledgment(&self, event_id: Uuid) -> Span {
        span!(
            Level::DEBUG,
            "track_acknowledgment",
            event_id = %event_id,
            component = "ack_tracker"
        )
    }

    /// Create span for circuit breaker check
    pub fn circuit_breaker_check(&self) -> Span {
        span!(
            Level::DEBUG,
            "circuit_breaker_check",
            component = "circuit_breaker"
        )
    }

    /// Create span for Kafka send
    pub fn kafka_send(&self, event_id: Uuid, topic: &str, partition: Option<i32>) -> Span {
        span!(
            Level::INFO,
            "kafka_send",
            event_id = %event_id,
            topic = %topic,
            partition = partition,
            component = "kafka_producer"
        )
    }

    /// Create span for acknowledgment wait
    pub fn wait_for_acknowledgment(&self, event_id: Uuid) -> Span {
        span!(
            Level::DEBUG,
            "wait_for_acknowledgment",
            event_id = %event_id,
            component = "kafka_producer"
        )
    }

    /// Create span for recovery operation
    pub fn recover_on_startup(&self, pending_count: usize) -> Span {
        span!(
            Level::INFO,
            "recover_on_startup",
            pending_count = pending_count,
            component = "replay_manager"
        )
    }

    /// Create span for finding unacknowledged events
    pub fn find_unacknowledged_events(&self) -> Span {
        span!(
            Level::DEBUG,
            "find_unacknowledged_events",
            component = "replay_manager"
        )
    }

    /// Create span for batch replay
    pub fn replay_batch(&self, batch_index: usize, batch_size: usize) -> Span {
        span!(
            Level::INFO,
            "replay_batch",
            batch_index = batch_index,
            batch_size = batch_size,
            component = "replay_manager"
        )
    }

    /// Create span for batch replay with retry
    pub fn replay_batch_with_retry(&self, batch_index: usize, attempt: u32) -> Span {
        span!(
            Level::WARN,
            "replay_batch_retry",
            batch_index = batch_index,
            attempt = attempt,
            component = "replay_manager"
        )
    }

    /// Create span for cleanup operation
    pub fn cleanup_acknowledged(&self) -> Span {
        span!(
            Level::DEBUG,
            "cleanup_acknowledged",
            component = "ack_tracker"
        )
    }

    /// Log write success
    pub fn log_write_success(
        &self,
        event_id: Uuid,
        wal_lsn: u64,
        kafka_partition: i32,
        kafka_offset: i64,
    ) {
        info!(
            event_id = %event_id,
            wal_lsn = wal_lsn,
            kafka_partition = kafka_partition,
            kafka_offset = kafka_offset,
            "Lineage event written successfully"
        );
    }

    /// Log write failure
    pub fn log_write_failure(&self, event_id: Uuid, error: &str) {
        error!(
            event_id = %event_id,
            error = %error,
            "Lineage event write failed"
        );
    }

    /// Log circuit breaker state change
    pub fn log_circuit_state_change(&self, from_state: &str, to_state: &str) {
        warn!(
            from_state = %from_state,
            to_state = %to_state,
            "Circuit breaker state changed"
        );
    }

    /// Log recovery start
    pub fn log_recovery_start(&self, total_events: usize) {
        info!(
            total_events = total_events,
            "Starting Kafka recovery operation"
        );
    }

    /// Log recovery completion
    pub fn log_recovery_complete(
        &self,
        total_events: usize,
        replayed: usize,
        failed: usize,
        duration_secs: f64,
    ) {
        info!(
            total_events = total_events,
            replayed = replayed,
            failed = failed,
            duration_secs = duration_secs,
            success_rate = (replayed as f64 / total_events as f64),
            "Kafka recovery operation completed"
        );
    }

    /// Log batch replay success
    pub fn log_batch_replay_success(&self, batch_index: usize, events_replayed: usize) {
        debug!(
            batch_index = batch_index,
            events_replayed = events_replayed,
            "Batch replayed successfully"
        );
    }

    /// Log batch replay failure
    pub fn log_batch_replay_failure(&self, batch_index: usize, error: &str) {
        error!(
            batch_index = batch_index,
            error = %error,
            "Batch replay failed"
        );
    }

    /// Log acknowledgment received
    pub fn log_acknowledgment_received(
        &self,
        event_id: Uuid,
        partition: i32,
        offset: i64,
        age_ms: u64,
    ) {
        debug!(
            event_id = %event_id,
            partition = partition,
            offset = offset,
            age_ms = age_ms,
            "Kafka acknowledgment received"
        );
    }

    /// Log acknowledgment failure
    pub fn log_acknowledgment_failure(&self, event_id: Uuid, retry_count: u32) {
        warn!(
            event_id = %event_id,
            retry_count = retry_count,
            "Kafka acknowledgment failed (will retry)"
        );
    }

    /// Log cleanup run
    pub fn log_cleanup_run(&self, removed: usize, total_acks: usize) {
        debug!(
            removed = removed,
            total_acks = total_acks,
            "Acknowledgment cleanup completed"
        );
    }

    /// Log backpressure applied
    pub fn log_backpressure(&self, pending_count: usize, max_pending: usize) {
        warn!(
            pending_count = pending_count,
            max_pending = max_pending,
            "Backpressure applied (pending acks at capacity)"
        );
    }

    /// Log circuit breaker opened
    pub fn log_circuit_opened(&self, failure_count: u64) {
        error!(
            failure_count = failure_count,
            "Circuit breaker OPENED (Kafka unavailable, entering WAL-only mode)"
        );
    }

    /// Log circuit breaker half-open
    pub fn log_circuit_half_open(&self) {
        warn!("Circuit breaker HALF-OPEN (testing Kafka recovery)");
    }

    /// Log circuit breaker closed
    pub fn log_circuit_closed(&self, success_count: u64) {
        info!(
            success_count = success_count,
            "Circuit breaker CLOSED (Kafka recovered, normal operation resumed)"
        );
    }
}

impl Default for KafkaTracing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_spans_creation() {
        let tracer = KafkaTracing::new();
        let event_id = Uuid::new_v4();

        // Verify spans can be created without panics
        let _write_span = tracer.write_lineage_event(event_id, "test_record");
        let _wal_span = tracer.wal_write(event_id, 42);
        let _ack_span = tracer.track_acknowledgment(event_id);
        let _circuit_span = tracer.circuit_breaker_check();
        let _send_span = tracer.kafka_send(event_id, "lineage", Some(0));
        let _wait_span = tracer.wait_for_acknowledgment(event_id);
        let _recovery_span = tracer.recover_on_startup(100);
        let _batch_span = tracer.replay_batch(0, 50);
    }

    #[test]
    fn test_logging_methods() {
        let tracer = KafkaTracing::new();
        let event_id = Uuid::new_v4();

        // Verify logging methods don't panic
        tracer.log_write_success(event_id, 42, 0, 1000);
        tracer.log_write_failure(event_id, "test error");
        tracer.log_circuit_state_change("CLOSED", "OPEN");
        tracer.log_recovery_start(100);
        tracer.log_recovery_complete(100, 95, 5, 30.5);
        tracer.log_batch_replay_success(0, 50);
        tracer.log_batch_replay_failure(1, "test error");
        tracer.log_acknowledgment_received(event_id, 0, 1000, 150);
        tracer.log_acknowledgment_failure(event_id, 3);
        tracer.log_cleanup_run(50, 200);
        tracer.log_backpressure(10000, 10000);
        tracer.log_circuit_opened(5);
        tracer.log_circuit_half_open();
        tracer.log_circuit_closed(3);
    }

    #[test]
    fn test_default_construction() {
        let tracer1 = KafkaTracing::new();
        let tracer2 = KafkaTracing::default();

        // Both should be usable
        let event_id = Uuid::new_v4();
        let _span1 = tracer1.write_lineage_event(event_id, "test");
        let _span2 = tracer2.write_lineage_event(event_id, "test");
    }
}
