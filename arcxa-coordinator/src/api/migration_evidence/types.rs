use graphica_core::migration_evidence::{
    ApprovalEvent, ConnectorRunRequest, ConnectorRunSummary, ControlResult,
    EvidenceIngestionRuntimeStatus, EvidencePacket, ExceptionRecord, MigrationConnector,
    TraceabilityRebuildSummary, TraceabilityRuntimeStatus, ValueExplanation,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpsertMigrationConnectorRequest {
    #[serde(flatten)]
    pub connector: MigrationConnector,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpsertMigrationConnectorResponse {
    pub connector: MigrationConnector,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunMigrationConnectorRequest {
    #[serde(flatten)]
    pub run: ConnectorRunRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunMigrationConnectorResponse {
    pub summary: ConnectorRunSummary,
    #[serde(default)]
    pub ingested_events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ExplainValueQuery {
    pub program_id: String,
    pub object_id: String,
    pub target_field_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct EvidencePacketQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceErrorResponse {
    pub error: String,
    #[serde(default)]
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExplainValueResponse {
    pub explanation: ValueExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EvidencePacketResponse {
    pub packet: EvidencePacket,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ObjectControlsResponse {
    pub controls: Vec<ControlResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramExceptionsResponse {
    pub exceptions: Vec<ExceptionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramApprovalsResponse {
    pub approvals: Vec<ApprovalEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceRuntimeStatusResponse {
    pub status: TraceabilityRuntimeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingestion_status: Option<EvidenceIngestionRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceRebuildResponse {
    pub summary: TraceabilityRebuildSummary,
}
