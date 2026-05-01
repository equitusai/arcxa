use anyhow::{Context, Result};
use arcxa_evidence_ingestion::{
    EventDispatchSummary, EvidenceIngestionManager, KafkaTraceabilityForwarder, TraceabilityForwarder,
    VerificationProvider,
};
use arcxa_traceability::{
    EventBusRuntimeMonitor, GraphProjectionConfig, KafkaTraceabilityConsumerConfig,
    KafkaTraceabilityEventConsumer, PersistedTraceabilityStore, TraceabilityManager,
};
use arcxa_verification::VerificationManager;
use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use graphica_core::distributed::proto::migration_evidence::{
    evidence_ingestion_service_client::EvidenceIngestionServiceClient, traceability_service_client::TraceabilityServiceClient,
    GetEvidencePacketRequest, GetObjectRequest, GetProgramRequest, RebuildReadModelsRequest,
    RuntimeStatusRequest, RunConnectorRequest as ProtoRunConnectorRequest, UpsertConnectorRequest,
};
use graphica_core::migration_evidence::{
    ApprovalEvent, ConnectorRunRequest, ConnectorRunSummary, ControlResult,
    EvidenceIngestionRuntimeStatus, EvidencePacket, ExceptionRecord, MigrationConnector,
    MigrationEvidenceDeliveryMode, MigrationEvidenceEvent, MigrationEvidenceEventForwarder,
    TraceabilityRebuildSummary, TraceabilityRuntimeStatus, ValueExplanation, ValueLocator,
    VerificationDispatchRequest, VerificationDispatchResult,
};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct MigrationEvidenceEventBusConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub consumer_group: String,
}

#[derive(Debug, Clone)]
pub struct MigrationEvidenceRemoteGatewayConfig {
    pub evidence_ingestion_endpoint: String,
    pub traceability_endpoint: String,
}

#[derive(Debug, Clone)]
pub struct MigrationEvidenceGatewayConfig {
    pub connector_state_path: PathBuf,
    pub connector_rocksdb_path: Option<PathBuf>,
    pub traceability_state_path: PathBuf,
    pub traceability_rocksdb_path: Option<PathBuf>,
    pub signing_key_seed: [u8; 32],
    pub shard_endpoint: Option<String>,
    pub event_bus: Option<MigrationEvidenceEventBusConfig>,
    pub remote_services: Option<MigrationEvidenceRemoteGatewayConfig>,
}

#[derive(Clone)]
pub struct MigrationEvidenceGateway {
    backend: MigrationEvidenceGatewayBackend,
}

#[derive(Clone)]
enum MigrationEvidenceGatewayBackend {
    Embedded {
        ingestion: Arc<EvidenceIngestionManager>,
        traceability: Arc<TraceabilityManager>,
    },
    Remote(RemoteMigrationEvidenceGateway),
}

#[derive(Clone)]
struct RemoteMigrationEvidenceGateway {
    evidence_ingestion_endpoint: String,
    traceability_endpoint: String,
}

