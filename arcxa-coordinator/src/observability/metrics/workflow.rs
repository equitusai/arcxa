//! Workflow execution metrics
//!
//! Tracks workflow and action execution metrics:
//! - Action execution counts by type and status
//! - Action execution latency distribution
//! - Workflow execution counts and duration
//! - Integration-specific metrics (Kafka, HTTP, Lineage)

use anyhow::Result;
use prometheus::{
    exponential_buckets, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};

/// Workflow execution metrics
///
/// Monitors workflow routing, action execution, and integration health.
pub struct WorkflowMetrics {
    // Action execution metrics
    actions_executed_total: IntCounterVec,
    action_duration_seconds: HistogramVec,
    actions_in_flight: IntGauge,

    // Integration-specific metrics
    kafka_messages_sent_total: IntCounterVec,
    kafka_send_duration_seconds: HistogramVec,
    kafka_send_failures_total: IntCounterVec,

    http_requests_sent_total: IntCounterVec,
    http_request_duration_seconds: Histogram,
    http_request_failures_total: IntCounterVec,
    http_retries_total: IntCounter,

    lineage_events_recorded_total: IntCounterVec,
    lineage_recording_failures_total: IntCounter,

    // Workflow routing metrics
    workflow_executions_total: IntCounterVec,
    workflow_execution_duration_seconds: HistogramVec,
    route_matches_total: IntCounterVec,

    // Memory usage metrics (Proposal 4)
    workflow_memory_bytes: Gauge,
    step_memory_bytes: HistogramVec,
    rows_in_memory: IntGauge,

    // Progress tracking metrics (Phase 3)
    executions_active: IntGaugeVec,
    execution_status_total: IntCounterVec,
    execution_rows_processed: HistogramVec,
    execution_progress_percent: GaugeVec,
    execution_cancellations_total: IntCounterVec,
}

