//! GDPR Core Types
//!
//! Fundamental types used across GDPR compliance operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Data Subject Identifier
///
/// Represents a unique identifier for an individual whose data is being processed.
/// Can be a user ID, email, tenant ID, or any other identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DataSubjectId {
    /// The identifier value
    pub id: String,
    /// Type of identifier (e.g., "user_id", "email", "tenant_id")
    pub id_type: String,
}

impl DataSubjectId {
    /// Create a new data subject ID
    pub fn new(id: impl Into<String>, id_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            id_type: id_type.into(),
        }
    }

    /// Create a user ID data subject identifier
    pub fn user(user_id: impl Into<String>) -> Self {
        Self::new(user_id, "user_id")
    }

    /// Create an email-based data subject identifier
    pub fn email(email: impl Into<String>) -> Self {
        Self::new(email, "email")
    }

    /// Create a tenant ID data subject identifier
    pub fn tenant(tenant_id: impl Into<String>) -> Self {
        Self::new(tenant_id, "tenant_id")
    }
}

impl fmt::Display for DataSubjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.id_type, self.id)
    }
}

/// GDPR Rights
///
/// Enumeration of rights guaranteed under GDPR that can be exercised by data subjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GdprRight {
    /// Right to be informed (Article 13-14)
    Transparency,
    /// Right of access (Article 15)
    Access,
    /// Right to rectification (Article 16)
    Rectification,
    /// Right to erasure / "Right to be forgotten" (Article 17)
    Erasure,
    /// Right to restrict processing (Article 18)
    RestrictionOfProcessing,
    /// Right to data portability (Article 20)
    DataPortability,
    /// Right to object (Article 21)
    ObjectToProcessing,
    /// Rights related to automated decision making and profiling (Article 22)
    AutomatedDecisionMaking,
}

impl fmt::Display for GdprRight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GdprRight::Transparency => write!(f, "Right to be informed"),
            GdprRight::Access => write!(f, "Right of access"),
            GdprRight::Rectification => write!(f, "Right to rectification"),
            GdprRight::Erasure => write!(f, "Right to erasure"),
            GdprRight::RestrictionOfProcessing => write!(f, "Right to restrict processing"),
            GdprRight::DataPortability => write!(f, "Right to data portability"),
            GdprRight::ObjectToProcessing => write!(f, "Right to object"),
            GdprRight::AutomatedDecisionMaking => {
                write!(f, "Rights related to automated decision making")
            }
        }
    }
}

/// Legal basis for processing personal data under GDPR Article 6
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingBasis {
    /// Consent of the data subject
    Consent,
    /// Processing is necessary for the performance of a contract
    Contract,
    /// Processing is necessary for compliance with a legal obligation
    LegalObligation,
    /// Processing is necessary to protect vital interests
    VitalInterests,
    /// Processing is necessary for the performance of a task carried out in the public interest
    PublicInterest,
    /// Processing is necessary for legitimate interests pursued by the controller
    LegitimateInterests,
}

/// GDPR Audit Event
///
/// Records actions taken in response to GDPR requests for compliance auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdprAuditEvent {
    /// Unique event identifier
    pub event_id: String,
    /// The GDPR right being exercised
    pub gdpr_right: GdprRight,
    /// The data subject making the request
    pub data_subject: DataSubjectId,
    /// When the request was initiated
    pub requested_at: DateTime<Utc>,
    /// When the request was completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
    /// Whether the request succeeded
    pub success: bool,
    /// Human-readable description of the action taken
    pub action_description: String,
    /// Technical details (storage backends affected, record counts, etc.)
    pub technical_details: Option<serde_json::Value>,
    /// User or system that initiated the request
    pub initiated_by: String,
    /// Any errors encountered
    pub error_message: Option<String>,
}

impl GdprAuditEvent {
    /// Create a new GDPR audit event
    pub fn new(
        gdpr_right: GdprRight,
        data_subject: DataSubjectId,
        initiated_by: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            gdpr_right,
            data_subject,
            requested_at: Utc::now(),
            completed_at: None,
            success: false,
            action_description: String::new(),
            technical_details: None,
            initiated_by: initiated_by.into(),
            error_message: None,
        }
    }

    /// Mark the event as completed successfully
    pub fn complete_success(mut self, description: impl Into<String>) -> Self {
        self.completed_at = Some(Utc::now());
        self.success = true;
        self.action_description = description.into();
        self
    }

    /// Mark the event as failed
    pub fn complete_failure(mut self, error: impl Into<String>) -> Self {
        self.completed_at = Some(Utc::now());
        self.success = false;
        self.error_message = Some(error.into());
        self
    }

    /// Add technical details to the event
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.technical_details = Some(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_subject_id_creation() {
        let user_id = DataSubjectId::user("user123");
        assert_eq!(user_id.id, "user123");
        assert_eq!(user_id.id_type, "user_id");
        assert_eq!(user_id.to_string(), "user_id:user123");

        let email_id = DataSubjectId::email("test@example.com");
        assert_eq!(email_id.id, "test@example.com");
        assert_eq!(email_id.id_type, "email");
        assert_eq!(email_id.to_string(), "email:test@example.com");

        let tenant_id = DataSubjectId::tenant("tenant-456");
        assert_eq!(tenant_id.id, "tenant-456");
        assert_eq!(tenant_id.id_type, "tenant_id");
    }

    #[test]
    fn test_gdpr_right_display() {
        assert_eq!(GdprRight::Erasure.to_string(), "Right to erasure");
        assert_eq!(
            GdprRight::DataPortability.to_string(),
            "Right to data portability"
        );
    }

    #[test]
    fn test_gdpr_audit_event_lifecycle() {
        let data_subject = DataSubjectId::user("user123");
        let mut event = GdprAuditEvent::new(GdprRight::Erasure, data_subject, "admin@example.com");

        assert!(!event.success);
        assert!(event.completed_at.is_none());
        assert_eq!(event.gdpr_right, GdprRight::Erasure);

        event = event.complete_success("All user data erased from 5 storage backends");
        assert!(event.success);
        assert!(event.completed_at.is_some());
        assert_eq!(
            event.action_description,
            "All user data erased from 5 storage backends"
        );

        let failed_event = GdprAuditEvent::new(
            GdprRight::DataPortability,
            DataSubjectId::user("user456"),
            "system",
        )
        .complete_failure("Export failed: database connection timeout");

        assert!(!failed_event.success);
        assert!(failed_event.error_message.is_some());
    }

    #[test]
    fn test_processing_basis_serialization() {
        let basis = ProcessingBasis::Consent;
        let json = serde_json::to_string(&basis).unwrap();
        assert_eq!(json, "\"consent\"");

        let deserialized: ProcessingBasis = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ProcessingBasis::Consent);
    }
}