impl MigrationEvidenceGateway {
    pub async fn new(config: MigrationEvidenceGatewayConfig) -> Result<Self> {
        if let Some(remote_services) = config.remote_services {
            return Ok(Self::new_remote(remote_services));
        }

        let connector_store = if let Some(rocksdb_path) = &config.connector_rocksdb_path {
            arcxa_evidence_ingestion::PersistedConnectorStore::open_rocksdb(
                rocksdb_path,
                Some(config.connector_state_path.clone()),
            )
            .await?
        } else {
            arcxa_evidence_ingestion::PersistedConnectorStore::open_file(
                &config.connector_state_path,
            )
            .await?
        };
        let traceability_store = if let Some(rocksdb_path) = &config.traceability_rocksdb_path {
            PersistedTraceabilityStore::open_rocksdb(
                rocksdb_path,
                Some(config.traceability_state_path.clone()),
            )
            .await?
        } else {
            PersistedTraceabilityStore::open_file(&config.traceability_state_path).await?
        };
        let event_delivery_mode = if config.event_bus.is_some() {
            MigrationEvidenceDeliveryMode::Kafka
        } else {
            MigrationEvidenceDeliveryMode::Direct
        };
        let event_bus_runtime = if let Some(event_bus) = &config.event_bus {
            EventBusRuntimeMonitor::kafka(&KafkaTraceabilityConsumerConfig {
                bootstrap_servers: event_bus.bootstrap_servers.clone(),
                topic: event_bus.topic.clone(),
                consumer_group: event_bus.consumer_group.clone(),
                client_id: Some("arcxa-coordinator-traceability".to_string()),
                retry_delay: Duration::from_secs(1),
            })
        } else {
            EventBusRuntimeMonitor::direct()
        };
        let traceability = Arc::new(TraceabilityManager::new(
            traceability_store,
            SigningKey::from_bytes(&config.signing_key_seed),
            GraphProjectionConfig {
                shard_endpoint: config.shard_endpoint,
            },
            event_bus_runtime.clone(),
        ));
        let forwarder = if let Some(event_bus) = config.event_bus {
            match KafkaTraceabilityEventConsumer::new(
                KafkaTraceabilityConsumerConfig {
                    bootstrap_servers: event_bus.bootstrap_servers.clone(),
                    topic: event_bus.topic.clone(),
                    consumer_group: event_bus.consumer_group.clone(),
                    client_id: Some("arcxa-coordinator-traceability".to_string()),
                    retry_delay: Duration::from_secs(1),
                },
                event_bus_runtime.clone(),
            ) {
                Ok(consumer) => {
                    consumer.spawn((*traceability).clone());
                }
                Err(error) => {
                    event_bus_runtime
                        .mark_startup_failed(error.to_string())
                        .await;
                    warn!(
                        error = %error,
                        "migration evidence Kafka consumer failed to initialize inside coordinator; runtime status will reflect degraded async delivery"
                    );
                }
            }
            GatewayTraceabilityForwarder::Kafka(KafkaTraceabilityForwarder::new(
                event_bus.bootstrap_servers,
                event_bus.topic,
            )?)
        } else {
            GatewayTraceabilityForwarder::Local(LocalTraceabilityForwarder {
                traceability: traceability.clone(),
            })
        };
        let verification_event_forwarder: Arc<dyn MigrationEvidenceEventForwarder> =
            Arc::new(forwarder.clone());
        let verification = Arc::new(VerificationManager::new(verification_event_forwarder));
        let ingestion = Arc::new(EvidenceIngestionManager::new(
            connector_store,
            Arc::new(forwarder),
            Arc::new(LocalVerificationForwarder { verification }),
            event_delivery_mode,
        ));
        Ok(Self {
            backend: MigrationEvidenceGatewayBackend::Embedded {
                ingestion,
                traceability,
            },
        })
    }

    pub fn new_remote(config: MigrationEvidenceRemoteGatewayConfig) -> Self {
        Self {
            backend: MigrationEvidenceGatewayBackend::Remote(RemoteMigrationEvidenceGateway {
                evidence_ingestion_endpoint: config.evidence_ingestion_endpoint,
                traceability_endpoint: config.traceability_endpoint,
            }),
        }
    }

