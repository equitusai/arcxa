use arcxa_traceability::{
    EventBusRuntimeMonitor, GraphProjectionConfig, KafkaTraceabilityConsumerConfig,
    KafkaTraceabilityEventConsumer, PersistedTraceabilityStore, TraceabilityManager,
    TraceabilityServiceImpl,
};
use clap::Parser;
use ed25519_dalek::SigningKey;
use std::net::SocketAddr;
use std::time::Duration;
use tonic::transport::Server;
use tracing::{info, warn};
use graphica_core::distributed::proto::migration_evidence::traceability_service_server::TraceabilityServiceServer;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:50072")]
    listen_addr: SocketAddr,
    #[arg(long, default_value = "data/migration-evidence/traceability/rocksdb")]
    rocksdb_path: String,
    #[arg(long, default_value = "data/migration-evidence/traceability/state.json")]
    legacy_state_path: String,
    #[arg(long, env = "ARCXA_TRACEABILITY_SIGNING_KEY_HEX")]
    signing_key_hex: Option<String>,
    #[arg(long, env = "ARCXA_MIGRATION_EVIDENCE_SHARD_ENDPOINT")]
    shard_endpoint: Option<String>,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_BOOTSTRAP_SERVERS")]
    kafka_bootstrap_servers: Option<String>,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_TOPIC", default_value = "arcxa.migration-evidence.events.v1")]
    kafka_topic: String,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_CONSUMER_GROUP", default_value = "arcxa-traceability")]
    kafka_consumer_group: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = Args::parse();
    let signing_key = signing_key_from_hex(args.signing_key_hex.as_deref())?;
    let store = PersistedTraceabilityStore::open_rocksdb(
        &args.rocksdb_path,
        Some(args.legacy_state_path.clone().into()),
    )
    .await?;
    let event_bus_runtime = if let Some(bootstrap_servers) = &args.kafka_bootstrap_servers {
        EventBusRuntimeMonitor::kafka(&KafkaTraceabilityConsumerConfig {
            bootstrap_servers: bootstrap_servers.clone(),
            topic: args.kafka_topic.clone(),
            consumer_group: args.kafka_consumer_group.clone(),
            client_id: Some("arcxa-traceability".to_string()),
            retry_delay: Duration::from_secs(1),
        })
    } else {
        EventBusRuntimeMonitor::direct()
    };
    let manager = TraceabilityManager::new(
        store,
        signing_key,
        GraphProjectionConfig {
            shard_endpoint: args.shard_endpoint,
        },
        event_bus_runtime.clone(),
    );

    if let Some(bootstrap_servers) = args.kafka_bootstrap_servers {
        match KafkaTraceabilityEventConsumer::new(KafkaTraceabilityConsumerConfig {
            bootstrap_servers,
            topic: args.kafka_topic,
            consumer_group: args.kafka_consumer_group,
            client_id: Some("arcxa-traceability".to_string()),
            retry_delay: Duration::from_secs(1),
        }, event_bus_runtime.clone()) {
            Ok(consumer) => {
                consumer.spawn(manager.clone());
            }
            Err(error) => {
                event_bus_runtime
                    .mark_startup_failed(error.to_string())
                    .await;
                warn!(
                    error = %error,
                    "migration evidence Kafka consumer failed to initialize; traceability service will start in degraded mode"
                );
            }
        }
    }

    info!("starting arcxa-traceability on {}", args.listen_addr);
    Server::builder()
        .add_service(TraceabilityServiceServer::new(TraceabilityServiceImpl::new(manager)))
        .serve(args.listen_addr)
        .await?;
    Ok(())
}

fn signing_key_from_hex(seed_hex: Option<&str>) -> anyhow::Result<SigningKey> {
    let seed_hex = seed_hex.unwrap_or("0101010101010101010101010101010101010101010101010101010101010101");
    let bytes = hex::decode(seed_hex)?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("signing key seed must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}
