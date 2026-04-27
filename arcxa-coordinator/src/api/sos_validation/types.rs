//! Systems-of-Systems (SoS) Validation API request/response types
//!
//! This module provides types for validating compatibility and integration
//! between systems in a Systems-of-Systems architecture. It supports:
//! - System registration and interface definition
//! - Data contract management
//! - Cross-system validation (schemas, SLAs, policies)
//! - Compatibility matrix analysis

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

// ============================================================================
// System Management Types
// ============================================================================

/// Request to register a new system in the SoS catalog
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RegisterSystemRequest {
    /// Unique identifier for the system
    pub system_id: String,

    /// Human-readable name
    pub system_name: String,

    /// System type (e.g., "satellite.early_warning", "radar.ground_based")
    pub system_type: String,

    /// Vendor/manufacturer
    pub vendor: String,

    /// Version string
    pub version: String,

    /// Classification level (e.g., "UNCLASSIFIED", "SECRET")
    pub classification: String,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deployment information (JSON object)
    #[serde(default)]
    pub deployment: HashMap<String, serde_json::Value>,

    /// System capabilities (JSON object)
    #[serde(default)]
    pub capabilities: HashMap<String, serde_json::Value>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to update an existing system
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateSystemRequest {
    /// Updated system name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,

    /// Updated version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Updated classification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,

    /// Updated description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Updated deployment info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<HashMap<String, serde_json::Value>>,

    /// Updated capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<HashMap<String, serde_json::Value>>,

    /// Updated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Active status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// Query parameters for listing systems
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListSystemsQuery {
    /// Filter by system type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_type: Option<String>,

    /// Filter by vendor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,

    /// Filter by classification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,

    /// Filter by tags (comma-separated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Filter by active status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Pagination offset
    #[serde(default)]
    pub offset: usize,

    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Response containing system information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemResponse {
    /// System ID
    pub system_id: String,

    /// System name
    pub system_name: String,

    /// System type
    pub system_type: String,

    /// Vendor
    pub vendor: String,

    /// Version
    pub version: String,

    /// Classification
    pub classification: String,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deployment information
    pub deployment: HashMap<String, serde_json::Value>,

    /// Capabilities
    pub capabilities: HashMap<String, serde_json::Value>,

    /// Tags
    pub tags: Vec<String>,

    /// Active status
    pub active: bool,

    /// Created timestamp (ISO 8601)
    pub created_at: String,

    /// Updated timestamp (ISO 8601)
    pub updated_at: String,
}

/// Response containing list of systems
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListSystemsResponse {
    /// List of systems
    pub systems: Vec<SystemResponse>,

    /// Total count (before pagination)
    pub total: usize,

    /// Offset
    pub offset: usize,

    /// Limit
    pub limit: usize,
}

// ============================================================================
// Interface Definition Types
// ============================================================================

/// System interface definition
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SystemInterface {
    /// Interface ID
    pub interface_id: String,

    /// Interface name
    pub interface_name: String,

    /// Direction: "inbound", "outbound", "bidirectional"
    pub direction: String,

    /// Protocol (e.g., "REST", "gRPC", "MQTT")
    pub protocol: String,

    /// Data format (e.g., "JSON", "XML", "Protobuf")
    pub data_format: String,

    /// JSON Schema for data validation
    pub schema: serde_json::Value,

    /// Coordinate system (e.g., "WGS84", "ECI_J2000")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinate_system: Option<String>,

    /// Unit system (e.g., "SI", "Imperial")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_system: Option<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to register a system interface
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RegisterInterfaceRequest {
    /// System ID that owns this interface
    pub system_id: String,

    /// Interface definition
    #[serde(flatten)]
    pub interface: SystemInterface,
}

/// Request to update an interface
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateInterfaceRequest {
    /// Updated interface name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_name: Option<String>,

    /// Updated direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,

    /// Updated schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,

    /// Updated coordinate system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinate_system: Option<String>,

    /// Updated unit system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_system: Option<String>,

    /// Updated metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Response containing interface information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InterfaceResponse {
    /// System ID
    pub system_id: String,

    /// Interface definition
    #[serde(flatten)]
    pub interface: SystemInterface,

    /// Created timestamp
    pub created_at: String,

    /// Updated timestamp
    pub updated_at: String,
}

// ============================================================================
// Data Contract Types
// ============================================================================

/// SLA metric definition
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SlaMetric {
    /// Metric name (e.g., "latency_ms", "reliability_percent")
    pub name: String,

    /// Metric value
    pub value: f64,

    /// Comparison operator (e.g., "<=", ">=", "==")
    pub operator: String,

    /// Unit (e.g., "ms", "percent", "mbps")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Request to create a data contract
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateDataContractRequest {
    /// Contract ID
    pub contract_id: String,

    /// Contract name
    pub contract_name: String,

    /// Provider interface ID
    pub provider_interface_id: String,

    /// Consumer interface ID
    pub consumer_interface_id: String,

    /// SLA metrics
    pub sla_metrics: Vec<SlaMetric>,

    /// Transformation rules (optional, JSON object)
    #[serde(default)]
    pub transformation_rules: HashMap<String, serde_json::Value>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to update a data contract
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateDataContractRequest {
    /// Updated contract name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_name: Option<String>,

    /// Updated SLA metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sla_metrics: Option<Vec<SlaMetric>>,

    /// Updated transformation rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformation_rules: Option<HashMap<String, serde_json::Value>>,

    /// Updated description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Updated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Deprecated compatibility field. Approval must transition through `/approve`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
}

/// Response containing data contract information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataContractResponse {
    /// Contract ID
    pub contract_id: String,

    /// Contract revision
    pub revision: u32,

    /// Contract name
    pub contract_name: String,

    /// Provider interface ID
    pub provider_interface_id: String,

    /// Consumer interface ID
    pub consumer_interface_id: String,

    /// SLA metrics
    pub sla_metrics: Vec<SlaMetric>,

    /// Transformation rules
    pub transformation_rules: HashMap<String, serde_json::Value>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags
    pub tags: Vec<String>,

    /// Approval status
    pub approved: bool,

    /// Signed status
    pub signed: bool,

    /// Explicit lifecycle state
    pub lifecycle_state: String,

    /// Approval workflow status
    pub approval_status: String,

    /// Approval requester
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_requested_by: Option<String>,

    /// Approval request timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_requested_at: Option<String>,

    /// Approval actor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,

    /// Approval timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,

    /// Signing actor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,

    /// Signing timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_at: Option<String>,

    /// Optional cryptographic signature / attestation for this exact contract revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<SosContractSignatureResponse>,

    /// Contract creator
    pub created_by: String,

    /// Last contract updater
    pub updated_by: String,

    /// Rejection actor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_by: Option<String>,

    /// Rejection timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_at: Option<String>,

    /// Rejection reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,

    /// Revision that superseded this one, if this response represents a historical snapshot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_revision: Option<u32>,

    /// Created timestamp
    pub created_at: String,

    /// Updated timestamp
    pub updated_at: String,
}

/// Cryptographic signature / attestation material for one immutable contract revision.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosContractSignatureResponse {
    pub signature_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub contract_revision_ref: String,
    pub payload_hash: String,
    pub payload_hash_algorithm: String,
    pub signature_algorithm: String,
    pub signature: String,
    pub public_key: String,
    pub key_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_version: Option<String>,
    pub signing_key_source: String,
    pub signed_by: String,
    pub signed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub signature_verified: bool,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Query parameters for listing contract signatures / attestations.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListContractSignaturesQuery {
    /// Pagination limit.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Operator-facing contract signature history response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListContractSignaturesResponse {
    pub signatures: Vec<SosContractSignatureResponse>,
    pub total: usize,
    pub limit: usize,
}

/// Current SoS contract signing-key status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosContractSigningKeyStatusResponse {
    pub signing_key_ref: Option<String>,
    pub signing_key_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_version: Option<String>,
    pub public_key: String,
    pub key_fingerprint: String,
    pub supports_rotation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_interval_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_last_rotated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_next_due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_auto_rotate: Option<bool>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to rotate the managed SoS contract signing key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateSosContractSigningKeyRequest {
    /// Optional operator-supplied reason captured in key metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Rotation result for the managed SoS contract signing key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateSosContractSigningKeyResponse {
    pub signing_key_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_signing_key_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_key_fingerprint: Option<String>,
    pub current_signing_key_version: String,
    pub current_key_fingerprint: String,
    pub current_public_key: String,
    pub rotated_by: String,
    pub rotated_at: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Query parameters for looking up a data contract by interface pair.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ContractLookupQuery {
    /// Provider interface ID
    pub provider_interface_id: String,

    /// Consumer interface ID
    pub consumer_interface_id: String,
}

/// Request to open a first-class approval request for a persisted contract revision.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateSosContractApprovalRequest {
    /// Actor requesting contract approval for the current revision.
    pub requested_by: String,

    /// Lifecycle state requested if the revision is approved. Currently `approved`.
    pub lifecycle_state: String,

    /// Optional expiration window for the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,

    /// Optional human-readable request note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Optional structured metadata for operators.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to attach approval evidence to a persisted contract approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct AddSosContractApprovalEvidenceRequest {
    /// Validation report that serves as approval evidence.
    pub report_id: String,

    /// Actor attaching the evidence.
    pub added_by: String,

    /// Optional human-readable evidence note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Optional structured metadata for operators.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to approve a specific contract approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ApproveSosContractApprovalRequest {
    /// Actor approving the request.
    pub approved_by: String,
}

/// Request to reject a specific contract approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RejectSosContractApprovalRequest {
    /// Actor rejecting the request.
    pub rejected_by: String,

    /// Human-readable rejection reason.
    pub reason: String,
}

/// Query parameters for listing contract approval requests.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListContractApprovalRequestsQuery {
    /// Filter by approval request status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Pagination offset.
    #[serde(default)]
    pub offset: usize,

    /// Pagination limit.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Persisted contract approval evidence response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SosContractApprovalEvidenceResponse {
    pub evidence_id: String,
    pub request_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub evidence_type: String,
    pub report_id: String,
    pub added_by: String,
    pub added_at: String,
    pub note: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Persisted contract approval request response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SosContractApprovalRequestResponse {
    pub request_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub approval_type: String,
    pub requested_lifecycle_state: String,
    pub status: String,
    pub note: Option<String>,
    pub requested_by: String,
    pub requested_at: String,
    pub expires_at: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub rejected_by: Option<String>,
    pub rejected_at: Option<String>,
    pub rejection_reason: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub evidence: Vec<SosContractApprovalEvidenceResponse>,
}

/// Paginated contract approval request listing response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ListContractApprovalRequestsResponse {
    pub requests: Vec<SosContractApprovalRequestResponse>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

// ============================================================================
// Validation Types
// ============================================================================

/// Validation request (tagged enum for different validation types)
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "type")]
pub enum ValidateRequest {
    /// Validate interface compatibility
    #[serde(rename = "interface_compatibility")]
    InterfaceCompatibility {
        /// Provider interface ID
        provider_interface_id: String,
        /// Consumer interface ID
        consumer_interface_id: String,
    },

    /// Validate data contract compliance
    #[serde(rename = "contract_compliance")]
    ContractCompliance {
        /// Contract ID to validate
        contract_id: String,
    },

    /// Validate system integration
    #[serde(rename = "system_integration")]
    SystemIntegration {
        /// Source system ID
        source_system_id: String,
        /// Target system ID
        target_system_id: String,
    },

    /// Validate governance policy
    #[serde(rename = "policy_check")]
    PolicyCheck {
        /// SPARQL query for policy validation
        sparql_query: String,
        /// Context (systems/interfaces to check)
        context: HashMap<String, serde_json::Value>,
    },

    /// Validate data payload
    #[serde(rename = "data_validation")]
    DataValidation {
        /// Interface ID to validate against
        interface_id: String,
        /// Data payload to validate
        data: serde_json::Value,
    },
}

/// Validation check result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckResult {
    /// Check name
    pub check_name: String,

    /// Pass/fail status
    pub passed: bool,

    /// Severity: "error", "warning", "info"
    pub severity: String,

    /// Description of the check
    pub description: String,

    /// Details (e.g., expected vs actual)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Validation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationResponse {
    /// Validation ID
    pub validation_id: String,

    /// Overall validation result
    pub passed: bool,

    /// Individual check results
    pub checks: Vec<CheckResult>,

    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,

    /// Validation timestamp
    pub validated_at: String,

    /// Report ID (for detailed report retrieval)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
}

/// Persisted validation report response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationReportResponse {
    pub report_id: String,
    pub validation_id: String,
    pub subject_type: String,
    pub subject_key: String,
    pub validation_type: String,
    pub passed: bool,
    pub confidence: f64,
    pub checks: Vec<CheckResult>,
    pub validated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_report_id: Option<String>,
    pub change_summary: ValidationChangeSummaryResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_step_id: Option<String>,
    #[serde(default)]
    pub ontology_refs: Vec<String>,
    #[serde(default)]
    pub shape_refs: Vec<String>,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub contract_refs: Vec<String>,
    #[serde(default)]
    pub schema_hashes: HashMap<String, String>,
}