    pub async fn upsert_connector(&self, connector: MigrationConnector) -> Result<MigrationConnector> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { ingestion, .. } => {
                ingestion.upsert_connector(connector).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => remote.upsert_connector(connector).await,
        }
    }

    pub async fn run_connector(
        &self,
        connector_id: &str,
        request: ConnectorRunRequest,
    ) -> Result<(ConnectorRunSummary, Vec<MigrationEvidenceEvent>)> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { ingestion, .. } => {
                ingestion.run_connector(connector_id, request).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => {
                remote.run_connector(connector_id, request).await
            }
        }
    }

    pub async fn explain_value(&self, locator: ValueLocator) -> Result<ValueExplanation> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.explain_value(locator).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => remote.explain_value(locator).await,
        }
    }

    pub async fn evidence_packet_for_object(
        &self,
        object_id: &str,
        value_key: Option<&str>,
    ) -> Result<EvidencePacket> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.evidence_packet_for_object(object_id, value_key).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => {
                remote.evidence_packet_for_object(object_id, value_key).await
            }
        }
    }

    pub async fn controls_for_object(&self, object_id: &str) -> Result<Vec<ControlResult>> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.controls_for_object(object_id).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => remote.controls_for_object(object_id).await,
        }
    }

    pub async fn exceptions_for_program(&self, program_id: &str) -> Result<Vec<ExceptionRecord>> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.exceptions_for_program(program_id).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => {
                remote.exceptions_for_program(program_id).await
            }
        }
    }

    pub async fn approvals_for_program(&self, program_id: &str) -> Result<Vec<ApprovalEvent>> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.approvals_for_program(program_id).await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => {
                remote.approvals_for_program(program_id).await
            }
        }
    }

    pub async fn runtime_status(&self) -> Result<TraceabilityRuntimeStatus> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.runtime_status().await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => remote.runtime_status().await,
        }
    }

    pub async fn rebuild_read_models(&self) -> Result<TraceabilityRebuildSummary> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { traceability, .. } => {
                traceability.rebuild_read_models().await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => remote.rebuild_read_models().await,
        }
    }

    pub async fn ingestion_runtime_status(&self) -> Result<EvidenceIngestionRuntimeStatus> {
        match &self.backend {
            MigrationEvidenceGatewayBackend::Embedded { ingestion, .. } => {
                ingestion.runtime_status().await
            }
            MigrationEvidenceGatewayBackend::Remote(remote) => {
                remote.ingestion_runtime_status().await
            }
        }
    }
}

impl RemoteMigrationEvidenceGateway {
    async fn upsert_connector(&self, connector: MigrationConnector) -> Result<MigrationConnector> {
        let mut client = self.evidence_ingestion_client().await?;
        let response = client
            .upsert_connector(UpsertConnectorRequest {
                connector_json: serialize(&connector)?,
            })
            .await?
            .into_inner();
        deserialize(&response.connector_json)
    }

    async fn run_connector(
        &self,
        connector_id: &str,
        request: ConnectorRunRequest,
    ) -> Result<(ConnectorRunSummary, Vec<MigrationEvidenceEvent>)> {
        let mut client = self.evidence_ingestion_client().await?;
        let response = client
            .run_connector(ProtoRunConnectorRequest {
                connector_id: connector_id.to_string(),
                run_request_json: serialize(&request)?,
            })
            .await?
            .into_inner();
        let summary = deserialize(&response.run_summary_json)?;
        let events = response
            .event_json
            .iter()
            .map(|event| deserialize(event))
            .collect::<Result<Vec<_>>>()?;
        Ok((summary, events))
    }

    async fn explain_value(&self, locator: ValueLocator) -> Result<ValueExplanation> {
        let mut client = self.traceability_client().await?;
        let response = client
            .explain_value(graphica_core::distributed::proto::migration_evidence::ExplainValueRequest {
                program_id: locator.program_id,
                object_id: locator.object_id,
                target_field_path: locator.target_field_path,
                target_record_id: locator.target_record_id.unwrap_or_default(),
                source_record_id: locator.source_record_id.unwrap_or_default(),
            })
            .await?
            .into_inner();
        deserialize(&response.value_explanation_json)
    }

    async fn evidence_packet_for_object(&self, object_id: &str, value_key: Option<&str>) -> Result<EvidencePacket> {
        let mut client = self.traceability_client().await?;
        let response = client
            .get_evidence_packet(GetEvidencePacketRequest {
                object_id: object_id.to_string(),
                value_key: value_key.unwrap_or_default().to_string(),
            })
            .await?
            .into_inner();
        deserialize(&response.evidence_packet_json)
    }

    async fn controls_for_object(&self, object_id: &str) -> Result<Vec<ControlResult>> {
        let mut client = self.traceability_client().await?;
        let response = client
            .get_controls(GetObjectRequest {
                object_id: object_id.to_string(),
            })
            .await?
            .into_inner();
        response
            .control_json
            .iter()
            .map(|item| deserialize(item))
            .collect()
    }

    async fn exceptions_for_program(&self, program_id: &str) -> Result<Vec<ExceptionRecord>> {
        let mut client = self.traceability_client().await?;
        let response = client
            .get_exceptions(GetProgramRequest {
                program_id: program_id.to_string(),
            })
            .await?
            .into_inner();
        response
            .exception_json
            .iter()
            .map(|item| deserialize(item))
            .collect()
    }

