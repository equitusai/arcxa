//! Consent Management
//!
//! Implements GDPR Article 7 (Conditions for consent) and supports tracking
//! user consent for different processing purposes.
//!
//! This is a placeholder module that will be fully implemented in Phase 2.

use super::types::{DataSubjectId, ProcessingBasis};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Consent Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentStatus {
    /// Consent has been granted
    Granted,
    /// Consent has been explicitly denied
    Denied,
    /// Consent has been withdrawn after being granted
    Withdrawn,
    /// Consent is pending (awaiting user response)
    Pending,
}

/// Purpose for which consent is requested
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsentPurpose {
    /// Unique identifier for this purpose
    pub purpose_id: String,
    /// Human-readable description
    pub description: String,
    /// Whether consent for this purpose is required (vs optional)
    pub required: bool,
}

/// Consent Record
///
/// Tracks a user's consent decision for a specific processing purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// Unique identifier for this consent record
    pub consent_id: String,
    /// The data subject who gave (or denied) consent
    pub data_subject: DataSubjectId,
    /// The purpose for which consent was requested
    pub purpose: ConsentPurpose,
    /// Current consent status
    pub status: ConsentStatus,
    /// When consent was granted (if applicable)
    pub granted_at: Option<DateTime<Utc>>,
    /// When consent was withdrawn (if applicable)
    pub withdrawn_at: Option<DateTime<Utc>>,
    /// When this record was last updated
    pub updated_at: DateTime<Utc>,
    /// Legal basis for processing (may be different from consent)
    pub processing_basis: ProcessingBasis,
}

/// Consent Manager Trait
///
/// Storage backends for consent management implement this trait.
/// Full implementation will be added in Phase 2.
pub trait ConsentManager: Send + Sync {
    // Placeholder - will be implemented in Phase 2
}