/// Summary of how a report changed compared with the previous persisted version.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationChangeSummaryResponse {
    pub resolved_checks: Vec<String>,
    pub new_failures: Vec<String>,
    pub confidence_delta: f64,
    pub schema_or_policy_version_changed: bool,
}

/// Query parameters for report history.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ValidationHistoryQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_type: Option<String>,
    pub subject_key: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Validation history response for a normalized subject.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationHistoryResponse {
    pub subject_type: String,
    pub subject_key: String,
    pub reports: Vec<ValidationReportResponse>,
}

/// Query parameters for validation lineage traversal.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ValidationLineageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_type: Option<String>,
    pub subject_key: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Directed edge between validation reports in the persisted lineage chain.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationLineageEdge {
    pub from_report_id: String,
    pub to_report_id: String,
    pub relationship: String,
}

/// Validation-lineage traversal response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationLineageResponse {
    pub subject_type: String,
    pub subject_key: String,
    pub reports: Vec<ValidationReportResponse>,
    pub edges: Vec<ValidationLineageEdge>,
}

// ============================================================================
// SoS Policy Types
// ============================================================================

fn default_policy_limit() -> usize {
    50
}

fn default_policy_stages() -> Vec<String> {
    vec!["pre_execution".to_string()]
}

