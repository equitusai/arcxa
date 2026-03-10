////! GDPR API Types
//!
//! Request and response types for GDPR compliance operations.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to erase tenant data (GDPR Article 17: Right to Erasure)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EraseTenantDataRequest {
    /// Whether this is a dry-run (preview without deleting)
    #[serde(default)]
    pub dry_run: bool,
}

/// Response to tenant data erasure request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EraseTenantDataResponse {
    /// Whether the operation was successful
    pub success: bool,

    /// Total number of records erased
    pub total_records_erased: u64,

    /// Number of backends that succeeded
    pub backends_succeeded: usize,

    /// Number of backends that failed
    pub backends_failed: usize,

    /// Whether this was a dry-run
    pub dry_run: bool,

    /// Detailed breakdown by backend
    pub backend_results: Vec<BackendErasureDetail>,

    /// Optional error messages
    pub errors: Vec<String>,
}

/// Detailed erasure result for a single backend
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackendErasureDetail {
    /// Backend name
    pub backend_name: String,

    /// Whether this backend succeeded
    pub success: bool,

    /// Number of records erased from this backend
    pub records_erased: u64,

    /// Optional error message
    pub error_message: Option<String>,
}

/// Response for tenant data count query
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantDataCountResponse {
    /// Tenant ID
    pub tenant_id: String,

    /// Total number of records across all backends
    pub total_records: u64,

    /// Breakdown by backend
    pub breakdown: std::collections::HashMap<String, u64>,
}

/// Response for erasure verification
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyErasureResponse {
    /// Tenant ID
    pub tenant_id: String,

    /// Whether erasure is verified (count is zero)
    pub verified: bool,

    /// Remaining records (if any)
    pub remaining_records: u64,
}

/// Request to erase user data (GDPR Article 17: Right to Erasure)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EraseUserDataRequest {
    /// Whether this is a dry-run (preview without deleting)
    #[serde(default)]
    pub dry_run: bool,

    /// Erasure strategy to use
    /// Options: "hard_delete", "anonymize", "tombstone", "archive_then_delete"
    #[serde(default = "default_erasure_strategy")]
    pub strategy: String,
}

fn default_erasure_strategy() -> String {
    "hard_delete".to_string()
}

/// Response to user data erasure request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EraseUserDataResponse {
    /// Whether the operation was successful
    pub success: bool,

    /// Total number of records erased
    pub total_records_erased: u64,

    /// Number of backends that succeeded
    pub backends_succeeded: usize,

    /// Number of backends that failed
    pub backends_failed: usize,

    /// Whether this was a dry-run
    pub dry_run: bool,

    /// Strategy used
    pub strategy: String,

    /// Detailed breakdown by backend
    pub backend_results: Vec<BackendErasureDetail>,

    /// Optional error messages
    pub errors: Vec<String>,

    /// Warnings (e.g., retention policy violations)
    pub warnings: Vec<String>,
}

/// Response for user data count query
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserDataCountResponse {
    /// User ID
    pub user_id: String,

    /// Total number of records across all backends
    pub total_records: u64,

    /// Breakdown by backend
    pub breakdown: std::collections::HashMap<String, u64>,
}

/// Request to check legal holds for a user
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckLegalHoldResponse {
    /// User ID
    pub user_id: String,

    /// Whether user is under legal hold
    pub under_hold: bool,

    /// Active legal holds (if any)
    pub active_holds: Vec<LegalHoldInfo>,
}

/// Legal hold information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LegalHoldInfo {
    /// Hold ID
    pub id: String,

    /// Hold name/case number
    pub name: String,

    /// Reason for the hold
    pub reason: String,

    /// When the hold was placed
    pub placed_at: String,

    /// Who placed the hold
    pub placed_by: String,

    /// Optional expiry date
    pub expires_at: Option<String>,
}
