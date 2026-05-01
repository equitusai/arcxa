use super::{
    MigrationEvidenceDeliveryMode, MigrationEvidenceDispatchSummary, MigrationEvidenceEvent,
    MigrationEvidenceEventEnvelope,
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use crate::distributed::proto::migration_evidence::{
    traceability_service_client::TraceabilityServiceClient, IngestEventsRequest,
};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::Duration;

#[async_trait]
pub trait MigrationEvidenceEventForwarder: Send + Sync {
    async fn ingest_events(&self, events: Vec<MigrationEvidenceEvent>) -> Result<MigrationEvidenceDispatchSummary>;
}

#[derive(Clone)]
pub struct GrpcMigrationEvidenceEventForwarder {
    endpoint: String,
}

impl GrpcMigrationEvidenceEventForwarder {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}

#[async_trait]
impl MigrationEvidenceEventForwarder for GrpcMigrationEvidenceEventForwarder {
    async fn ingest_events(&self, events: Vec<MigrationEvidenceEvent>) -> Result<MigrationEvidenceDispatchSummary> {
        let mut client = TraceabilityServiceClient::connect(self.endpoint.clone()).await?;
        let response = client
            .ingest_events(IngestEventsRequest {
                event_json: events
                    .iter()
                    .map(|event| serde_json::to_string(event))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            })
            .await?
            .into_inner();
        Ok(MigrationEvidenceDispatchSummary {
            accepted_event_count: response.ingested_count as usize,
            touched_program_ids: response.program_ids,
            touched_object_ids: response.object_ids,
            delivery_mode: MigrationEvidenceDeliveryMode::Direct,
            traceability_acknowledged: true,
        })
    }
}

#[derive(Clone)]
pub struct KafkaMigrationEvidenceEventForwarder {
    producer: FutureProducer,
    topic: String,
}

impl KafkaMigrationEvidenceEventForwarder {
    pub fn new(bootstrap_servers: impl AsRef<str>, topic: impl Into<String>) -> Result<Self> {
        let bootstrap_servers = bootstrap_servers.as_ref().trim();
        if bootstrap_servers.is_empty() {
            return Err(anyhow!("Kafka bootstrap servers cannot be empty"));
        }

        let producer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("enable.idempotence", "true")
            .set("acks", "all")
            .set("compression.type", "zstd")
            .create()
            .context("failed to create Kafka producer for migration evidence events")?;

        Ok(Self {
            producer,
            topic: topic.into(),
        })
    }
}

#[async_trait]
impl MigrationEvidenceEventForwarder for KafkaMigrationEvidenceEventForwarder {
    async fn ingest_events(&self, events: Vec<MigrationEvidenceEvent>) -> Result<MigrationEvidenceDispatchSummary> {
        for event in &events {
            let envelope = MigrationEvidenceEventEnvelope::from_event(event.clone());
            let key = envelope.partition_key();
            let payload = serde_json::to_string(&envelope)
                .context("failed to serialize migration evidence event envelope")?;

            self.producer
                .send(
                    FutureRecord::to(&self.topic).key(&key).payload(&payload),
                    Duration::from_secs(5),
                )
                .await
                .map_err(|(error, _)| anyhow!("failed to publish migration evidence event to Kafka: {error}"))?;
        }

        Ok(MigrationEvidenceDispatchSummary::from_events(
            &events,
            MigrationEvidenceDeliveryMode::Kafka,
            false,
        ))
    }
}