fn default_policy_enforcement_level() -> String {
    "mandatory".to_string()
}

fn default_policy_severity() -> String {
    "medium".to_string()
}

fn default_true() -> bool {
    true
}

/// Request to create a persisted SoS policy.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateSosPolicyRequest {
    /// Stable policy identifier.
    pub policy_id: String,

    /// Human-readable policy name.
    pub policy_name: String,

    /// Policy target type: global, interface_pair, contract, system_pair, interface.
    pub target_type: String,

    /// Validation stages this policy applies to:
    /// pre_execution, in_flight, post_execution, contract_approval, contract_signing.
    #[serde(default = "default_policy_stages")]
    pub stages: Vec<String>,

    /// Enforcement level: advisory or mandatory.
    #[serde(default = "default_policy_enforcement_level")]
    pub enforcement_level: String,

    /// Policy severity: critical, high, medium, low, error, warning, or info.
    #[serde(default = "default_policy_severity")]
    pub severity: String,

    /// SPARQL query template for policy evaluation.
    pub sparql_query: String,

    /// Template context merged into runtime evaluation context.
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,

    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional policy creator identity. Defaults to `system`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Optional actor recorded as the initial updater. Defaults to the creator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,

    /// Optional lifecycle / rollout state: draft, dry_run, active, deprecated, or retired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,

    /// Optional policy tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional ontology references used by the policy.
    #[serde(default)]
    pub ontology_refs: Vec<String>,

    /// Optional shape references used by the policy.
    #[serde(default)]
    pub shape_refs: Vec<String>,

    /// Legacy convenience flag for automatic stage participation. `lifecycle_state` is canonical.
    #[serde(default = "default_true")]
    pub active: bool,

    /// Target provider interface ID for interface_pair policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_interface_id: Option<String>,

    /// Target consumer interface ID for interface_pair policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_interface_id: Option<String>,

    /// Target contract ID for contract policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,

    /// Target source system ID for system_pair policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_system_id: Option<String>,

    /// Target target system ID for system_pair policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_id: Option<String>,

    /// Target interface ID for interface policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_id: Option<String>,
}

