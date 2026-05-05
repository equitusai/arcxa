mod service;
mod store;

pub use graphica_core::migration_evidence::{
    GrpcMigrationEvidenceEventForwarder as GrpcTraceabilityForwarder,
    KafkaMigrationEvidenceEventForwarder as KafkaTraceabilityForwarder,
    MigrationEvidenceDispatchSummary as EventDispatchSummary,
    MigrationEvidenceEventForwarder as TraceabilityForwarder,
};
pub use service::{
    EvidenceIngestionManager, EvidenceIngestionServiceImpl, GrpcVerificationForwarder,
    VerificationProvider,
};
pub use store::PersistedConnectorStore;
