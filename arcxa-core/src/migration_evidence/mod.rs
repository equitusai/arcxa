mod auth;
mod delivery;
mod ecc_adapter;
mod ecc_rfc_bapi;
mod ecc_staged_export;
mod idoc_extractor;
mod odata;

pub use auth::{resolve_connector_auth, ConnectorAuthResolutionMetadata, ResolvedConnectorAuth};
pub use delivery::{
    GrpcMigrationEvidenceEventForwarder, KafkaMigrationEvidenceEventForwarder,
    MigrationEvidenceEventForwarder,
};
pub use ecc_adapter::{
    derive_sap_ecc_projection_fields, discover_sap_ecc_adapter_capabilities,
    extract_sap_ecc_adapter_next_path, field_types_by_name, merge_sap_ecc_adapter_page_payloads,
    normalize_sap_ecc_adapter_payload, resolve_sap_ecc_adapter_value, SapEccAdapterCapabilities,
    SapEccAdapterField, SapEccProjectionFields,
};
pub use ecc_rfc_bapi::{
    derive_sap_ecc_rfc_bapi_projection_fields, discover_sap_ecc_rfc_bapi_capabilities,
    extract_sap_ecc_rfc_bapi_next_cursor, extract_sap_ecc_rfc_bapi_next_cursor_from_path,
    merge_sap_ecc_rfc_bapi_page_payloads, normalize_sap_ecc_rfc_bapi_payload,
    resolve_sap_ecc_rfc_bapi_value, rfc_field_types_by_name, SapEccRfcBapiCapabilities,
    SapEccRfcBapiField, SapEccRfcBapiProfile, SapEccRfcBapiProjectionFields,
};
pub use ecc_staged_export::{
    SapEccStagedApprovalEvidence, SapEccStagedControlEvidence, SapEccStagedExceptionEvidence,
    SapEccStagedExecutionEvidence, SapEccStagedExportBundle, SapEccStagedExportDataFormat,
    SapEccStagedExportDataSet, SapEccStagedExportManifest, SapEccStagedRuleEvidence,
};
pub use idoc_extractor::{
    SapExtractorFamily, SapExtractorMode, SapIdocExtractorApprovalEvidence, SapIdocExtractorBundle,
    SapIdocExtractorControlEvidence, SapIdocExtractorDataFormat, SapIdocExtractorDataSet,
    SapIdocExtractorExceptionEvidence, SapIdocExtractorExecutionEvidence, SapIdocExtractorManifest,
};
pub use odata::{
    derive_sap_s4_odata_projection_fields, discover_sap_s4_odata_capabilities,
    extract_json_path_value, extract_sap_s4_odata_next_link, infer_sap_s4_odata_metadata_path,
    infer_sap_s4_odata_service_root_path, merge_sap_s4_odata_page_payloads,
    normalize_sap_s4_odata_payload, resolve_sap_s4_odata_value, SapS4ODataCapabilities,
    SapS4ODataProjectionFields, SapS4ODataProperty, SapS4ODataVersion,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationConnectorVendor {
    IbmRapidMove,
    SnpCrystalBridge,
    SmartShift,
    SapHana,
    SapEcc,
    SapS4,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationConnectorRole {
    MigrationArtifactSource,
    VerificationSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorTransport {
    HttpJson,
    SapHanaSql,
    SapEccAdapter,
    SapEccRfcBapi,
    SapEccStagedExport,
    SapIdocExtractorPackage,
    SapOdpExtractorPackage,
    SapS4OData,
    ManualDrop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAuthKind {
    None,
    Bearer,
    ApiKey,
    Basic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapEccBackendAuthMode {
    UserPassword,
    Snc,
    Sso2,
    X509,
    Destination,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapEccSessionMode {
    Stateless,
    Stateful,
    Cached,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectorAuth {
    pub kind: ConnectorAuthKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl Default for ConnectorAuth {
    fn default() -> Self {
        Self {
            kind: ConnectorAuthKind::None,
            secret_ref: None,
            token: None,
            api_key: None,
            header_name: None,
            username: None,
            password: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectorEndpoint {
    pub base_url: String,
    pub path: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_http_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationConnector {
    pub connector_id: String,
    pub name: String,
    pub vendor: MigrationConnectorVendor,
    pub role: MigrationConnectorRole,
    pub transport: ConnectorTransport,
    pub program_id: String,
    pub endpoint: ConnectorEndpoint,
    #[serde(default)]
    pub auth: ConnectorAuth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationProgram {
    pub program_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_landscape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_landscape: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationObjectType {
    Table,
    BusinessObject,
    ApiEntity,
    Interface,
    CustomCodeArtifact,
    Record,
    FieldGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationObject {
    pub object_id: String,
    pub program_id: String,
    pub object_type: MigrationObjectType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_record_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceFieldRef {
    pub system: String,
    pub object_name: String,
    pub field_name: String,
    pub field_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TargetFieldRef {
    pub system: String,
    pub object_name: String,
    pub field_name: String,
    pub field_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransformationRuleType {
    Mapping,
    Conversion,
    Harmonization,
    Filter,
    DefaultValue,
    Aggregation,
    Enrichment,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransformationRule {
    pub rule_id: String,
    pub rule_type: TransformationRuleType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub source_fields: Vec<SourceFieldRef>,
    #[serde(default)]
    pub target_fields: Vec<TargetFieldRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Succeeded,
    Failed,
    Partial,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionEvent {
    pub execution_id: String,
    pub program_id: String,
    pub object_id: String,
    pub connector_run_id: String,
    pub tool_name: String,
    pub tool_run_id: String,
    pub stage: String,
    pub status: ExecutionStatus,
    pub happened_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_snapshot_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_snapshot_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records_examined: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records_affected: Option<u64>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionStatus {
    Open,
    Overridden,
    Remediated,
    Accepted,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExceptionRecord {
    pub exception_id: String,
    pub program_id: String,
    pub object_id: String,
    pub severity: ExceptionSeverity,
    pub status: ExceptionStatus,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    pub detected_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Passed,
    Failed,
    Warning,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ControlResult {
    pub control_id: String,
    pub program_id: String,
    pub object_id: String,
    pub control_name: String,
    pub control_type: String,
    pub status: ControlStatus,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    pub executed_at: DateTime<Utc>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Waived,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApprovalEvent {
    pub approval_id: String,
    pub program_id: String,
    pub object_id: String,
    pub approver_role: String,
    pub approver_id: String,
    pub status: ApprovalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub approved_at: DateTime<Utc>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation_ref: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EvidencePacketSignature {
    pub algorithm: String,
    pub payload_hash_algorithm: String,
    pub payload_hash: String,
    pub public_key: String,
    pub key_fingerprint: String,
    pub signature: String,
    pub signed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EvidencePacket {
    pub packet_id: String,
    pub program_id: String,
    pub object_id: String,
    pub value_key: String,
    pub generated_at: DateTime<Utc>,
    pub source_field: SourceFieldRef,
    pub target_field: TargetFieldRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformation_rule: Option<TransformationRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_event: Option<ExecutionEvent>,
    #[serde(default)]
    pub exceptions: Vec<ExceptionRecord>,
    #[serde(default)]
    pub controls: Vec<ControlResult>,
    #[serde(default)]
    pub approvals: Vec<ApprovalEvent>,
    #[serde(default)]
    pub graph_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<EvidencePacketSignature>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValueLocator {
    pub program_id: String,
    pub object_id: String,
    pub target_field_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValueExplanation {
    pub explanation_id: String,
    pub locator: ValueLocator,
    pub source_field: SourceFieldRef,
    pub target_field: TargetFieldRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformation_rule: Option<TransformationRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_event: Option<ExecutionEvent>,
    #[serde(default)]
    pub exceptions: Vec<ExceptionRecord>,
    #[serde(default)]
    pub controls: Vec<ControlResult>,
    #[serde(default)]
    pub approvals: Vec<ApprovalEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_packet_id: Option<String>,
    #[serde(default)]
    pub graph_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_summary: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceArtifactType {
    Program,
    Object,
    TransformationRule,
    ExecutionEvent,
    ExceptionRecord,
    ControlResult,
    ApprovalEvent,
    EvidencePacket,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceEvent {
    pub event_id: String,
    pub connector_id: String,
    pub run_id: String,
    pub vendor: MigrationConnectorVendor,
    pub program_id: String,
    pub object_id: String,
    pub artifact_type: MigrationEvidenceArtifactType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationRequest {
    pub control_name: String,
    pub program_id: String,
    pub object_id: String,
    pub source_field: SourceFieldRef,
    pub target_field: TargetFieldRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub source: VerificationSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationSource {
    pub transport: ConnectorTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<ConnectorEndpoint>,
    #[serde(default)]
    pub auth: ConnectorAuth,
    #[serde(default)]
    pub connection: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationResult {
    pub execution_event: ExecutionEvent,
    pub control_result: ControlResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception_record: Option<ExceptionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationDispatchRequest {
    pub connector_id: String,
    pub run_id: String,
    pub vendor: MigrationConnectorVendor,
    pub verification: VerificationRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationDispatchResult {
    pub verification_result: VerificationResult,
    #[serde(default)]
    pub emitted_events: Vec<MigrationEvidenceEvent>,
    pub dispatch_summary: MigrationEvidenceDispatchSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectorRunRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_label: Option<String>,
    #[serde(default)]
    pub manual_events: Vec<MigrationEvidenceEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<Value>,
    #[serde(default)]
    pub request_headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectorRunSummary {
    pub run_id: String,
    pub connector_id: String,
    pub ingested_event_count: usize,
    #[serde(default)]
    pub delivery_mode: MigrationEvidenceDeliveryMode,
    #[serde(default = "default_traceability_acknowledged")]
    pub traceability_acknowledged: bool,
    #[serde(default)]
    pub touched_program_ids: Vec<String>,
    #[serde(default)]
    pub touched_object_ids: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct MigrationEvidenceDispatchSummary {
    pub accepted_event_count: usize,
    #[serde(default)]
    pub touched_program_ids: Vec<String>,
    #[serde(default)]
    pub touched_object_ids: Vec<String>,
    pub delivery_mode: MigrationEvidenceDeliveryMode,
    pub traceability_acknowledged: bool,
}

impl MigrationEvidenceDispatchSummary {
    pub fn from_events(
        events: &[MigrationEvidenceEvent],
        delivery_mode: MigrationEvidenceDeliveryMode,
        traceability_acknowledged: bool,
    ) -> Self {
        let mut touched_program_ids = events
            .iter()
            .map(|event| event.program_id.clone())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        touched_program_ids.sort();
        touched_program_ids.dedup();

        let mut touched_object_ids = events
            .iter()
            .map(|event| event.object_id.clone())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        touched_object_ids.sort();
        touched_object_ids.dedup();

        Self {
            accepted_event_count: events.len(),
            touched_program_ids,
            touched_object_ids,
            delivery_mode,
            traceability_acknowledged,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceDeliveryMode {
    #[default]
    Direct,
    Kafka,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceEventConsumerState {
    #[default]
    Disabled,
    Running,
    Recovering,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceEventBusMode {
    #[default]
    Direct,
    Kafka,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceEventBusLagState {
    #[default]
    Unknown,
    CaughtUp,
    Backlog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEvidenceBrokerReachability {
    #[default]
    Unknown,
    Reachable,
    Degraded,
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStoreBackend {
    #[default]
    Unknown,
    File,
    RocksDb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStoreHealth {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectorStoreRuntimeStatus {
    pub backend: ConnectorStoreBackend,
    pub health: ConnectorStoreHealth,
    pub connector_count: usize,
    pub writable: bool,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_successful_write_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_imported_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EvidenceIngestionRuntimeStatus {
    pub connector_store: ConnectorStoreRuntimeStatus,
    pub delivery_mode: MigrationEvidenceDeliveryMode,
    pub verification_service_configured: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraceabilityStoreBackend {
    File,
    RocksDb,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct TraceabilityReadModelCounts {
    pub programs: usize,
    pub objects: usize,
    pub rules: usize,
    pub executions: usize,
    pub exceptions: usize,
    pub controls: usize,
    pub approvals: usize,
    pub packets: usize,
    pub object_indexes: usize,
    pub program_object_links: usize,
    pub event_log_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraceabilityRuntimeStatus {
    pub backend: TraceabilityStoreBackend,
    pub replay_supported: bool,
    pub event_log_available: bool,
    pub read_models: TraceabilityReadModelCounts,
    #[serde(default)]
    pub event_bus: MigrationEvidenceEventBusStatus,
    pub last_event_sequence: u64,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rebuild_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_imported_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraceabilityRebuildSummary {
    pub backend: TraceabilityStoreBackend,
    pub replayed_event_count: usize,
    pub touched_program_count: usize,
    pub touched_object_count: usize,
    pub rebuilt_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceEventBusStatus {
    pub mode: MigrationEvidenceEventBusMode,
    pub async_delivery_enabled: bool,
    pub consumer_state: MigrationEvidenceEventConsumerState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_servers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_group: Option<String>,
    pub processed_message_count: u64,
    pub malformed_message_count: u64,
    pub retry_attempt_count: u64,
    pub lag_state: MigrationEvidenceEventBusLagState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_lag_message_count: Option<u64>,
    pub broker_reachability: MigrationEvidenceBrokerReachability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_broker_count: Option<u32>,
    #[serde(default)]
    pub assigned_partitions: Vec<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_partition_count: Option<u32>,
    #[serde(default)]
    pub partition_lag: Vec<MigrationEvidencePartitionLagStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_consumed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_successful_ingest_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_retry_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag_observed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_state_changed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_assignment_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_broker_probe_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag_diagnostics: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct MigrationEvidencePartitionLagStatus {
    pub partition: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_watermark: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_lag_message_count: Option<u64>,
}

impl Default for MigrationEvidenceEventBusStatus {
    fn default() -> Self {
        Self {
            mode: MigrationEvidenceEventBusMode::Direct,
            async_delivery_enabled: false,
            consumer_state: MigrationEvidenceEventConsumerState::Disabled,
            bootstrap_servers: None,
            topic: None,
            consumer_group: None,
            processed_message_count: 0,
            malformed_message_count: 0,
            retry_attempt_count: 0,
            lag_state: MigrationEvidenceEventBusLagState::Unknown,
            estimated_lag_message_count: None,
            broker_reachability: MigrationEvidenceBrokerReachability::Unknown,
            discovered_broker_count: None,
            assigned_partitions: Vec::new(),
            topic_partition_count: None,
            partition_lag: Vec::new(),
            last_consumed_at: None,
            last_successful_ingest_at: None,
            last_retry_at: None,
            lag_observed_at: None,
            last_state_changed_at: None,
            startup_completed_at: None,
            startup_failure_reason: None,
            last_assignment_at: None,
            last_broker_probe_at: None,
            lag_diagnostics: None,
            last_error: None,
        }
    }
}

pub const MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationEvidenceEventEnvelope {
    pub schema_version: u32,
    pub published_at: DateTime<Utc>,
    pub connector_id: String,
    pub run_id: String,
    pub program_id: String,
    pub object_id: String,
    pub event: MigrationEvidenceEvent,
}

impl MigrationEvidenceEventEnvelope {
    pub fn from_event(event: MigrationEvidenceEvent) -> Self {
        Self {
            schema_version: MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION,
            published_at: Utc::now(),
            connector_id: event.connector_id.clone(),
            run_id: event.run_id.clone(),
            program_id: event.program_id.clone(),
            object_id: event.object_id.clone(),
            event,
        }
    }

    pub fn partition_key(&self) -> String {
        format!("{}::{}", self.program_id, self.object_id)
    }
}

fn default_traceability_acknowledged() -> bool {
    true
}

impl MigrationEvidenceEvent {
    pub fn new(
        connector_id: impl Into<String>,
        run_id: impl Into<String>,
        vendor: MigrationConnectorVendor,
        program_id: impl Into<String>,
        object_id: impl Into<String>,
        artifact_type: MigrationEvidenceArtifactType,
        value_key: Option<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            connector_id: connector_id.into(),
            run_id: run_id.into(),
            vendor,
            program_id: program_id.into(),
            object_id: object_id.into(),
            artifact_type,
            value_key,
            captured_at: Utc::now(),
            payload,
        }
    }
}

pub fn verification_result_to_events(
    connector_id: impl Into<String>,
    run_id: impl Into<String>,
    vendor: MigrationConnectorVendor,
    result: VerificationResult,
) -> Result<Vec<MigrationEvidenceEvent>, serde_json::Error> {
    let connector_id = connector_id.into();
    let run_id = run_id.into();
    let object_id = result.control_result.object_id.clone();
    let program_id = result.control_result.program_id.clone();
    let value_key = result
        .control_result
        .metadata
        .get("value_key")
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{}::{}",
                result.control_result.object_id, result.control_result.control_name
            )
        });

    let mut events = vec![
        MigrationEvidenceEvent::new(
            connector_id.clone(),
            run_id.clone(),
            vendor.clone(),
            program_id.clone(),
            object_id.clone(),
            MigrationEvidenceArtifactType::ExecutionEvent,
            Some(value_key.clone()),
            serde_json::to_value(&result.execution_event)?,
        ),
        MigrationEvidenceEvent::new(
            connector_id.clone(),
            run_id.clone(),
            vendor.clone(),
            program_id.clone(),
            object_id.clone(),
            MigrationEvidenceArtifactType::ControlResult,
            Some(value_key.clone()),
            serde_json::to_value(&result.control_result)?,
        ),
    ];

    if let Some(exception) = result.exception_record {
        events.push(MigrationEvidenceEvent::new(
            connector_id,
            run_id,
            vendor,
            program_id,
            object_id,
            MigrationEvidenceArtifactType::ExceptionRecord,
            Some(value_key),
            serde_json::to_value(&exception)?,
        ));
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_evidence_event_serializes_round_trip() {
        let event = MigrationEvidenceEvent::new(
            "connector-1",
            "run-1",
            MigrationConnectorVendor::IbmRapidMove,
            "program-1",
            "object-1",
            MigrationEvidenceArtifactType::TransformationRule,
            Some("sales_order.total".to_string()),
            serde_json::json!({"rule_id": "rule-1"}),
        );

        let serialized = serde_json::to_string(&event).expect("event should serialize");
        let restored: MigrationEvidenceEvent =
            serde_json::from_str(&serialized).expect("event should deserialize");

        assert_eq!(restored.connector_id, "connector-1");
        assert_eq!(restored.run_id, "run-1");
        assert_eq!(
            restored.artifact_type,
            MigrationEvidenceArtifactType::TransformationRule
        );
        assert_eq!(restored.value_key.as_deref(), Some("sales_order.total"));
    }

    #[test]
    fn event_envelope_captures_partition_key_and_schema_version() {
        let event = MigrationEvidenceEvent::new(
            "connector-1",
            "run-1",
            MigrationConnectorVendor::SapHana,
            "program-1",
            "object-1",
            MigrationEvidenceArtifactType::ControlResult,
            None,
            serde_json::json!({"control_id": "control-1"}),
        );

        let envelope = MigrationEvidenceEventEnvelope::from_event(event);

        assert_eq!(
            envelope.schema_version,
            MIGRATION_EVIDENCE_EVENT_SCHEMA_VERSION
        );
        assert_eq!(envelope.partition_key(), "program-1::object-1");
        assert_eq!(envelope.connector_id, "connector-1");
    }
}
