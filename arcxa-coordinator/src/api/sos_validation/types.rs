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

    /// Approval status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
}

/// Response containing data contract information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataContractResponse {
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

    /// Created timestamp
    pub created_at: String,

    /// Updated timestamp
    pub updated_at: String,
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

// ============================================================================
// Analytics Types
// ============================================================================

/// Compatibility score between two interfaces
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompatibilityDetail {
    /// Aspect being checked (e.g., "schema", "units", "coordinate_system")
    pub aspect: String,

    /// Compatible or not
    pub compatible: bool,

    /// Explanation
    pub explanation: String,
}

/// Compatibility matrix response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompatibilityMatrixResponse {
    /// All compatibility scores
    pub matrix: Vec<CompatibilityScore>,

    /// Generated timestamp
    pub generated_at: String,
}

/// What-if analysis request
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct WhatIfRequest {
    /// Scenario description
    pub scenario: String,

    /// Changes to apply (system updates, new interfaces, etc.)
    pub changes: Vec<serde_json::Value>,
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
