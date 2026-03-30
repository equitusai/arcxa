//! Workflow execution metrics
//!
//! Tracks workflow and action execution metrics:
//! - Action execution counts by type and status
//! - Action execution latency distribution
//! - Workflow execution counts and duration
//! - Integration-specific metrics (Kafka, HTTP, Lineage)

use crate::workflows::domain::ExecutionRuntimeMetricsSummary;
use crate::workflows::engine::{StreamRuntimeSummary, StreamStats};
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

    // Runtime storage telemetry (Phase 2)
    execution_runtime_summaries_total: IntCounterVec,
    execution_runtime_storage_backend_total: IntCounterVec,
    execution_runtime_planned_tier_total: IntCounterVec,
    execution_runtime_storage_decision_total: IntCounterVec,
    execution_runtime_spill_events_total: IntCounterVec,
    execution_runtime_spill_bytes_total: IntCounterVec,
    execution_runtime_steps_with_metrics: HistogramVec,
    execution_runtime_steps_with_disk_storage: HistogramVec,
    execution_runtime_memory_high_water_mark_bytes: HistogramVec,
    execution_runtime_reserved_spill_bytes: HistogramVec,

    // Streaming control-plane/runtime telemetry
    streams_active: IntGaugeVec,
    stream_runtime_summaries_total: IntCounterVec,
    stream_runtime_storage_backend_total: IntCounterVec,
    stream_runtime_checkpoint_interval_records: HistogramVec,
    stream_runtime_records_processed: GaugeVec,
    stream_runtime_throughput: GaugeVec,
    stream_runtime_avg_latency_ms: GaugeVec,
    stream_runtime_lag: GaugeVec,
    stream_runtime_active_workers: GaugeVec,
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

        // Runtime storage telemetry (Phase 2)
        let execution_runtime_summaries_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_summaries_total",
                "Total workflow executions that reported runtime storage telemetry",
            ),
            &["workflow_id"],
        )?;

        let execution_runtime_storage_backend_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_storage_backend_total",
                "Total workflow executions by observed runtime storage backend",
            ),
            &["workflow_id", "storage_backend"],
        )?;

        let execution_runtime_planned_tier_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_planned_tier_total",
                "Total workflow executions by observed planned storage tier",
            ),
            &["workflow_id", "planned_tier"],
        )?;

        let execution_runtime_storage_decision_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_storage_decision_total",
                "Total workflow executions by runtime storage decision reason",
            ),
            &["workflow_id", "reason"],
        )?;

        let execution_runtime_spill_events_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_spill_events_total",
                "Total spill events observed across workflow executions",
            ),
            &["workflow_id"],
        )?;

        let execution_runtime_spill_bytes_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_execution_runtime_spill_bytes_total",
                "Total spill bytes observed across workflow executions",
            ),
            &["workflow_id"],
        )?;

        let execution_runtime_steps_with_metrics = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_runtime_steps_with_metrics",
                "Distribution of steps per execution that reported runtime metrics",
            )
            .buckets(vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0]),
            &["workflow_id"],
        )?;

        let execution_runtime_steps_with_disk_storage = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_runtime_steps_with_disk_storage",
                "Distribution of steps per execution that used on-disk storage",
            )
            .buckets(vec![0.0, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0]),
            &["workflow_id"],
        )?;

        let execution_runtime_memory_high_water_mark_bytes = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_runtime_memory_high_water_mark_bytes",
                "Runtime memory high-water mark per workflow execution in bytes",
            )
            .buckets(vec![
                1_000_000.0,
                10_000_000.0,
                100_000_000.0,
                500_000_000.0,
                1_000_000_000.0,
                5_000_000_000.0,
            ]),
            &["workflow_id"],
        )?;

        let execution_runtime_reserved_spill_bytes = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_execution_runtime_reserved_spill_bytes",
                "Reserved spill bytes high-water mark per workflow execution",
            )
            .buckets(vec![
                1_000_000.0,
                10_000_000.0,
                100_000_000.0,
                500_000_000.0,
                1_000_000_000.0,
                5_000_000_000.0,
            ]),
            &["workflow_id"],
        )?;

        let streams_active = IntGaugeVec::new(
            Opts::new(
                "graphica_workflow_streams_active",
                "Current number of active streaming workflows by workflow and storage backend",
            ),
            &["workflow_id", "storage_backend"],
        )?;

        let stream_runtime_summaries_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_summaries_total",
                "Total streaming workflow runtime summaries recorded",
            ),
            &["workflow_id", "execution_engine"],
        )?;

        let stream_runtime_storage_backend_total = IntCounterVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_storage_backend_total",
                "Total streaming workflows started by storage backend",
            ),
            &["workflow_id", "storage_backend"],
        )?;

        let stream_runtime_checkpoint_interval_records = HistogramVec::new(
            HistogramOpts::new(
                "graphica_workflow_stream_runtime_checkpoint_interval_records",
                "Checkpoint interval in records for streaming workflow executions",
            )
            .buckets(vec![10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0]),
            &["workflow_id"],
        )?;

        let stream_runtime_records_processed = GaugeVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_records_processed",
                "Current number of processed records for active streaming workflows",
            ),
            &["workflow_id"],
        )?;

        let stream_runtime_throughput = GaugeVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_throughput",
                "Current throughput in records/sec for active streaming workflows",
            ),
            &["workflow_id"],
        )?;

        let stream_runtime_avg_latency_ms = GaugeVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_avg_latency_ms",
                "Current average latency in milliseconds for active streaming workflows",
            ),
            &["workflow_id"],
        )?;

        let stream_runtime_lag = GaugeVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_lag",
                "Current consumer lag for active streaming workflows",
            ),
            &["workflow_id"],
        )?;

        let stream_runtime_active_workers = GaugeVec::new(
            Opts::new(
                "graphica_workflow_stream_runtime_active_workers",
                "Current active workers for streaming workflows",
            ),
            &["workflow_id"],
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
        registry.register(Box::new(execution_runtime_summaries_total.clone()))?;
        registry.register(Box::new(execution_runtime_storage_backend_total.clone()))?;
        registry.register(Box::new(execution_runtime_planned_tier_total.clone()))?;
        registry.register(Box::new(execution_runtime_storage_decision_total.clone()))?;
        registry.register(Box::new(execution_runtime_spill_events_total.clone()))?;
        registry.register(Box::new(execution_runtime_spill_bytes_total.clone()))?;
        registry.register(Box::new(execution_runtime_steps_with_metrics.clone()))?;
        registry.register(Box::new(execution_runtime_steps_with_disk_storage.clone()))?;
        registry.register(Box::new(
            execution_runtime_memory_high_water_mark_bytes.clone(),
        ))?;
        registry.register(Box::new(execution_runtime_reserved_spill_bytes.clone()))?;
        registry.register(Box::new(streams_active.clone()))?;
        registry.register(Box::new(stream_runtime_summaries_total.clone()))?;
        registry.register(Box::new(stream_runtime_storage_backend_total.clone()))?;
        registry.register(Box::new(stream_runtime_checkpoint_interval_records.clone()))?;
        registry.register(Box::new(stream_runtime_records_processed.clone()))?;
        registry.register(Box::new(stream_runtime_throughput.clone()))?;
        registry.register(Box::new(stream_runtime_avg_latency_ms.clone()))?;
        registry.register(Box::new(stream_runtime_lag.clone()))?;
        registry.register(Box::new(stream_runtime_active_workers.clone()))?;

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
            execution_runtime_summaries_total,
            execution_runtime_storage_backend_total,
            execution_runtime_planned_tier_total,
            execution_runtime_storage_decision_total,
            execution_runtime_spill_events_total,
            execution_runtime_spill_bytes_total,
            execution_runtime_steps_with_metrics,
            execution_runtime_steps_with_disk_storage,
            execution_runtime_memory_high_water_mark_bytes,
            execution_runtime_reserved_spill_bytes,
            streams_active,
            stream_runtime_summaries_total,
            stream_runtime_storage_backend_total,
            stream_runtime_checkpoint_interval_records,
            stream_runtime_records_processed,
            stream_runtime_throughput,
            stream_runtime_avg_latency_ms,
            stream_runtime_lag,
            stream_runtime_active_workers,
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

    /// Record execution-level runtime storage telemetry for a completed workflow run.
    pub fn record_execution_runtime_summary(
        &self,
        workflow_id: &str,
        summary: &ExecutionRuntimeMetricsSummary,
    ) {
        self.execution_runtime_summaries_total
            .with_label_values(&[workflow_id])
            .inc();

        for backend in &summary.storage_backends {
            self.execution_runtime_storage_backend_total
                .with_label_values(&[workflow_id, backend])
                .inc();
        }

        for planned_tier in &summary.planned_tiers {
            self.execution_runtime_planned_tier_total
                .with_label_values(&[workflow_id, planned_tier])
                .inc();
        }

        for reason in &summary.storage_decision_reasons {
            self.execution_runtime_storage_decision_total
                .with_label_values(&[workflow_id, reason])
                .inc();
        }

        self.execution_runtime_spill_events_total
            .with_label_values(&[workflow_id])
            .inc_by(summary.total_spill_events as u64);
        self.execution_runtime_spill_bytes_total
            .with_label_values(&[workflow_id])
            .inc_by(summary.total_spill_bytes as u64);
        self.execution_runtime_steps_with_metrics
            .with_label_values(&[workflow_id])
            .observe(summary.steps_with_runtime_metrics as f64);
        self.execution_runtime_steps_with_disk_storage
            .with_label_values(&[workflow_id])
            .observe(summary.steps_with_disk_storage as f64);
        self.execution_runtime_memory_high_water_mark_bytes
            .with_label_values(&[workflow_id])
            .observe(summary.max_memory_high_water_mark as f64);
        self.execution_runtime_reserved_spill_bytes
            .with_label_values(&[workflow_id])
            .observe(summary.max_reserved_spill_bytes as f64);

        self.set_workflow_memory_bytes(summary.max_memory_high_water_mark as f64);
    }

    /// Record that a streaming workflow has started with a given runtime/storage profile.
    pub fn stream_started(&self, workflow_id: &str, runtime: &StreamRuntimeSummary) {
        self.streams_active
            .with_label_values(&[workflow_id, &runtime.storage_backend])
            .inc();
        self.stream_runtime_summaries_total
            .with_label_values(&[workflow_id, &runtime.execution_engine])
            .inc();
        self.stream_runtime_storage_backend_total
            .with_label_values(&[workflow_id, &runtime.storage_backend])
            .inc();
        self.stream_runtime_checkpoint_interval_records
            .with_label_values(&[workflow_id])
            .observe(runtime.checkpoint_interval_records as f64);
    }

    /// Record that a streaming workflow has stopped.
    pub fn stream_stopped(&self, workflow_id: &str, runtime: &StreamRuntimeSummary) {
        self.streams_active
            .with_label_values(&[workflow_id, &runtime.storage_backend])
            .dec();
        self.stream_runtime_throughput
            .with_label_values(&[workflow_id])
            .set(0.0);
        self.stream_runtime_lag
            .with_label_values(&[workflow_id])
            .set(0.0);
        self.stream_runtime_active_workers
            .with_label_values(&[workflow_id])
            .set(0.0);
    }

    /// Update the latest runtime/control-plane stats for an active streaming workflow.
    pub fn record_stream_runtime_stats(&self, workflow_id: &str, stats: &StreamStats) {
        self.stream_runtime_records_processed
            .with_label_values(&[workflow_id])
            .set(stats.records_processed as f64);
        self.stream_runtime_throughput
            .with_label_values(&[workflow_id])
            .set(stats.throughput);
        self.stream_runtime_avg_latency_ms
            .with_label_values(&[workflow_id])
            .set(stats.avg_latency_ms as f64);
        self.stream_runtime_lag
            .with_label_values(&[workflow_id])
            .set(stats.lag as f64);
        self.stream_runtime_active_workers
            .with_label_values(&[workflow_id])
            .set(stats.active_workers as f64);
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

    #[test]
    fn test_execution_runtime_summary_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowMetrics::new(&registry).unwrap();
        let summary = ExecutionRuntimeMetricsSummary {
            steps_with_runtime_metrics: 3,
            steps_with_disk_storage: 2,
            total_spill_events: 4,
            total_spill_bytes: 8192,
            max_memory_high_water_mark: 16_384,
            max_reserved_spill_bytes: 8_192,
            max_execution_reserved_spill_bytes: 8_192,
            max_total_reserved_spill_bytes: 16_384,
            storage_backends: vec!["parquet".to_string(), "rocksdb".to_string()],
            planned_tiers: vec!["parquet".to_string()],
            storage_decision_reasons: vec!["planned".to_string(), "spill_required".to_string()],
        };

        metrics.record_execution_runtime_summary("wf_runtime", &summary);

        assert_eq!(
            metrics
                .execution_runtime_summaries_total
                .with_label_values(&["wf_runtime"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .execution_runtime_storage_backend_total
                .with_label_values(&["wf_runtime", "parquet"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .execution_runtime_planned_tier_total
                .with_label_values(&["wf_runtime", "parquet"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .execution_runtime_storage_decision_total
                .with_label_values(&["wf_runtime", "spill_required"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .execution_runtime_spill_events_total
                .with_label_values(&["wf_runtime"])
                .get(),
            4
        );
        assert_eq!(
            metrics
                .execution_runtime_spill_bytes_total
                .with_label_values(&["wf_runtime"])
                .get(),
            8192
        );
        assert_eq!(
            metrics
                .execution_runtime_steps_with_metrics
                .with_label_values(&["wf_runtime"])
                .get_sample_count(),
            1
        );
        assert_eq!(
            metrics
                .execution_runtime_memory_high_water_mark_bytes
                .with_label_values(&["wf_runtime"])
                .get_sample_count(),
            1
        );
        assert_eq!(metrics.workflow_memory_bytes.get(), 16_384.0);
    }

    #[test]
    fn test_stream_runtime_metrics() {
        let registry = Registry::new();
        let metrics = WorkflowMetrics::new(&registry).unwrap();
        let runtime = StreamRuntimeSummary {
            execution_engine: StreamRuntimeSummary::SIMPLE_KAFKA_LOOP_ENGINE.to_string(),
            storage_backend: "rocksdb".to_string(),
            persistent_state: true,
            state_location: Some("/tmp/stream-state".to_string()),
            checkpoint_interval_records:
                StreamRuntimeSummary::SIMPLE_KAFKA_LOOP_CHECKPOINT_INTERVAL_RECORDS,
            configured_checkpoint_interval_ms: 60_000,
        };
        let stats = StreamStats {
            records_processed: 125,
            throughput: 42.5,
            avg_latency_ms: 18,
            lag: 7,
            watermark: None,
            active_workers: 1,
            runtime: runtime.clone(),
        };

        metrics.stream_started("wf_stream", &runtime);
        metrics.record_stream_runtime_stats("wf_stream", &stats);

        assert_eq!(
            metrics
                .streams_active
                .with_label_values(&["wf_stream", "rocksdb"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .stream_runtime_summaries_total
                .with_label_values(&["wf_stream", "simple_kafka_loop"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .stream_runtime_storage_backend_total
                .with_label_values(&["wf_stream", "rocksdb"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .stream_runtime_records_processed
                .with_label_values(&["wf_stream"])
                .get(),
            125.0
        );
        assert_eq!(
            metrics
                .stream_runtime_lag
                .with_label_values(&["wf_stream"])
                .get(),
            7.0
        );

        metrics.stream_stopped("wf_stream", &runtime);
        assert_eq!(
            metrics
                .streams_active
                .with_label_values(&["wf_stream", "rocksdb"])
                .get(),
            0
        );
    }
}