/// Partial update request for a persisted SoS policy.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateSosPolicyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcement_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparql_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_refs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape_refs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_interface_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_interface_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_system_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_id: Option<String>,
}

/// Request to approve a persisted SoS policy revision for rollout.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ApproveSosPolicyRequest {
    /// Actor approving the current policy revision.
    pub approved_by: String,

    /// Optional approval request ID when using the approval-request workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Optional rollout state to apply immediately after approval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
}

/// Request to reject a persisted SoS policy revision for rollout.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RejectSosPolicyRequest {
    /// Actor rejecting the current policy revision.
    pub rejected_by: String,

    /// Optional approval request ID when using the approval-request workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Human-readable rejection reason.
    pub reason: String,
}

/// Request to open a first-class approval request for a persisted policy revision.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateSosPolicyApprovalRequest {
    /// Actor requesting rollout approval for the current policy revision.
    pub requested_by: String,

    /// Automatic lifecycle state being requested if approved.
    pub lifecycle_state: String,

    /// Optional expiration window for the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,

    /// Optional human-readable request note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Optional structured metadata for operators.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to attach approval evidence to a persisted policy approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct AddSosPolicyApprovalEvidenceRequest {
    /// Validation report that serves as approval evidence.
    pub report_id: String,

    /// Actor attaching the evidence.
    pub added_by: String,

    /// Optional human-readable evidence note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Optional structured metadata for operators.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to approve a specific policy approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ApproveSosPolicyApprovalRequest {
    /// Actor approving the request.
    pub approved_by: String,
}

