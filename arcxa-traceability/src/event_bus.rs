use crate::TraceabilityManager;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use graphica_core::migration_evidence::{
    MigrationEvidenceBrokerReachability, MigrationEvidenceEventBusLagState,
    MigrationEvidenceEventBusMode, MigrationEvidenceEventBusStatus,
    MigrationEvidenceEventConsumerState, MigrationEvidenceEventEnvelope,
    MigrationEvidencePartitionLagStatus, MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION,
};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
    Offset, TopicPartitionList,
    ClientConfig,
};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::sync::RwLock;
use std::sync::Arc;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct KafkaTraceabilityConsumerConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub consumer_group: String,
    pub client_id: Option<String>,
    pub retry_delay: Duration,
}

impl KafkaTraceabilityConsumerConfig {
    pub fn validate(&self) -> Result<()> {
        if self.bootstrap_servers.trim().is_empty() {
            return Err(anyhow!("Kafka bootstrap servers cannot be empty"));
        }
        if self.topic.trim().is_empty() {
            return Err(anyhow!("Kafka topic cannot be empty"));
        }
        if self.consumer_group.trim().is_empty() {
            return Err(anyhow!("Kafka consumer group cannot be empty"));
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct EventBusRuntimeMonitor {
    inner: Arc<RwLock<MigrationEvidenceEventBusStatus>>,
}

impl EventBusRuntimeMonitor {
    pub fn direct() -> Self {
        Self::default()
    }

    pub fn kafka(config: &KafkaTraceabilityConsumerConfig) -> Self {
        let now = Utc::now();
        Self {
            inner: Arc::new(RwLock::new(MigrationEvidenceEventBusStatus {
                mode: MigrationEvidenceEventBusMode::Kafka,
                async_delivery_enabled: true,
                consumer_state: MigrationEvidenceEventConsumerState::Recovering,
                bootstrap_servers: Some(config.bootstrap_servers.clone()),
                topic: Some(config.topic.clone()),
                consumer_group: Some(config.consumer_group.clone()),
                last_state_changed_at: Some(now),
                ..MigrationEvidenceEventBusStatus::default()
            })),
        }
    }

    pub async fn snapshot(&self) -> MigrationEvidenceEventBusStatus {
        self.inner.read().await.clone()
    }

    pub async fn mark_running(&self) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.consumer_state = MigrationEvidenceEventConsumerState::Running;
        guard.last_state_changed_at = Some(now);
        guard.startup_completed_at.get_or_insert(now);
        guard.startup_failure_reason = None;
        guard.last_error = None;
    }

    pub async fn mark_retry(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.consumer_state = MigrationEvidenceEventConsumerState::Recovering;
        guard.retry_attempt_count += 1;
        guard.last_retry_at = Some(now);
        guard.last_state_changed_at = Some(now);
        guard.last_error = Some(error.into());
    }

    pub async fn mark_poison_message(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        guard.malformed_message_count += 1;
        guard.last_error = Some(error.into());
    }

    pub async fn mark_processed(&self) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.consumer_state = MigrationEvidenceEventConsumerState::Running;
        guard.processed_message_count += 1;
        guard.last_consumed_at = Some(now);
        guard.last_successful_ingest_at = Some(now);
        guard.startup_failure_reason = None;
        guard.last_error = None;
    }

    async fn observe_probe(&self, probe: KafkaRuntimeProbe) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.broker_reachability = probe.broker_reachability;
        guard.discovered_broker_count = probe.discovered_broker_count;
        guard.assigned_partitions = probe.assigned_partitions;
        guard.topic_partition_count = probe.topic_partition_count;
        guard.partition_lag = probe.partition_lag;
        guard.estimated_lag_message_count = probe.estimated_lag_message_count;
        guard.lag_state = probe.lag_state;
        guard.lag_observed_at = Some(now);
        guard.last_assignment_at = Some(now);
        guard.last_broker_probe_at = Some(now);
        guard.lag_diagnostics = probe.lag_diagnostics;
        guard.last_error = None;
    }

    pub async fn mark_broker_probe_failed(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.broker_reachability = MigrationEvidenceBrokerReachability::Unreachable;
        guard.last_broker_probe_at = Some(now);
        guard.last_error = Some(error.into());
    }