impl WorkflowMetrics {
    /// Create and register workflow metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        // Action execution metrics
        let actions_executed_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_actions_executed_total",
                "Total workflow actions executed by type and status",
            ),
            &["action_type", "status"],
        )?;

        let action_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_action_duration_seconds",
                "Workflow action execution latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 12)?), // 1ms to ~4s
            &["action_type"],
        )?;

        let actions_in_flight = IntGauge::new(
            "graphica_workflow_actions_in_flight",
            "Current number of workflow actions being executed",
        )?;

        // Kafka metrics
        let kafka_messages_sent_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_kafka_messages_sent_total",
                "Total Kafka messages sent by topic",
            ),
            &["topic"],
        )?;

        let kafka_send_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_kafka_send_duration_seconds",
                "Kafka message send latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 10)?), // 1ms to ~1s
            &["topic"],
        )?;

        let kafka_send_failures_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_kafka_send_failures_total",
                "Total Kafka send failures by topic",
            ),
            &["topic"],
        )?;

        // HTTP metrics
        let http_requests_sent_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_http_requests_sent_total",
                "Total HTTP requests sent by status code",
            ),
            &["status_code"],
        )?;

        let http_request_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "graphica_workflow_http_request_duration_seconds",
                "HTTP request latency in seconds",
            )
            .buckets(exponential_buckets(0.01, 2.0, 10)?), // 10ms to ~10s
        )?;

        let http_request_failures_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_http_request_failures_total",
                "Total HTTP request failures",
            ),
            &["error_type"],
        )?;

        let http_retries_total = IntCounter::with_opts(Opts::new(
            "graphica_workflow_http_retries_total",
            "Total HTTP request retries",
        ))?;

        // Lineage metrics
        let lineage_events_recorded_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_lineage_events_recorded_total",
                "Total lineage events recorded by event type",
            ),
            &["event_type"],
        )?;

        let lineage_recording_failures_total = IntCounter::with_opts(Opts::new(
            "graphica_workflow_lineage_recording_failures_total",
            "Total lineage recording failures",
        ))?;

        // Workflow routing metrics
        let workflow_executions_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_executions_total",
                "Total workflow executions by workflow ID and result",
            ),
            &["workflow_id", "result"],
        )?;

        let workflow_execution_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_duration_seconds",
                "Workflow execution duration in seconds",
            )
            .buckets(exponential_buckets(0.01, 2.0, 12)?), // 10ms to ~40s
            &["workflow_id"],
        )?;

        let route_matches_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_route_matches_total",
                "Total route matches by workflow and route ID",
            ),
            &["workflow_id", "route_id"],
        )?;

        // Memory usage metrics (Proposal 4)
        let workflow_memory_bytes = Gauge::new(
            "graphica_workflow_memory_bytes",
            "Estimated memory usage in bytes for current workflow execution",
        )?;

        let step_memory_bytes = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_step_memory_bytes",
                "Memory usage histogram for workflow steps in bytes",
            )
            .buckets(vec![
                1_000_000.0,      // 1 MB
                10_000_000.0,     // 10 MB
                100_000_000.0,    // 100 MB
                500_000_000.0,    // 500 MB
                1_000_000_000.0,  // 1 GB
                5_000_000_000.0,  // 5 GB
                10_000_000_000.0, // 10 GB
                50_000_000_000.0, // 50 GB
            ]),
            &["step_id", "workflow_id"],
        )?;

        let rows_in_memory = IntGauge::new(
            "graphica_workflow_rows_in_memory",
            "Current number of data rows held in memory",
        )?;

        // Progress tracking metrics (Phase 3)
        let executions_active = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_executions_active",
                "Current number of active workflow executions by status",
            ),
            &["status"],
        )?;

        let execution_status_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_status_total",
                "Total workflow executions by final status",
            ),
            &["workflow_id", "status"],
        )?;

        let execution_rows_processed = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_rows_processed",
                "Distribution of rows processed per execution",
            )
            .buckets(vec![
                100.0,
                1_000.0,
                10_000.0,
                100_000.0,
                1_000_000.0,
                10_000_000.0,
                100_000_000.0,
            ]),
            &["workflow_id"],
        )?;

        let execution_progress_percent = GaugeVec::new(
            Opts::new(
                "graphica_workflow_execution_progress_percent",
                "Current execution progress percentage",
            ),
            &["execution_id", "workflow_id"],
        )?;

        let execution_cancellations_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_cancellations_total",
                "Total workflow execution cancellations",
            ),
            &["workflow_id", "reason"],
        )?;

        // Register all metrics
        registry.register(Box::new(actions_executed_total.clone()))?;
        registry.register(Box::new(action_duration_seconds.clone()))?;
        registry.register(Box::new(actions_in_flight.clone()))?;

        registry.register(Box::new(kafka_messages_sent_total.clone()))?;
        registry.register(Box::new(kafka_send_duration_seconds.clone()))?;
        registry.register(Box::new(kafka_send_failures_total.clone()))?;

        registry.register(Box::new(http_requests_sent_total.clone()))?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;
        registry.register(Box::new(http_request_failures_total.clone()))?;
        registry.register(Box::new(http_retries_total.clone()))?;

        registry.register(Box::new(lineage_events_recorded_total.clone()))?;
        registry.register(Box::new(lineage_recording_failures_total.clone()))?;

        registry.register(Box::new(workflow_executions_total.clone()))?;
        registry.register(Box::new(workflow_execution_duration_seconds.clone()))?;
        registry.register(Box::new(route_matches_total.clone()))?;

        registry.register(Box::new(workflow_memory_bytes.clone()))?;
        registry.register(Box::new(step_memory_bytes.clone()))?;
        registry.register(Box::new(rows_in_memory.clone()))?;

        // Register Phase 3 metrics
        registry.register(Box::new(executions_active.clone()))?;
        registry.register(Box::new(execution_status_total.clone()))?;
        registry.register(Box::new(execution_rows_processed.clone()))?;
        registry.register(Box::new(execution_progress_percent.clone()))?;
        registry.register(Box::new(execution_cancellations_total.clone()))?;

        Ok(Self {
            actions_executed_total,
            action_duration_seconds,
            actions_in_flight,
            kafka_messages_sent_total,
            kafka_send_duration_seconds,
            kafka_send_failures_total,
            http_requests_sent_total,
            http_request_duration_seconds,
            http_request_failures_total,
            http_retries_total,
            lineage_events_recorded_total,
            lineage_recording_failures_total,
            workflow_executions_total,
            workflow_execution_duration_seconds,
            route_matches_total,
            workflow_memory_bytes,
            step_memory_bytes,
            rows_in_memory,
            executions_active,
            execution_status_total,
            execution_rows_processed,
            execution_progress_percent,
            execution_cancellations_total,
        })
    }

    // Action execution metrics

    /// Record action execution start
    pub fn action_started(&self) {
        self.actions_in_flight.inc();
    }

    /// Record action execution completion
    pub fn record_action(&self, action_type: &str, status: &str, duration_secs: f64) {
        self.actions_in_flight.dec();

        self.actions_executed_total
            .with_label_values(&[action_type, status])
            .inc();

        self.action_duration_seconds
            .with_label_values(&[action_type])
            .observe(duration_secs);
    }

    // Kafka metrics

    /// Record successful Kafka message send
    pub fn record_kafka_send(&self, topic: &str, duration_secs: f64) {
        self.kafka_messages_sent_total
            .with_label_values(&[topic])
            .inc();

        self.kafka_send_duration_seconds
            .with_label_values(&[topic])
            .observe(duration_secs);
    }

    /// Record Kafka send failure
    pub fn record_kafka_failure(&self, topic: &str) {
        self.kafka_send_failures_total
            .with_label_values(&[topic])
            .inc();
    }

    // HTTP metrics

    /// Record HTTP request completion
    pub fn record_http_request(&self, status_code: u16, duration_secs: f64, retries: u32) {
        self.http_requests_sent_total
            .with_label_values(&[&status_code.to_string()])
            .inc();

        self.http_request_duration_seconds.observe(duration_secs);

        if retries > 0 {
            self.http_retries_total.inc_by(retries as u64);
        }
    }

    /// Record HTTP request failure
    pub fn record_http_failure(&self, error_type: &str) {
        self.http_request_failures_total
            .with_label_values(&[error_type])
            .inc();
    }

    // Lineage metrics

    /// Record lineage event
    pub fn record_lineage_event(&self, event_type: &str) {
        self.lineage_events_recorded_total
            .with_label_values(&[event_type])
            .inc();
    }

    /// Record lineage recording failure
    pub fn record_lineage_failure(&self) {
        self.lineage_recording_failures_total.inc();
    }

    // Workflow routing metrics

    /// Record workflow execution
    pub fn record_workflow_execution(&self, workflow_id: &str, result: &str, duration_secs: f64) {
        self.workflow_executions_total
            .with_label_values(&[workflow_id, result])
            .inc();

        self.workflow_execution_duration_seconds
            .with_label_values(&[workflow_id])
            .observe(duration_secs);
    }

    /// Record route match
    pub fn record_route_match(&self, workflow_id: &str, route_id: &str) {
        self.route_matches_total
            .with_label_values(&[workflow_id, route_id])
            .inc();
    }

    // Memory usage metrics (Proposal 4)

    /// Update total workflow memory usage
    pub fn set_workflow_memory_bytes(&self, bytes: f64) {
        self.workflow_memory_bytes.set(bytes);
    }

    /// Record memory usage for a specific workflow step
    pub fn record_step_memory(&self, step_id: &str, workflow_id: &str, bytes: f64) {
        self.step_memory_bytes
            .with_label_values(&[step_id, workflow_id])
            .observe(bytes);
    }

    /// Update the count of rows currently in memory
    pub fn set_rows_in_memory(&self, count: i64) {
        self.rows_in_memory.set(count);
    }

    /// Estimate memory usage for JSON data
    ///
    /// Rough estimation: each JSON value uses ~100 bytes overhead
    /// plus the actual data size
    pub fn estimate_json_memory_bytes(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Null => 8,
            serde_json::Value::Bool(_) => 8,
            serde_json::Value::Number(_) => 16,
            serde_json::Value::String(s) => 24 + s.len(),
            serde_json::Value::Array(arr) => {
                24 + arr
                    .iter()
                    .map(|v| Self::estimate_json_memory_bytes(v))
                    .sum::<usize>()
            }
            serde_json::Value::Object(obj) => {
                24 + obj
                    .iter()
                    .map(|(k, v)| k.len() + Self::estimate_json_memory_bytes(v))
                    .sum::<usize>()
            }
        }
    }

    // Progress tracking metrics (Phase 3)

    /// Record execution started (increment active count)
    pub fn execution_started(&self, status: &str) {
        self.executions_active.with_label_values(&[status]).inc();
    }

    /// Record execution completed (decrement active count, update status)
    pub fn execution_completed(&self, workflow_id: &str, status: &str, rows_processed: u64) {
        // Decrement active count (transition from running/queued to completed/failed/cancelled)
        self.executions_active.with_label_values(&["running"]).dec();

        // Record final status
        self.execution_status_total
            .with_label_values(&[workflow_id, status])
            .inc();

        // Record rows processed distribution
        self.execution_rows_processed
            .with_label_values(&[workflow_id])
            .observe(rows_processed as f64);
    }

    /// Update execution progress percentage
    pub fn set_execution_progress(&self, execution_id: &str, workflow_id: &str, percent: f64) {
        self.execution_progress_percent
            .with_label_values(&[execution_id, workflow_id])
            .set(percent);
    }

    /// Clear execution progress (when execution completes)
    pub fn clear_execution_progress(&self, execution_id: &str, workflow_id: &str) {
        // Prometheus doesn't have a delete metric, so we set to -1 to indicate completion
        self.execution_progress_percent
            .with_label_values(&[execution_id, workflow_id])
            .set(-1.0);
    }

    /// Record execution cancellation
    pub fn record_cancellation(&self, workflow_id: &str, reason: &str) {
        self.execution_cancellations_total
            .with_label_values(&[workflow_id, reason])
            .inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_metrics_creation() {
        let registry = Registry::new();
        let result = WorkflowMetrics::new(&registry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_action_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowMetrics::new(&registry).unwrap();

        metrics.action_started();
        metrics.record_action("SendToKafka", "success", 0.025);
        metrics.record_action("SendToHttp", "failed", 0.150);

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_kafka_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowMetrics::new(&registry).unwrap();

        metrics.record_kafka_send("test_topic", 0.015);
        metrics.record_kafka_failure("test_topic");

        // Metrics should be recorded without panics
    }

    #[test]
    fn test_http_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowMetrics::new(&registry).unwrap();

        metrics.record_http_request(200, 0.125, 1);
        metrics.record_http_failure("timeout");

        // Metrics should be recorded without panics
    }
}