/// Request to reject a specific policy approval request.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RejectSosPolicyApprovalRequest {
    /// Actor rejecting the request.
    pub rejected_by: String,

    /// Human-readable rejection reason.
    pub reason: String,
}

/// Query parameters for listing policy approval requests.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListPolicyApprovalRequestsQuery {
    /// Filter by approval request status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Pagination offset.
    #[serde(default)]
    pub offset: usize,

    /// Pagination limit.
    #[serde(default = "default_policy_limit")]
    pub limit: usize,
}

/// Persisted policy approval evidence response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SosPolicyApprovalEvidenceResponse {
    pub evidence_id: String,
    pub request_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub policy_revision_ref: String,
    pub evidence_type: String,
    pub report_id: String,
    pub added_by: String,
    pub added_at: String,
    pub note: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Persisted policy approval request response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SosPolicyApprovalRequestResponse {
    pub request_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub policy_revision_ref: String,
    pub approval_type: String,
    pub requested_lifecycle_state: String,
    pub status: String,
    pub note: Option<String>,
    pub requested_by: String,
    pub requested_at: String,
    pub expires_at: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub rejected_by: Option<String>,
    pub rejected_at: Option<String>,
    pub rejection_reason: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub evidence: Vec<SosPolicyApprovalEvidenceResponse>,
}

/// Paginated policy approval request listing response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ListPolicyApprovalRequestsResponse {
    pub requests: Vec<SosPolicyApprovalRequestResponse>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Query parameters for listing policy approval attestations.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListPolicyAttestationsQuery {
    /// Pagination limit.
    #[serde(default = "default_policy_limit")]
    pub limit: usize,
}

/// Current SoS policy signing-key status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosPolicySigningKeyStatusResponse {
    pub signing_key_ref: Option<String>,
    pub signing_key_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_version: Option<String>,
    pub public_key: String,
    pub key_fingerprint: String,
    pub supports_rotation: bool,
    /// Trust mode for policy approvals. `software` means Graphica signs locally;
    /// `external_reference` means the local signature is paired with external trust metadata.
    pub trust_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_key_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_attestation_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_interval_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_last_rotated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_next_due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_auto_rotate: Option<bool>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request to rotate the managed SoS policy signing key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateSosPolicySigningKeyRequest {
    /// Optional operator-supplied reason captured in key metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Optional external-trust mode recorded for operator audit.
    /// This does not replace local signing; it captures provenance such as KMS/HSM custody.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_mode: Option<String>,

    /// Optional external trust provider name, for example `aws-kms` or `hsm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_provider: Option<String>,

    /// Optional external key reference such as a KMS key ARN or HSM key label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_key_ref: Option<String>,

    /// Optional operator-visible reference to external attestation evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_attestation_ref: Option<String>,
}

/// Rotation result for the managed SoS policy signing key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateSosPolicySigningKeyResponse {
    pub signing_key_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_signing_key_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_key_fingerprint: Option<String>,
    pub current_signing_key_version: String,
    pub current_key_fingerprint: String,
    pub current_public_key: String,
    pub rotated_by: String,
    pub rotated_at: String,
    pub trust_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_key_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_attestation_ref: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Query parameters for listing persisted SoS policies.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
pub struct ListPoliciesQuery {
    /// Filter by target type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,

    /// Filter by validation stage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,

    /// Filter by active status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Filter by lifecycle / rollout state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,

    /// Filter by approval status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_status: Option<String>,

    /// Pagination offset.
    #[serde(default)]
    pub offset: usize,

    /// Pagination limit.
    #[serde(default = "default_policy_limit")]
    pub limit: usize,
}