    async fn approvals_for_program(&self, program_id: &str) -> Result<Vec<ApprovalEvent>> {
        let mut client = self.traceability_client().await?;
        let response = client
            .get_approvals(GetProgramRequest {
                program_id: program_id.to_string(),
            })
            .await?
            .into_inner();
        response
            .approval_json
            .iter()
            .map(|item| deserialize(item))
            .collect()
    }

    async fn runtime_status(&self) -> Result<TraceabilityRuntimeStatus> {
        let mut client = self.traceability_client().await?;
        let response = client
            .get_runtime_status(RuntimeStatusRequest {})
            .await?
            .into_inner();
        deserialize(&response.runtime_status_json)
    }

    async fn ingestion_runtime_status(&self) -> Result<EvidenceIngestionRuntimeStatus> {
        let mut client = self.evidence_ingestion_client().await?;
        let response = client
            .get_runtime_status(RuntimeStatusRequest {})
            .await?
            .into_inner();
        deserialize(&response.runtime_status_json)
    }

    async fn rebuild_read_models(&self) -> Result<TraceabilityRebuildSummary> {
        let mut client = self.traceability_client().await?;
        let response = client
            .rebuild_read_models(RebuildReadModelsRequest {})
            .await?
            .into_inner();
        deserialize(&response.rebuild_summary_json)
    }

    async fn evidence_ingestion_client(&self) -> Result<EvidenceIngestionServiceClient<tonic::transport::Channel>> {
        EvidenceIngestionServiceClient::connect(self.evidence_ingestion_endpoint.clone())
            .await
            .with_context(|| {
                format!(
                    "failed to connect to remote migration-evidence ingestion service at {}",
                    self.evidence_ingestion_endpoint
                )
            })
    }

    async fn traceability_client(&self) -> Result<TraceabilityServiceClient<tonic::transport::Channel>> {
        TraceabilityServiceClient::connect(self.traceability_endpoint.clone())
            .await
            .with_context(|| {
                format!(
                    "failed to connect to remote migration-evidence traceability service at {}",
                    self.traceability_endpoint
                )
            })
    }
}

#[derive(Clone)]
struct LocalTraceabilityForwarder {
    traceability: Arc<TraceabilityManager>,
}

#[async_trait]
impl TraceabilityForwarder for LocalTraceabilityForwarder {
    async fn ingest_events(
        &self,
        events: Vec<MigrationEvidenceEvent>,
    ) -> Result<EventDispatchSummary> {
        self.traceability
            .ingest_events(events)
            .await
            .map(|(accepted_event_count, touched_program_ids, touched_object_ids)| {
                EventDispatchSummary {
                    accepted_event_count,
                    touched_program_ids,
                    touched_object_ids,
                    delivery_mode:
                        graphica_core::migration_evidence::MigrationEvidenceDeliveryMode::Direct,
                    traceability_acknowledged: true,
                }
            })
    }
}

#[derive(Clone)]
enum GatewayTraceabilityForwarder {
    Local(LocalTraceabilityForwarder),
    Kafka(KafkaTraceabilityForwarder),
}

#[async_trait]
impl TraceabilityForwarder for GatewayTraceabilityForwarder {
    async fn ingest_events(
        &self,
        events: Vec<MigrationEvidenceEvent>,
    ) -> Result<EventDispatchSummary> {
        match self {
            Self::Local(forwarder) => forwarder.ingest_events(events).await,
            Self::Kafka(forwarder) => forwarder.ingest_events(events).await,
        }
    }
}

#[derive(Clone)]
struct LocalVerificationForwarder {
    verification: Arc<VerificationManager>,
}

#[async_trait]
impl VerificationProvider for LocalVerificationForwarder {
    async fn run_verification_and_emit(
        &self,
        request: VerificationDispatchRequest,
    ) -> Result<VerificationDispatchResult> {
        self.verification.run_verification_and_emit(request).await
    }
}

fn serialize<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("failed to serialize migration evidence gateway payload")
}

fn deserialize<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).context("failed to deserialize migration evidence gateway payload")
}
