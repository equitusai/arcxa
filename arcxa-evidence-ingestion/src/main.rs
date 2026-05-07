use arcxa_evidence_ingestion::{
    EvidenceIngestionManager, EvidenceIngestionServiceImpl, GrpcTraceabilityForwarder,
    GrpcVerificationForwarder, KafkaTraceabilityForwarder, PersistedConnectorStore,
};
use clap::Parser;
use graphica_core::distributed::proto::migration_evidence::evidence_ingestion_service_server::EvidenceIngestionServiceServer;
use graphica_core::migration_evidence::MigrationEvidenceDeliveryMode;
use graphica_core::secrets::providers::{FileSecretStore, InlineSecretStore, SecretStoreRegistry};
use graphica_core::secrets::SecretStoreRef;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:50071")]
    listen_addr: SocketAddr,
    #[arg(long, default_value = "data/migration-evidence/connectors/state.json")]
    state_path: String,
    #[arg(long, default_value = "data/migration-evidence/connectors/rocksdb")]
    rocksdb_path: String,
    #[arg(
        long,
        env = "MIGRATION_EVIDENCE_EVENT_BUS_MODE",
        default_value = "direct"
    )]
    event_delivery_mode: String,
    #[arg(long, default_value = "http://127.0.0.1:50072")]
    traceability_endpoint: String,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_BOOTSTRAP_SERVERS")]
    kafka_bootstrap_servers: Option<String>,
    #[arg(
        long,
        env = "MIGRATION_EVIDENCE_KAFKA_TOPIC",
        default_value = "arcxa.migration-evidence.events.v1"
    )]
    kafka_topic: String,
    #[arg(long, default_value = "http://127.0.0.1:50073")]
    verification_endpoint: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = Args::parse();
    let store = PersistedConnectorStore::open_rocksdb(
        &args.rocksdb_path,
        Some(args.state_path.clone().into()),
    )
    .await?;
    let traceability = if args.event_delivery_mode.eq_ignore_ascii_case("kafka") {
        let bootstrap_servers = args.kafka_bootstrap_servers.as_deref().ok_or_else(|| {
            anyhow::anyhow!("Kafka event delivery mode requires --kafka-bootstrap-servers")
        })?;
        Arc::new(KafkaTraceabilityForwarder::new(
            bootstrap_servers,
            args.kafka_topic,
        )?) as Arc<dyn arcxa_evidence_ingestion::TraceabilityForwarder>
    } else {
        Arc::new(GrpcTraceabilityForwarder::new(args.traceability_endpoint))
            as Arc<dyn arcxa_evidence_ingestion::TraceabilityForwarder>
    };
    let secret_store_registry = init_secret_store_registry()?;
    let mut manager = EvidenceIngestionManager::new(
        store,
        traceability,
        Arc::new(GrpcVerificationForwarder::new(args.verification_endpoint)),
        if args.event_delivery_mode.eq_ignore_ascii_case("kafka") {
            MigrationEvidenceDeliveryMode::Kafka
        } else {
            MigrationEvidenceDeliveryMode::Direct
        },
    );
    if let Some(registry) = secret_store_registry {
        manager = manager.with_secret_store_registry(registry);
    }

    info!("starting arcxa-evidence-ingestion on {}", args.listen_addr);
    Server::builder()
        .add_service(EvidenceIngestionServiceServer::new(
            EvidenceIngestionServiceImpl::new(manager),
        ))
        .serve(args.listen_addr)
        .await?;
    Ok(())
}

fn init_secret_store_registry() -> anyhow::Result<Option<Arc<SecretStoreRegistry>>> {
    let registry = Arc::new(SecretStoreRegistry::with_cache(300, 1000));
    let store_type = std::env::var("GRAPHICA_SECRET_STORE_TYPE")
        .ok()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| {
            if std::env::var("GRAPHICA_SECRET_STORE_DIR").is_ok() {
                "file".to_string()
            } else {
                "inline".to_string()
            }
        });
    let store: Option<SecretStoreRef> = match store_type.as_str() {
        "file" => {
            let directory = std::env::var("GRAPHICA_SECRET_STORE_DIR")
                .unwrap_or_else(|_| "./data/secrets".to_string());
            let format = std::env::var("GRAPHICA_SECRET_STORE_FORMAT")
                .unwrap_or_else(|_| "json".to_string());
            let store = FileSecretStore::with_directory_and_format(&directory, &format)?;
            std::fs::create_dir_all(store.base_dir())?;
            Some(Arc::new(store))
        }
        "inline" => Some(Arc::new(InlineSecretStore::new())),
        _ => Some(Arc::new(InlineSecretStore::new())),
    };
    if let Some(store) = store {
        registry.register("default", store.clone());
        registry.set_default(store);
        Ok(Some(registry))
    } else {
        Ok(None)
    }
}