/// Persisted SoS policy response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SosPolicyResponse {
    pub policy_id: String,
    pub revision: u32,
    pub policy_ref: String,
    pub policy_revision_ref: String,
    pub policy_name: String,
    pub description: Option<String>,
    pub lifecycle_state: String,
    pub approval_status: String,
    pub approval_requested_by: Option<String>,
    pub approval_requested_at: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub rejected_by: Option<String>,
    pub rejected_at: Option<String>,
    pub rejection_reason: Option<String>,
    pub target_type: String,
    pub target_key: Option<String>,
    pub stages: Vec<String>,
    pub enforcement_level: String,
    pub severity: String,
    pub sparql_query: String,
    pub context: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub ontology_refs: Vec<String>,
    pub shape_refs: Vec<String>,
    pub active: bool,
    pub provider_interface_id: Option<String>,
    pub consumer_interface_id: Option<String>,
    pub contract_id: Option<String>,
    pub source_system_id: Option<String>,
    pub target_system_id: Option<String>,
    pub interface_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<SosPolicyAttestationResponse>,
    pub created_by: String,
    pub updated_by: String,
    pub superseded_by_revision: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
}

/// Cryptographic approval attestation material for one immutable policy revision.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosPolicyAttestationResponse {
    pub attestation_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub policy_revision_ref: String,
    pub payload_hash: String,
    pub payload_hash_algorithm: String,
    pub signature_algorithm: String,
    pub signature: String,
    pub public_key: String,
    pub key_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_version: Option<String>,
    pub signing_key_source: String,
    pub trust_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_key_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_attestation_ref: Option<String>,
    pub attested_by: String,
    pub attested_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub attestation_verified: bool,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Operator-facing policy attestation history response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListPolicyAttestationsResponse {
    pub attestations: Vec<SosPolicyAttestationResponse>,
    pub total: usize,
    pub limit: usize,
}

/// Paginated SoS policy listing response.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ListPoliciesResponse {
    pub policies: Vec<SosPolicyResponse>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Request to evaluate a persisted SoS policy directly.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct EvaluatePolicyRequest {
    /// Optional explicit validation stage. Defaults to the first configured policy stage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,

    /// Optional explicit stored revision. When omitted, evaluation uses the latest revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,

    /// Runtime context merged on top of the stored policy context.
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

// ============================================================================
// Analytics Types
// ============================================================================

/// Query parameters for compatibility-matrix generation.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams, PartialEq, Eq)]
pub struct CompatibilityMatrixQuery {
    /// Optional request-local budget for evaluated provider/consumer interface pairs.
    ///
    /// The effective budget is capped by the coordinator's configured server-side limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_budget: Option<usize>,
}

/// Query parameters for dependency-graph generation.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, utoipa::IntoParams, PartialEq, Eq)]
pub struct DependencyGraphQuery {
    /// Optional request-local budget for returned graph nodes.
    ///
    /// The effective budget is capped by the coordinator's configured server-side limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_budget: Option<usize>,

    /// Optional request-local budget for returned graph edges.
    ///
    /// The effective budget is capped by the coordinator's configured server-side limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_budget: Option<usize>,
}

/// Compatibility score between two interfaces
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct CompatibilityScore {
    /// Provider interface ID
    pub provider_interface_id: String,

    /// Consumer interface ID
    pub consumer_interface_id: String,

    /// Compatibility score (0.0 to 1.0)
    pub score: f64,

    /// Compatibility details
    pub details: Vec<CompatibilityDetail>,
}

/// Compatibility detail
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CompatibilityDetail {
    /// Aspect being checked (e.g., "schema", "units", "coordinate_system")
    pub aspect: String,

    /// Compatible or not
    pub compatible: bool,

    /// Explanation
    pub explanation: String,
}

/// Compatibility-matrix generation metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CompatibilityMatrixMetadata {
    /// Total interfaces visible to the compatibility-matrix run.
    pub total_interfaces: usize,

    /// Total ordered provider/consumer pairs eligible for evaluation.
    pub total_candidate_pairs: usize,

    /// Number of interface pairs actually evaluated in this response.
    pub evaluated_pairs: usize,

    /// Remaining candidate pairs not evaluated because the response hit its budget.
    pub remaining_candidate_pairs: usize,

    /// True when the response is partial because the evaluation budget was exhausted.
    pub truncated: bool,

    /// Request-local evaluation budget, when one was supplied by the caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_evaluation_budget: Option<usize>,

    /// Effective budget applied to this response after server-side capping.
    pub applied_evaluation_budget: usize,

    /// Server-side maximum budget for one compatibility-matrix response.
    pub server_evaluation_budget: usize,
}

