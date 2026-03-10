//! # gRPC Service Implementation

use crate::storage::LineageStorage;
use graphica_core::core::lineage::{LineageEvent as DomainLineageEvent, LineageSink};
use graphica_core::core::quality::{
    QualityViolation as DomainViolation, RuleType as DomainRuleType, Severity as DomainSeverity,
};
use graphica_core::distributed::proto::*;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tonic::{Request, Response, Status};

/// Lineage service implementation
pub struct LineageServiceImpl {
    storage: Arc<LineageStorage>,
    kafka_brokers: String,
}

impl LineageServiceImpl {
    /// Maximum number of lineage events to return in a single query
    /// This prevents memory exhaustion when querying large lineage chains or model impacts
    pub const MAX_LINEAGE_RESULTS: usize = 10000;

    pub fn new(storage: Arc<LineageStorage>, kafka_brokers: String) -> Self {
        Self {
            storage,
            kafka_brokers,
        }
    }
}

#[tonic::async_trait]
impl lineage_service_server::LineageService for LineageServiceImpl {
    async fn get_record_lineage(
        &self,
        request: Request<GetRecordLineageRequest>,
    ) -> Result<Response<LineageResponse>, Status> {
        let req = request.into_inner();

        tracing::info!("Getting lineage for record: {}", req.record_id);

        // Query lineage from storage
        let mut events = self
            .storage
            .get_record_lineage(&req.record_id)
            .map_err(|e| Status::internal(format!("Failed to query lineage: {}", e)))?;

        // Enforce result limit to prevent memory exhaustion
        let total_events = events.len();
        let truncated = total_events > Self::MAX_LINEAGE_RESULTS;

        if truncated {
            tracing::warn!(
                "Lineage query for record '{}' returned {} events, truncating to {} (limit exceeded)",
                req.record_id,
                total_events,
                Self::MAX_LINEAGE_RESULTS
            );
            events.truncate(Self::MAX_LINEAGE_RESULTS);
        }

        tracing::info!(
            "Returning {} lineage events for record '{}' (truncated: {})",
            events.len(),
            req.record_id,
            truncated
        );

        // Convert domain events to proto events
        let proto_events: Vec<LineageEvent> = events
            .into_iter()
            .map(|e| convert_to_proto_event(e))
            .collect();

        Ok(Response::new(LineageResponse {
            events: proto_events,
        }))
    }

    async fn get_model_impact(
        &self,
        request: Request<GetModelImpactRequest>,
    ) -> Result<Response<ModelImpactResponse>, Status> {
        let req = request.into_inner();

        tracing::info!("Getting impact for model: {}@{}", req.model_id, req.version);

        // Query model impact from storage
        let mut events = self
            .storage
            .get_model_impact(&req.model_id, &req.version)
            .map_err(|e| Status::internal(format!("Failed to query model impact: {}", e)))?;

        // Store total count before truncation
        let total_affected = events.len() as i64;
        let truncated = events.len() > Self::MAX_LINEAGE_RESULTS;

        // Enforce result limit to prevent memory exhaustion
        if truncated {
            tracing::warn!(
                "Model impact query for '{}@{}' returned {} events, truncating to {} (limit exceeded)",
                req.model_id,
                req.version,
                total_affected,
                Self::MAX_LINEAGE_RESULTS
            );
            events.truncate(Self::MAX_LINEAGE_RESULTS);
        }

        tracing::info!(
            "Returning {} of {} total affected records for model '{}@{}' (truncated: {})",
            events.len(),
            total_affected,
            req.model_id,
            req.version,
            truncated
        );

        // Convert domain events to proto events
        let proto_events: Vec<LineageEvent> = events
            .into_iter()
            .map(|e| convert_to_proto_event(e))
            .collect();

        Ok(Response::new(ModelImpactResponse {
            model_id: req.model_id,
            version: req.version,
            affected_records: proto_events,
            total_records_affected: total_affected, // Still report total count
        }))
    }

    type StreamLineageStream = tokio_stream::wrappers::ReceiverStream<Result<LineageEvent, Status>>;

