use arcxa_verification::{VerificationManager, VerificationServiceImpl};
use clap::Parser;
use graphica_core::migration_evidence::{
    GrpcMigrationEvidenceEventForwarder, KafkaMigrationEvidenceEventForwarder,
    MigrationEvidenceEventForwarder,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;
use graphica_core::distributed::proto::migration_evidence::verification_service_server::VerificationServiceServer;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:50073")]
    listen_addr: SocketAddr,
    #[arg(long, env = "MIGRATION_EVIDENCE_EVENT_BUS_MODE", default_value = "direct")]
    event_delivery_mode: String,
    #[arg(long, default_value = "http://127.0.0.1:50072")]
    traceability_endpoint: String,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_BOOTSTRAP_SERVERS")]
    kafka_bootstrap_servers: Option<String>,
    #[arg(long, env = "MIGRATION_EVIDENCE_KAFKA_TOPIC", default_value = "arcxa.migration-evidence.events.v1")]
    kafka_topic: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = Args::parse();
    let event_forwarder = if args.event_delivery_mode.eq_ignore_ascii_case("kafka") {
        let bootstrap_servers = args
            .kafka_bootstrap_servers
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Kafka event delivery mode requires --kafka-bootstrap-servers"))?;
        Arc::new(KafkaMigrationEvidenceEventForwarder::new(bootstrap_servers, args.kafka_topic)?)
            as Arc<dyn MigrationEvidenceEventForwarder>
    } else {
        Arc::new(GrpcMigrationEvidenceEventForwarder::new(args.traceability_endpoint))
            as Arc<dyn MigrationEvidenceEventForwarder>
    };
    info!("starting arcxa-verification on {}", args.listen_addr);
    Server::builder()
        .add_service(VerificationServiceServer::new(VerificationServiceImpl::new(
            VerificationManager::new(event_forwarder),
        )))
        .serve(args.listen_addr)
        .await?;
    Ok(())
}