/// Compatibility matrix response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct CompatibilityMatrixResponse {
    /// All compatibility scores
    pub matrix: Vec<CompatibilityScore>,

    /// Generation metadata describing how much of the matrix was evaluated.
    pub metadata: CompatibilityMatrixMetadata,

    /// Generated timestamp
    pub generated_at: String,
}

/// One node in the Systems-of-Systems dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DependencyGraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
}

/// One directed edge in the Systems-of-Systems dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DependencyGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
}

/// Dependency-graph generation metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DependencyGraphMetadata {
    /// Total graph nodes available before any response budget is applied.
    pub total_nodes: usize,

    /// Total graph edges available before any response budget is applied.
    pub total_edges: usize,

    /// Number of nodes returned in this response.
    pub returned_nodes: usize,

    /// Number of edges returned in this response.
    pub returned_edges: usize,

    /// Remaining nodes not returned because the node budget was exhausted.
    pub remaining_nodes: usize,

    /// Remaining edges not returned because the edge budget was exhausted.
    pub remaining_edges: usize,

    /// True when either the node or edge response budget produced a partial graph.
    pub truncated: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_node_budget: Option<usize>,
    pub applied_node_budget: usize,
    pub server_node_budget: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_edge_budget: Option<usize>,
    pub applied_edge_budget: usize,
    pub server_edge_budget: usize,
}

/// Dependency graph response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct DependencyGraphResponse {
    pub nodes: Vec<DependencyGraphNode>,
    pub edges: Vec<DependencyGraphEdge>,
    pub metadata: DependencyGraphMetadata,
    pub generated_at: String,
}

/// What-if analysis request
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct WhatIfRequest {
    /// Scenario description
    pub scenario: String,

    /// Changes to apply (system updates, new interfaces, etc.)
    pub changes: Vec<serde_json::Value>,

    /// Optional request-local budget for candidate compatibility/integration evaluations.
    ///
    /// The effective budget is capped by the coordinator's configured server-side limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_budget: Option<usize>,
}

/// What-if analysis generation metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct WhatIfAnalysisMetadata {
    /// Total candidate evaluations identified after applying the hypothetical overlay.
    pub total_candidate_evaluations: usize,

    /// Number of candidate evaluations actually executed for this response.
    pub evaluated_candidate_evaluations: usize,

    /// Remaining candidate evaluations not executed because the budget was exhausted.
    pub remaining_candidate_evaluations: usize,

    /// True when the what-if response is partial because the evaluation budget was exhausted.
    pub truncated: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_evaluation_budget: Option<usize>,
    pub applied_evaluation_budget: usize,
    pub server_evaluation_budget: usize,
}

/// What-if analysis response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WhatIfResponse {
    /// Scenario ID
    pub scenario_id: String,

    /// Impact analysis
    pub impact: Vec<String>,

    /// Affected systems/interfaces
    pub affected_entities: Vec<String>,

    /// Recommended actions
    pub recommendations: Vec<String>,

    /// Generation metadata describing how much of the what-if candidate set was evaluated.
    pub metadata: WhatIfAnalysisMetadata,
}

/// Administrative request to run explicit SoS reconcile/recovery actions.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct ReconcileSosRequest {
    /// When true, attempt ontology/shape asset reconciliation before rebuilding SoS RDF graphs.
    ///
    /// If the persisted ontology registry is not configured, this phase is skipped.
    #[serde(default = "default_true")]
    pub include_ontology_sync: bool,
}

/// Summary of one explicit SoS reconcile/recovery run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct ReconcileSosResponse {
    pub triggered_by: String,
    pub include_ontology_sync: bool,
    pub ontology_registry_available: bool,
    pub ontology_sync_performed: bool,
    pub graph_reconcile_performed: bool,
    pub system_count: usize,
    pub interface_count: usize,
    pub contract_count: usize,
    pub policy_count: usize,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u128,
}

// ============================================================================
// Error Types
// ============================================================================

/// SoS API error response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosErrorResponse {
    /// Error code
    pub error: String,

    /// Human-readable message
    pub message: String,

    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