    pub async fn mark_assignment_probe_failed(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.broker_reachability = if guard.async_delivery_enabled {
            MigrationEvidenceBrokerReachability::Degraded
        } else {
            MigrationEvidenceBrokerReachability::Unknown
        };
        guard.last_assignment_at = Some(now);
        guard.last_error = Some(error.into());
    }

    pub async fn mark_stopped(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        guard.last_state_changed_at = Some(Utc::now());
        guard.consumer_state = MigrationEvidenceEventConsumerState::Stopped;
        guard.last_error = Some(error.into());
    }

    pub async fn mark_startup_failed(&self, error: impl Into<String>) {
        let mut guard = self.inner.write().await;
        let error = error.into();
        let now = Utc::now();
        guard.consumer_state = MigrationEvidenceEventConsumerState::Stopped;
        guard.last_state_changed_at = Some(now);
        guard.startup_failure_reason = Some(error.clone());
        guard.last_error = Some(error);
    }
}

pub struct KafkaTraceabilityEventConsumer {
    consumer: StreamConsumer,
    topic: String,
    consumer_group: String,
    retry_delay: Duration,
    runtime_monitor: EventBusRuntimeMonitor,
}

impl KafkaTraceabilityEventConsumer {
    pub fn new(config: KafkaTraceabilityConsumerConfig, runtime_monitor: EventBusRuntimeMonitor) -> Result<Self> {
        config.validate()?;

        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &config.bootstrap_servers)
            .set("group.id", &config.consumer_group)
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "earliest");

        if let Some(client_id) = &config.client_id {
            if !client_id.trim().is_empty() {
                client_config.set("client.id", client_id);
            }
        }

        let consumer: StreamConsumer = client_config
            .create()
            .context("failed to create Kafka traceability consumer")?;
        consumer
            .subscribe(&[&config.topic])
            .with_context(|| format!("failed to subscribe to Kafka topic '{}'", config.topic))?;

        Ok(Self {
            consumer,
            topic: config.topic,
            consumer_group: config.consumer_group,
            retry_delay: config.retry_delay,
            runtime_monitor,
        })
    }

    pub fn spawn(self, manager: TraceabilityManager) -> JoinHandle<()> {
        let runtime_monitor = self.runtime_monitor.clone();
        tokio::spawn(async move {
            if let Err(error) = self.run(manager).await {
                runtime_monitor.mark_stopped(error.to_string()).await;
                error!(error = %error, "migration evidence Kafka consumer exited unexpectedly");
            }
        })
    }

    pub async fn run(self, manager: TraceabilityManager) -> Result<()> {
        self.runtime_monitor.mark_running().await;
        self.refresh_runtime_probe(&CommitCursor {
            topic: self.topic.clone(),
            partition: -1,
            offset: -1,
        })
        .await;
        info!(
            topic = %self.topic,
            consumer_group = %self.consumer_group,
            "starting migration evidence Kafka traceability consumer"
        );
        loop {
            let message = self
                .consumer
                .recv()
                .await
                .context("failed to receive migration evidence Kafka message")?;
            let payload = message
                .payload_view::<str>()
                .transpose()
                .context("failed to decode Kafka payload as UTF-8")?
                .ok_or_else(|| anyhow!("migration evidence Kafka message payload was empty"))?
                .to_string();
            let cursor = CommitCursor::from_message(&message);
            self.process_with_retry(&manager, payload, cursor).await?;
        }
    }

    async fn process_with_retry(
        &self,
        manager: &TraceabilityManager,
        payload: String,
        cursor: CommitCursor,
    ) -> Result<()> {
        loop {
            match self.process_message(manager, &payload, &cursor).await {
                Ok(ProcessOutcome::Committed) | Ok(ProcessOutcome::SkippedPoison) => return Ok(()),
                Err(error) => {
                    self.runtime_monitor.mark_retry(error.to_string()).await;
                    warn!(
                        error = %error,
                        topic = %self.topic,
                        consumer_group = %self.consumer_group,
                        "failed to apply migration evidence event from Kafka; will retry without committing offset"
                    );
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        }
    }

    async fn process_message(
        &self,
        manager: &TraceabilityManager,
        payload: &str,
        cursor: &CommitCursor,
    ) -> Result<ProcessOutcome> {
        let envelope = match decode_event_envelope(payload) {
            Ok(envelope) => envelope,
            Err(error) => {
                self.runtime_monitor.mark_poison_message(error.to_string()).await;
                warn!(
                    error = %error,
                    topic = %self.topic,
                    consumer_group = %self.consumer_group,
                    "dropping malformed migration evidence Kafka message after logging the error"
                );
                self.commit_cursor(cursor, "malformed Kafka message")?;
                return Ok(ProcessOutcome::SkippedPoison);
            }
        };

        manager.ingest_events(vec![envelope.event]).await?;
        self.runtime_monitor.mark_processed().await;
        self.refresh_runtime_probe(&cursor).await;
        self.commit_cursor(cursor, "processed Kafka message")?;
        Ok(ProcessOutcome::Committed)
    }

    async fn refresh_runtime_probe(&self, cursor: &CommitCursor) {
        match self.probe_runtime_state() {
            Ok(probe) => self.runtime_monitor.observe_probe(probe).await,
            Err(error) => {
                let error_text = error.to_string();
                if error_text.contains("metadata") || error_text.contains("broker") {
                    self.runtime_monitor.mark_broker_probe_failed(error_text).await;
                } else {
                    self.runtime_monitor.mark_assignment_probe_failed(error_text).await;
                }
                warn!(
                    topic = %cursor.topic,
                    partition = cursor.partition,
                    offset = cursor.offset,
                    error = %error,
                    "failed to refresh migration evidence Kafka runtime probe"
                );
            }
        }
    }

    fn probe_runtime_state(&self) -> Result<KafkaRuntimeProbe> {
        let metadata = self
            .consumer
            .fetch_metadata(Some(&self.topic), Duration::from_secs(2))
            .context("failed to fetch Kafka metadata for migration evidence runtime probe")?;
        let topic = metadata
            .topics()
            .iter()
            .find(|topic| topic.name() == self.topic)
            .ok_or_else(|| anyhow!("Kafka topic '{}' not found in runtime metadata", self.topic))?;

        let assignment = self
            .consumer
            .assignment()
            .context("failed to read Kafka partition assignment for migration evidence runtime probe")?;
        let positions = self
            .consumer
            .position()
            .context("failed to read Kafka consumer position for migration evidence runtime probe")?;

        let mut partition_lag = Vec::new();
        let mut assigned_partitions = Vec::new();
        let mut total_lag = 0_u64;

        for elem in assignment.elements() {
            let partition = elem.partition();
            assigned_partitions.push(partition);

            let current_offset = positions
                .find_partition(&self.topic, partition)
                .and_then(|entry| entry.offset().to_raw());
            let (_, high) = self
                .consumer
                .fetch_watermarks(&self.topic, partition, Duration::from_secs(2))
                .with_context(|| format!("failed to fetch Kafka watermarks for partition {partition}"))?;
            let estimated_lag_message_count = current_offset
                .map(|offset| high.saturating_sub(offset))
                .and_then(|lag| u64::try_from(lag).ok());
            if let Some(lag) = estimated_lag_message_count {
                total_lag = total_lag.saturating_add(lag);
            }

            partition_lag.push(MigrationEvidencePartitionLagStatus {
                partition,
                current_offset,
                high_watermark: Some(high),
                estimated_lag_message_count,
            });
        }

        assigned_partitions.sort_unstable();
        partition_lag.sort_by_key(|status| status.partition);

        let estimated_lag_message_count = (!partition_lag.is_empty()).then_some(total_lag);
        let lag_state = match estimated_lag_message_count {
            Some(0) => MigrationEvidenceEventBusLagState::CaughtUp,
            Some(_) => MigrationEvidenceEventBusLagState::Backlog,
            None => MigrationEvidenceEventBusLagState::Unknown,
        };
        let lag_diagnostics = Some(match estimated_lag_message_count {
            Some(0) if !assigned_partitions.is_empty() => {
                format!("consumer is caught up across {} assigned partition(s)", assigned_partitions.len())
            }
            Some(total) if !assigned_partitions.is_empty() => {
                format!(
                    "estimated backlog of {total} message(s) across {} assigned partition(s)",
                    assigned_partitions.len()
                )
            }
            _ => "consumer has no partition assignment yet".to_string(),
        });

        Ok(KafkaRuntimeProbe {
            broker_reachability: if metadata.brokers().is_empty() {
                MigrationEvidenceBrokerReachability::Unreachable
            } else {
                MigrationEvidenceBrokerReachability::Reachable
            },
            discovered_broker_count: Some(metadata.brokers().len() as u32),
            assigned_partitions,
            topic_partition_count: Some(topic.partitions().len() as u32),
            partition_lag,
            estimated_lag_message_count,
            lag_state,
            lag_diagnostics,
        })
    }

    fn commit_cursor(&self, cursor: &CommitCursor, context: &str) -> Result<()> {
        let mut partitions = TopicPartitionList::new();
        partitions
            .add_partition_offset(&cursor.topic, cursor.partition, Offset::Offset(cursor.offset + 1))
            .with_context(|| format!("failed to stage offset commit for {}", context))?;
        self.consumer
            .commit(&partitions, CommitMode::Sync)
            .with_context(|| format!("failed to commit {}", context))
    }
}

enum ProcessOutcome {
    Committed,
    SkippedPoison,
}

#[derive(Debug, Clone)]
struct KafkaRuntimeProbe {
    broker_reachability: MigrationEvidenceBrokerReachability,
    discovered_broker_count: Option<u32>,
    assigned_partitions: Vec<i32>,
    topic_partition_count: Option<u32>,
    partition_lag: Vec<MigrationEvidencePartitionLagStatus>,
    estimated_lag_message_count: Option<u64>,
    lag_state: MigrationEvidenceEventBusLagState,
    lag_diagnostics: Option<String>,
}

#[derive(Debug, Clone)]
struct CommitCursor {
    topic: String,
    partition: i32,
    offset: i64,
}

impl CommitCursor {
    fn from_message(message: &impl Message) -> Self {
        Self {
            topic: message.topic().to_string(),
            partition: message.partition(),
            offset: message.offset(),
        }
    }
}

fn decode_event_envelope(payload: &str) -> Result<MigrationEvidenceEventEnvelope> {
    let envelope: MigrationEvidenceEventEnvelope =
        serde_json::from_str(payload).context("failed to deserialize migration evidence event envelope")?;
    if envelope.schema_version != MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported migration evidence event schema version {}",
            envelope.schema_version
        ));
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{
        MigrationConnectorVendor, MigrationEvidenceArtifactType, MigrationEvidenceEvent,
    };

    #[test]
    fn decode_event_envelope_round_trips() {
        let event = MigrationEvidenceEvent::new(
            "connector-1",
            "run-1",
            MigrationConnectorVendor::IbmRapidMove,
            "program-1",
            "object-1",
            MigrationEvidenceArtifactType::Object,
            None,
            serde_json::json!({"object_id": "object-1"}),
        );
        let envelope = MigrationEvidenceEventEnvelope::from_event(event);
        let payload = serde_json::to_string(&envelope).unwrap();

        let decoded = decode_event_envelope(&payload).unwrap();

        assert_eq!(decoded.schema_version, MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION);
        assert_eq!(decoded.event.program_id, "program-1");
    }

    #[test]
    fn decode_event_envelope_rejects_unknown_schema_versions() {
        let payload = serde_json::json!({
            "schema_version": 99,
            "published_at": chrono::Utc::now(),
            "connector_id": "connector-1",
            "run_id": "run-1",
            "program_id": "program-1",
            "object_id": "object-1",
            "event": MigrationEvidenceEvent::new(
                "connector-1",
                "run-1",
                MigrationConnectorVendor::Generic,
                "program-1",
                "object-1",
                MigrationEvidenceArtifactType::Object,
                None,
                serde_json::json!({})
            )
        });

        let error = decode_event_envelope(&payload.to_string()).unwrap_err();
        assert!(error.to_string().contains("unsupported migration evidence event schema version"));
    }

    #[tokio::test]
    async fn runtime_monitor_tracks_kafka_consumer_posture() {
        let monitor = EventBusRuntimeMonitor::kafka(&KafkaTraceabilityConsumerConfig {
            bootstrap_servers: "kafka:9092".to_string(),
            topic: "migration-evidence".to_string(),
            consumer_group: "arcxa-traceability".to_string(),
            client_id: Some("arcxa-test".to_string()),
            retry_delay: Duration::from_secs(1),
        });

        let initial = monitor.snapshot().await;
        assert_eq!(initial.mode, MigrationEvidenceEventBusMode::Kafka);
        assert!(initial.async_delivery_enabled);
        assert_eq!(
            initial.consumer_state,
            MigrationEvidenceEventConsumerState::Recovering
        );
        assert_eq!(initial.topic.as_deref(), Some("migration-evidence"));

        monitor.mark_retry("temporary failure").await;
        monitor.mark_processed().await;
        monitor
            .observe_probe(KafkaRuntimeProbe {
                broker_reachability: MigrationEvidenceBrokerReachability::Reachable,
                discovered_broker_count: Some(3),
                assigned_partitions: vec![0, 1],
                topic_partition_count: Some(2),
                partition_lag: vec![
                    MigrationEvidencePartitionLagStatus {
                        partition: 0,
                        current_offset: Some(10),
                        high_watermark: Some(10),
                        estimated_lag_message_count: Some(0),
                    },
                    MigrationEvidencePartitionLagStatus {
                        partition: 1,
                        current_offset: Some(8),
                        high_watermark: Some(8),
                        estimated_lag_message_count: Some(0),
                    },
                ],
                estimated_lag_message_count: Some(0),
                lag_state: MigrationEvidenceEventBusLagState::CaughtUp,
                lag_diagnostics: Some(
                    "consumer is caught up across 2 assigned partition(s)".to_string(),
                ),
            })
            .await;
        let running = monitor.snapshot().await;
        assert_eq!(
            running.consumer_state,
            MigrationEvidenceEventConsumerState::Running
        );
        assert_eq!(running.retry_attempt_count, 1);
        assert_eq!(running.processed_message_count, 1);
        assert!(running.last_successful_ingest_at.is_some());
        assert_eq!(running.lag_state, MigrationEvidenceEventBusLagState::CaughtUp);
        assert_eq!(running.estimated_lag_message_count, Some(0));
        assert_eq!(
            running.broker_reachability,
            MigrationEvidenceBrokerReachability::Reachable
        );
        assert_eq!(running.discovered_broker_count, Some(3));
        assert_eq!(running.assigned_partitions, vec![0, 1]);
        assert_eq!(running.topic_partition_count, Some(2));
        assert_eq!(running.partition_lag.len(), 2);
        assert_eq!(
            running.lag_diagnostics.as_deref(),
            Some("consumer is caught up across 2 assigned partition(s)")
        );

        monitor.mark_poison_message("bad payload").await;
        let poison = monitor.snapshot().await;
        assert_eq!(poison.malformed_message_count, 1);
        assert_eq!(poison.last_error.as_deref(), Some("bad payload"));
    }

    #[tokio::test]
    async fn runtime_monitor_tracks_startup_failures() {
        let monitor = EventBusRuntimeMonitor::kafka(&KafkaTraceabilityConsumerConfig {
            bootstrap_servers: "kafka:9092".to_string(),
            topic: "migration-evidence".to_string(),
            consumer_group: "arcxa-traceability".to_string(),
            client_id: Some("arcxa-test".to_string()),
            retry_delay: Duration::from_secs(1),
        });

        monitor.mark_startup_failed("bootstrap configuration rejected").await;
        let status = monitor.snapshot().await;

        assert_eq!(
            status.consumer_state,
            MigrationEvidenceEventConsumerState::Stopped
        );
        assert_eq!(
            status.startup_failure_reason.as_deref(),
            Some("bootstrap configuration rejected")
        );
        assert!(status.last_state_changed_at.is_some());
    }

    #[tokio::test]
    async fn runtime_monitor_tracks_probe_failures() {
        let monitor = EventBusRuntimeMonitor::kafka(&KafkaTraceabilityConsumerConfig {
            bootstrap_servers: "kafka:9092".to_string(),
            topic: "migration-evidence".to_string(),
            consumer_group: "arcxa-traceability".to_string(),
            client_id: Some("arcxa-test".to_string()),
            retry_delay: Duration::from_secs(1),
        });

        monitor
            .mark_broker_probe_failed("metadata fetch failed")
            .await;
        monitor
            .mark_assignment_probe_failed("assignment refresh failed")
            .await;
        let status = monitor.snapshot().await;

        assert_eq!(
            status.broker_reachability,
            MigrationEvidenceBrokerReachability::Degraded
        );
        assert_eq!(status.last_error.as_deref(), Some("assignment refresh failed"));
        assert!(status.last_broker_probe_at.is_some());
        assert!(status.last_assignment_at.is_some());
    }
}