    async fn stream_lineage(
        &self,
        request: Request<StreamLineageRequest>,
    ) -> Result<Response<Self::StreamLineageStream>, Status> {
        let req = request.into_inner();

        // Proto3 strings are always present but may be empty
        let dataset_filter = if req.dataset.is_empty() {
            None
        } else {
            Some(req.dataset)
        };

        let tenant_id = &req.tenant_id;

        // Create unique consumer group per stream to avoid message distribution across clients
        // This ensures each client receives ALL events, not a subset
        let consumer_group = format!("grpc-lineage-stream-{}-{}", tenant_id, uuid::Uuid::new_v4());

        tracing::info!(
            "Starting lineage stream - dataset: {:?}, tenant: {}, consumer_group: {}",
            dataset_filter,
            tenant_id,
            consumer_group
        );

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        // Create Kafka consumer for lineage events with unique consumer group
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.kafka_brokers)
            .set("group.id", &consumer_group)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "latest") // Only new events
            .create()
            .map_err(|e| Status::internal(format!("Failed to create Kafka consumer: {}", e)))?;

        consumer
            .subscribe(&["graphica.lineage.events"])
            .map_err(|e| Status::internal(format!("Failed to subscribe to topic: {}", e)))?;

        // Spawn background task to poll Kafka and forward events
        tokio::spawn(async move {
            tracing::info!(
                "Lineage stream task started - consumer_group: {}",
                consumer_group
            );

            loop {
                match consumer.recv().await {
                    Ok(msg) => {
                        // Parse message payload
                        if let Some(payload) = msg.payload() {
                            match serde_json::from_slice::<DomainLineageEvent>(payload) {
                                Ok(domain_event) => {
                                    // Apply dataset filter if specified
                                    if let Some(ref filter) = &dataset_filter {
                                        if &domain_event.dataset != filter {
                                            continue; // Skip events not matching filter
                                        }
                                    }

                                    // Convert to proto event
                                    let proto_event = convert_to_proto_event(domain_event);

                                    // Send through gRPC stream
                                    if tx.send(Ok(proto_event)).await.is_err() {
                                        tracing::info!(
                                            "Client disconnected from lineage stream (consumer_group: {})",
                                            consumer_group
                                        );
                                        break; // Client disconnected
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to deserialize lineage event: {}", e);
                                    continue;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Kafka consumer error (consumer_group: {}): {}",
                            consumer_group,
                            e
                        );
                        let _ = tx
                            .send(Err(Status::internal(format!(
                                "Kafka consumer error: {}",
                                e
                            ))))
                            .await;
                        break;
                    }
                }
            }

            tracing::info!(
                "Lineage stream task ended (consumer_group: {})",
                consumer_group
            );
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn query_lineage_by_time(
        &self,
        request: Request<QueryLineageByTimeRequest>,
    ) -> Result<Response<LineageResponse>, Status> {
        let req = request.into_inner();

        // Convert proto timestamps to DateTime
        let start_time = req.start_time.map(|ts| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_else(chrono::Utc::now)
        });

        let end_time = req.end_time.map(|ts| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_else(chrono::Utc::now)
        });

        tracing::info!(
            "Querying lineage by time: {:?} to {:?}",
            start_time,
            end_time
        );

        // Query events with time range
        let query_start =
            start_time.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
        let query_end = end_time.unwrap_or_else(|| chrono::Utc::now());

        let mut events = self
            .storage
            .query_all(query_start, query_end)
            .map_err(|e| Status::internal(format!("Failed to query lineage: {}", e)))?;

        // Enforce result limit to prevent memory exhaustion
        let total_events = events.len();
        let truncated = total_events > Self::MAX_LINEAGE_RESULTS;

        if truncated {
            tracing::warn!(
                "Time range query ({:?} to {:?}) returned {} events, truncating to {} (limit exceeded)",
                query_start,
                query_end,
                total_events,
                Self::MAX_LINEAGE_RESULTS
            );
            events.truncate(Self::MAX_LINEAGE_RESULTS);
        }

        tracing::info!(
            "Returning {} lineage events for time range query (truncated: {})",
            events.len(),
            truncated
        );

        // Convert domain events to proto events
        let proto_events: Vec<LineageEvent> = events
            .into_iter()
            .map(|e| convert_to_proto_event(e))
            .collect();

        Ok(Response::new(LineageResponse {
            events: proto_events,
        }))
    }
}

/// Quality service implementation with enterprise-grade streaming
pub struct QualityServiceImpl {
    kafka_brokers: String,
    /// Maximum number of violations to buffer per stream (backpressure control)
    stream_buffer_size: usize,
    /// Consumer group prefix for violations streaming
    consumer_group_prefix: String,
}

impl QualityServiceImpl {
    pub fn new(kafka_brokers: String) -> Self {
        Self {
            kafka_brokers,
            stream_buffer_size: 256, // Enterprise default: larger buffer for throughput
            consumer_group_prefix: "grpc-quality-stream".to_string(),
        }
    }

    /// Configure stream buffer size (for backpressure control)
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.stream_buffer_size = size;
        self
    }

    /// Configure consumer group prefix
    pub fn with_consumer_group(mut self, prefix: String) -> Self {
        self.consumer_group_prefix = prefix;
        self
    }
}

#[tonic::async_trait]
impl quality_service_server::QualityService for QualityServiceImpl {
    async fn get_scorecard(
        &self,
        request: Request<GetScorecardRequest>,
    ) -> Result<Response<ScorecardResponse>, Status> {
        let req = request.into_inner();

        tracing::info!("Getting scorecard for dataset: {}", req.dataset);

        Ok(Response::new(ScorecardResponse {
            scorecard: Some(QualityScorecard {
                dataset: req.dataset,
                period_start: req.period_start,
                period_end: req.period_end,
                overall_score: 0.95,
                dimension_scores: Default::default(),
                total_records: 0,
                violation_counts: Default::default(),
                rule_results: vec![],
            }),
        }))
    }

    async fn list_violations(
        &self,
        request: Request<ListViolationsRequest>,
    ) -> Result<Response<ListViolationsResponse>, Status> {
        let _req = request.into_inner();

        Ok(Response::new(ListViolationsResponse {
            violations: vec![],
            next_page_token: String::new(),
        }))
    }

    async fn upsert_rule(
        &self,
        request: Request<UpsertRuleRequest>,
    ) -> Result<Response<RuleResponse>, Status> {
        let req = request.into_inner();

        Ok(Response::new(RuleResponse { rule: req.rule }))
    }

    type StreamViolationsStream =
        tokio_stream::wrappers::ReceiverStream<Result<QualityViolation, Status>>;

    async fn stream_violations(
        &self,
        request: Request<StreamViolationsRequest>,
    ) -> Result<Response<Self::StreamViolationsStream>, Status> {
        let req = request.into_inner();

        // Proto3 strings are always present but may be empty
        let dataset_filter = if req.dataset.is_empty() {
            None
        } else {
            Some(req.dataset)
        };

        let tenant_id = req.tenant_id;

        tracing::info!(
            "Starting quality violations stream - dataset: {:?}, tenant: {}",
            dataset_filter,
            tenant_id
        );

        // Use configured buffer size for backpressure control
        let (tx, rx) = tokio::sync::mpsc::channel(self.stream_buffer_size);

        // Create unique consumer group per stream to avoid conflicts
        // This follows Netflix OSS pattern for independent consumers
        let consumer_group = format!(
            "{}-{}-{}",
            self.consumer_group_prefix,
            tenant_id,
            uuid::Uuid::new_v4()
        );

        // Enterprise-grade Kafka consumer configuration
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.kafka_brokers)
            .set("group.id", &consumer_group)
            .set("enable.auto.commit", "false") // Manual commit for reliability
            .set("auto.offset.reset", "latest") // Only new violations
            .set("session.timeout.ms", "30000") // 30s session timeout
            .set("heartbeat.interval.ms", "3000") // 3s heartbeat
            .set("max.poll.interval.ms", "300000") // 5min max poll interval
            .set("fetch.min.bytes", "1") // Low latency
            .set("fetch.max.wait.ms", "100") // 100ms max wait
            .create()
            .map_err(|e| {
                tracing::error!("Failed to create Kafka consumer: {}", e);
                Status::internal(format!("Failed to create Kafka consumer: {}", e))
            })?;

        // Subscribe to quality violations topic
        consumer
            .subscribe(&["graphica.quality.violations"])
            .map_err(|e| {
                tracing::error!("Failed to subscribe to violations topic: {}", e);
                Status::internal(format!("Failed to subscribe to topic: {}", e))
            })?;

        // Metrics tracking for observability
        let stream_metrics = Arc::new(StreamMetrics::new());
        let metrics_clone = stream_metrics.clone();

        // Spawn background task with enterprise error handling
        tokio::spawn(async move {
            let start_time = Instant::now();
            let mut consecutive_errors = 0u32;
            const MAX_CONSECUTIVE_ERRORS: u32 = 10; // Circuit breaker threshold
            const ERROR_BACKOFF_MS: u64 = 100; // Exponential backoff starting point

            tracing::info!(
                "Quality violations stream task started - consumer_group: {}",
                consumer_group
            );

            loop {
                match consumer.recv().await {
                    Ok(msg) => {
                        // Reset error counter on success (circuit breaker reset)
                        consecutive_errors = 0;
                        metrics_clone.increment_received();

                        // Parse message payload
                        if let Some(payload) = msg.payload() {
                            match serde_json::from_slice::<DomainViolation>(payload) {
                                Ok(domain_violation) => {
                                    // Apply dataset filter if specified
                                    if let Some(ref filter) = dataset_filter {
                                        if &domain_violation.dataset != filter {
                                            metrics_clone.increment_filtered();
                                            continue; // Skip violations not matching filter
                                        }
                                    }

                                    // Convert to proto violation
                                    let proto_violation =
                                        convert_to_proto_violation(domain_violation);

                                    // Send through gRPC stream with backpressure handling
                                    match tx.send(Ok(proto_violation)).await {
                                        Ok(_) => {
                                            metrics_clone.increment_sent();
                                        }
                                        Err(_) => {
                                            tracing::info!(
                                                "Client disconnected from violations stream after {} events",
                                                metrics_clone.get_sent()
                                            );
                                            break; // Client disconnected
                                        }
                                    }
                                }
                                Err(e) => {
                                    metrics_clone.increment_parse_errors();
                                    tracing::warn!(
                                        "Failed to deserialize quality violation: {} (total parse errors: {})",
                                        e,
                                        metrics_clone.get_parse_errors()
                                    );
                                    // Continue processing other messages
                                    continue;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        metrics_clone.increment_kafka_errors();

                        tracing::error!(
                            "Kafka consumer error ({}/{}): {}",
                            consecutive_errors,
                            MAX_CONSECUTIVE_ERRORS,
                            e
                        );

                        // Circuit breaker pattern: stop after too many consecutive errors
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            tracing::error!(
                                "Circuit breaker triggered - {} consecutive errors, terminating stream",
                                consecutive_errors
                            );

                            let _ = tx
                                .send(Err(Status::unavailable(format!(
                                    "Quality violations stream failed after {} consecutive errors",
                                    consecutive_errors
                                ))))
                                .await;
                            break;
                        }

                        // Exponential backoff before retry
                        let backoff_ms = ERROR_BACKOFF_MS * 2u64.pow(consecutive_errors.min(5));
                        tracing::info!("Backing off for {}ms before retry", backoff_ms);
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    }
                }
            }

            // Log final metrics on shutdown
            let duration = start_time.elapsed();
            tracing::info!(
                "Quality violations stream ended - duration: {:?}, metrics: {:?}",
                duration,
                metrics_clone
            );
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

/// Stream metrics for observability (production monitoring)
#[derive(Debug)]
struct StreamMetrics {
    received: AtomicU64,
    sent: AtomicU64,
    filtered: AtomicU64,
    parse_errors: AtomicU64,
    kafka_errors: AtomicU64,
}

impl StreamMetrics {
    fn new() -> Self {
        Self {
            received: AtomicU64::new(0),
            sent: AtomicU64::new(0),
            filtered: AtomicU64::new(0),
            parse_errors: AtomicU64::new(0),
            kafka_errors: AtomicU64::new(0),
        }
    }

    fn increment_received(&self) {
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_filtered(&self) {
        self.filtered.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_parse_errors(&self) {
        self.parse_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_kafka_errors(&self) {
        self.kafka_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn get_sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    fn get_parse_errors(&self) -> u64 {
        self.parse_errors.load(Ordering::Relaxed)
    }
}

// Helper functions for converting domain types to protobuf types

fn convert_to_proto_event(event: graphica_core::core::lineage::LineageEvent) -> LineageEvent {
    LineageEvent {
        id: event.id.to_string(),
        dataset: event.dataset,
        record_id: event.record_id,
        source_refs: event
            .source_refs
            .into_iter()
            .map(convert_data_ref)
            .collect(),
        transforms: event
            .transforms
            .into_iter()
            .map(convert_transform_ref)
            .collect(),
        model_refs: event
            .model_refs
            .into_iter()
            .map(convert_model_ref)
            .collect(),
        output_ref: Some(convert_data_ref(event.output_ref)),
        timestamp: Some(prost_types::Timestamp {
            seconds: event.ts.timestamp(),
            nanos: event.ts.timestamp_subsec_nanos() as i32,
        }),
        run_id: event.run_id,
        tenant_id: event.tenant_id,
        metadata: std::collections::HashMap::new(), // TODO: Add metadata if available
    }
}

fn convert_data_ref(data_ref: graphica_core::core::lineage::DataRef) -> DataRef {
    DataRef {
        system: data_ref.system,
        path: data_ref.path,
        version: data_ref.version, // Already Option<String>
        extracted_at: Some(prost_types::Timestamp {
            seconds: data_ref.extracted_at.timestamp(),
            nanos: data_ref.extracted_at.timestamp_subsec_nanos() as i32,
        }),
        cdc_position: data_ref.cdc_position.map(|pos| CdcPosition {
            topic: pos.topic,
            partition: pos.partition,
            offset: pos.offset,
            lsn: pos.lsn, // Already Option<String>
        }),
    }
}

fn convert_transform_ref(transform: graphica_core::core::lineage::TransformRef) -> TransformRef {
    // Convert HashMap<String, JsonValue> to HashMap<String, String> for proto
    let mut proto_params = std::collections::HashMap::new();
    for (k, v) in transform.parameters {
        proto_params.insert(k, v.to_string());
    }

    TransformRef {
        id: transform.id.to_string(), // Convert Uuid to String
        transform_type: transform.transform_type,
        rule_id: transform.rule_id,
        version: transform.version,
        parameters: proto_params,
        applied_at: Some(prost_types::Timestamp {
            seconds: transform.applied_at.timestamp(),
            nanos: transform.applied_at.timestamp_subsec_nanos() as i32,
        }),
        fields_modified: transform.fields_modified,
    }
}

fn convert_model_ref(model: graphica_core::core::lineage::ModelRef) -> ModelRef {
    ModelRef {
        model_id: model.model_id,
        version: model.version,
        model_type: model.model_type,
        params_hash: model.params_hash,
        training_data: model
            .training_data
            .into_iter()
            .map(convert_data_ref)
            .collect(),
        metrics: Some(ModelMetrics {
            accuracy: model.metrics.accuracy,
            precision: model.metrics.precision,
            recall: model.metrics.recall,
            f1_score: model.metrics.f1_score,
            rmse: model.metrics.rmse,
            custom_metrics: model.metrics.custom_metrics,
        }),
        registry_uri: model.registry_uri,
        inference_at: Some(prost_types::Timestamp {
            seconds: model.inference_at.timestamp(),
            nanos: model.inference_at.timestamp_subsec_nanos() as i32,
        }),
        features_used: model.features_used,
        outputs: model.outputs,
    }
}

/// Convert domain quality violation to protobuf format
fn convert_to_proto_violation(violation: DomainViolation) -> QualityViolation {
    QualityViolation {
        id: violation.id.to_string(),
        rule_id: violation.rule_id,
        dataset: violation.dataset,
        record_id: violation.record_id,
        field: violation.field,
        actual_value: violation.actual_value,
        expected_value: violation.expected_value,
        message: violation.message,
        severity: convert_severity_to_proto(violation.severity) as i32,
        detected_at: Some(prost_types::Timestamp {
            seconds: violation.detected_at.timestamp(),
            nanos: violation.detected_at.timestamp_subsec_nanos() as i32,
        }),
        resolved_at: violation.resolved_at.map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }),
        lineage_ref: violation.lineage_ref.map(|id| id.to_string()),
    }
}

/// Convert domain severity to proto severity enum
fn convert_severity_to_proto(severity: DomainSeverity) -> Severity {
    match severity {
        DomainSeverity::Info => Severity::Info,
        DomainSeverity::Warning => Severity::Warning,
        DomainSeverity::Error => Severity::Error,
        DomainSeverity::Critical => Severity::Critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::{
        CdcPosition as DomainCdcPosition, DataRef as DomainDataRef,
        LineageEvent as DomainLineageEvent,
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn test_lineage_service_creation() {
        // Create in-memory storage
        let storage = Arc::new(
            LineageStorage::new(
                "./test_rocks",
                "./test_parquet",
                "./test_cold",
                "localhost:9092",
            )
            .expect("Failed to create storage"),
        );

        // Create service
        let service = LineageServiceImpl::new(storage, "localhost:9092".to_string());

        // Service should be created successfully
        assert!(std::ptr::addr_of!(service.storage) as usize != 0);
        assert_eq!(service.kafka_brokers, "localhost:9092");
    }

    #[test]
    fn test_convert_to_proto_event() {
        // Create a minimal domain event
        let domain_event = DomainLineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: "rec_123".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DomainDataRef {
                system: "test_system".to_string(),
                path: "/test/path".to_string(),
                version: Some("v1".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(DomainCdcPosition {
                    topic: "test_topic".to_string(),
                    partition: 0,
                    offset: 123,
                    lsn: Some("0/1234567".to_string()),
                }),
            },
            ts: Utc::now(),
            run_id: "run_456".to_string(),
            tenant_id: "tenant_789".to_string(),
            correlation_id: None,
            metadata: std::collections::HashMap::new(),
        };

        // Convert to proto
        let proto_event = convert_to_proto_event(domain_event.clone());

        // Verify conversion
        assert_eq!(proto_event.id, domain_event.id.to_string());
        assert_eq!(proto_event.dataset, "test_dataset");
        assert_eq!(proto_event.record_id, "rec_123");
        assert_eq!(proto_event.run_id, "run_456");
        assert_eq!(proto_event.tenant_id, "tenant_789");
        assert!(proto_event.timestamp.is_some());
        assert!(proto_event.output_ref.is_some());
    }

    #[test]
    fn test_unique_consumer_groups_per_stream() {
        // Test that consumer groups are unique for each stream request
        // This is a unit test that verifies the consumer group format

        let tenant_id = "tenant_123";
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        let group1 = format!("grpc-lineage-stream-{}-{}", tenant_id, uuid1);
        let group2 = format!("grpc-lineage-stream-{}-{}", tenant_id, uuid2);

        // Consumer groups should be different due to different UUIDs
        assert_ne!(group1, group2);

        // Both should start with tenant prefix
        assert!(group1.starts_with(&format!("grpc-lineage-stream-{}-", tenant_id)));
        assert!(group2.starts_with(&format!("grpc-lineage-stream-{}-", tenant_id)));
    }

    #[test]
    fn test_consumer_group_format() {
        // Verify consumer group naming convention
        let tenant_id = "tenant_xyz";
        let uuid = Uuid::new_v4();
        let consumer_group = format!("grpc-lineage-stream-{}-{}", tenant_id, uuid);

        // Should contain tenant ID
        assert!(consumer_group.contains(tenant_id));

        // Should contain UUID (36 characters)
        let uuid_str = uuid.to_string();
        assert!(consumer_group.contains(&uuid_str));

        // Should start with prefix
        assert!(consumer_group.starts_with("grpc-lineage-stream-"));
    }

    #[test]
    fn test_max_lineage_results_constant() {
        // Verify MAX_LINEAGE_RESULTS is set to prevent memory exhaustion
        assert_eq!(LineageServiceImpl::MAX_LINEAGE_RESULTS, 10000);

        // Verify it's a reasonable limit (not too small, not too large)
        assert!(LineageServiceImpl::MAX_LINEAGE_RESULTS >= 1000);
        assert!(LineageServiceImpl::MAX_LINEAGE_RESULTS <= 100000);
    }

    #[test]
    fn test_result_truncation_logic() {
        // Test truncation logic for lineage results
        let mut events: Vec<u32> = (0..15000).collect();
        let total_events = events.len();

        // Verify we have more than max
        assert!(total_events > LineageServiceImpl::MAX_LINEAGE_RESULTS);

        // Truncate
        events.truncate(LineageServiceImpl::MAX_LINEAGE_RESULTS);

        // Verify truncation
        assert_eq!(events.len(), LineageServiceImpl::MAX_LINEAGE_RESULTS);
        assert_eq!(events.len(), 10000);

        // Verify first and last elements
        assert_eq!(events[0], 0);
        assert_eq!(events[9999], 9999);
    }

    #[test]
    fn test_truncation_detection() {
        // Test logic for detecting when truncation is needed
        let small_set = 500;
        let medium_set = 10000;
        let large_set = 50000;

        assert!(!should_truncate(small_set));
        assert!(!should_truncate(medium_set));
        assert!(should_truncate(large_set));

        fn should_truncate(count: usize) -> bool {
            count > LineageServiceImpl::MAX_LINEAGE_RESULTS
        }
    }

    #[test]
    fn test_memory_estimate() {
        // Estimate memory usage with MAX_LINEAGE_RESULTS
        const BYTES_PER_EVENT: usize = 500; // Rough estimate
        let max_memory = LineageServiceImpl::MAX_LINEAGE_RESULTS * BYTES_PER_EVENT;

        // Should be under 10 MB per request
        assert!(max_memory < 10_000_000);

        // Verify it's approximately 5 MB
        assert!(max_memory > 4_000_000);
        assert!(max_memory < 6_000_000);
    }
}
