//! GDPR to RDF Conversion
//!
//! Converts GDPR Rust types (ConsentRecord, LegalHold, etc.) to RDF triples
//! for storage in the governance brain.
//!
//! This enables SPARQL queries like:
//! - "Show all consent records for user X"
//! - "What legal basis justifies processing activity Y?"
//! - "Which users are under legal hold?"

use anyhow::Result;
use chrono::{DateTime, Utc};
use graphica_core::gdpr::{
    ConsentPurpose, ConsentRecord, ConsentStatus, DataCategory, DataSubjectId, ErasureRequest,
    ErasureResult, ErasureStrategy, LegalHold, RetentionPolicy,
};

use super::ontology::{uris, GDPR_NS};

/// RDF Triple
pub type Triple = (String, String, String);

/// Convert ConsentRecord to RDF triples
///
/// Creates triples for:
/// - Data subject with consent relationship
/// - Consent record with all properties
/// - Status, purpose, timestamps
pub fn consent_record_to_rdf(consent: &ConsentRecord) -> Vec<Triple> {
    let mut triples = Vec::new();

    let subject_uri = uris::data_subject(&consent.data_subject.id);
    let consent_uri = uris::consent_record(&consent.consent_id);

    // Data Subject class
    triples.push((
        subject_uri.clone(),
        "rdf:type".to_string(),
        format!("<{}>DataSubject", GDPR_NS),
    ));

    triples.push((
        subject_uri.clone(),
        format!("<{}>subjectId", GDPR_NS),
        format!("\"{}\"", consent.data_subject.id),
    ));

    triples.push((
        subject_uri.clone(),
        format!("<{}>subjectType", GDPR_NS),
        format!("\"{}\"", consent.data_subject.id_type),
    ));

    // Consent Record class
    triples.push((
        consent_uri.clone(),
        "rdf:type".to_string(),
        format!("<{}>ConsentRecord", GDPR_NS),
    ));

    // Link subject to consent
    triples.push((
        subject_uri,
        format!("<{}>hasConsent", GDPR_NS),
        format!("<{}>", consent_uri),
    ));

    // Consent properties
    triples.push((
        consent_uri.clone(),
        format!("<{}>consentId", GDPR_NS),
        format!("\"{}\"", consent.consent_id),
    ));

    triples.push((
        consent_uri.clone(),
        format!("<{}>consentPurpose", GDPR_NS),
        format!("\"{}\"", consent.purpose.purpose_id),
    ));

    triples.push((
        consent_uri.clone(),
        format!("<{}>consentStatus", GDPR_NS),
        format!("\"{}\"", consent_status_to_string(consent.status)),
    ));

    if let Some(granted_at) = consent.granted_at {
        triples.push((
            consent_uri.clone(),
            format!("<{}>grantedAt", GDPR_NS),
            format_datetime(granted_at),
        ));
    }

    if let Some(withdrawn_at) = consent.withdrawn_at {
        triples.push((
            consent_uri.clone(),
            format!("<{}>withdrawnAt", GDPR_NS),
            format_datetime(withdrawn_at),
        ));
    }

    triples
}

/// Convert LegalHold to RDF triples
pub fn legal_hold_to_rdf(hold: &LegalHold) -> Vec<Triple> {
    let mut triples = Vec::new();

    let hold_uri = uris::legal_hold(&hold.id);

    // Legal Hold class
    triples.push((
        hold_uri.clone(),
        "rdf:type".to_string(),
        format!("<{}>LegalHold", GDPR_NS),
    ));

    // Hold properties
    triples.push((
        hold_uri.clone(),
        format!("<{}>holdId", GDPR_NS),
        format!("\"{}\"", hold.id),
    ));

    triples.push((
        hold_uri.clone(),
        format!("<{}>holdName", GDPR_NS),
        format!("\"{}\"", hold.name),
    ));

    triples.push((
        hold_uri.clone(),
        format!("<{}>holdReason", GDPR_NS),
        format!("\"{}\"", hold.reason),
    ));

    triples.push((
        hold_uri.clone(),
        format!("<{}>holdPlacedAt", GDPR_NS),
        format_datetime(hold.placed_at),
    ));

    triples.push((
        hold_uri.clone(),
        format!("<{}>holdPlacedBy", GDPR_NS),
        format!("\"{}\"", hold.placed_by),
    ));

    // Link to data subjects
    for subject_id in &hold.data_subjects {
        let subject_uri = uris::data_subject(subject_id);

        triples.push((
            hold_uri.clone(),
            format!("<{}>holdsData", GDPR_NS),
            format!("<{}>", subject_uri),
        ));
    }

    triples
}

/// Convert RetentionPolicy to RDF triples
pub fn retention_policy_to_rdf(policy: &RetentionPolicy) -> Vec<Triple> {
    let mut triples = Vec::new();

    let policy_uri = uris::retention_policy(&policy.id);

    // Retention Policy class
    triples.push((
        policy_uri.clone(),
        "rdf:type".to_string(),
        format!("<{}>RetentionPolicy", GDPR_NS),
    ));

    // Policy properties
    triples.push((
        policy_uri.clone(),
        format!("<{}>dataCategory", GDPR_NS),
        format!("\"{}\"", data_category_to_string(&policy.data_category)),
    ));

    if let Some(min_days) = policy.min_retention_days {
        triples.push((
            policy_uri.clone(),
            format!("<{}>minRetentionDays", GDPR_NS),
            format!(
                "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                min_days
            ),
        ));
    }

    if let Some(max_days) = policy.max_retention_days {
        triples.push((
            policy_uri.clone(),
            format!("<{}>maxRetentionDays", GDPR_NS),
            format!(
                "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                max_days
            ),
        ));
    }

    triples.push((
        policy_uri.clone(),
        format!("<{}>legalBasis", GDPR_NS),
        format!("\"{}\"", policy.legal_basis),
    ));

    triples
}

/// Convert ErasureResult to RDF triples
pub fn erasure_result_to_rdf(result: &ErasureResult) -> Vec<Triple> {
    let mut triples = Vec::new();

    let request_uri = uris::erasure_request(&result.request.request_id);

    // Erasure Request class
    triples.push((
        request_uri.clone(),
        "rdf:type".to_string(),
        format!("<{}>ErasureRequest", GDPR_NS),
    ));

    // Request properties
    triples.push((
        request_uri.clone(),
        format!("<{}>erasureStrategy", GDPR_NS),
        format!(
            "\"{}\"",
            erasure_strategy_to_string(
                result
                    .request
                    .strategy
                    .unwrap_or(ErasureStrategy::HardDelete)
            )
        ),
    ));

    triples.push((
        request_uri.clone(),
        format!("<{}>recordsErased", GDPR_NS),
        format!(
            "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
            result.total_records_erased
        ),
    ));

    triples.push((
        request_uri.clone(),
        format!("<{}>erasedAt", GDPR_NS),
        format_datetime(result.completed_at),
    ));

    // Link to data subject
    let subject_uri = uris::data_subject(&result.request.data_subject.id);
    triples.push((
        request_uri,
        "prov:wasAssociatedWith".to_string(),
        format!("<{}>", subject_uri),
    ));

    triples
}

// Helper functions

fn consent_status_to_string(status: ConsentStatus) -> &'static str {
    match status {
        ConsentStatus::Granted => "granted",
        ConsentStatus::Denied => "denied",
        ConsentStatus::Withdrawn => "withdrawn",
        ConsentStatus::Pending => "pending",
    }
}

fn data_category_to_string(category: &DataCategory) -> String {
    match category {
        DataCategory::PersonalIdentifiers => "personal_identifiers".to_string(),
        DataCategory::Financial => "financial".to_string(),
        DataCategory::AuditLogs => "audit_logs".to_string(),
        DataCategory::UserContent => "user_content".to_string(),
        DataCategory::SystemLogs => "system_logs".to_string(),
        DataCategory::Marketing => "marketing".to_string(),
        DataCategory::Analytics => "analytics".to_string(),
        DataCategory::Backups => "backups".to_string(),
        DataCategory::Custom(s) => s.clone(),
    }
}

fn erasure_strategy_to_string(strategy: ErasureStrategy) -> &'static str {
    match strategy {
        ErasureStrategy::HardDelete => "hard_delete",
        ErasureStrategy::Tombstone => "tombstone",
        ErasureStrategy::Anonymize => "anonymize",
        ErasureStrategy::ArchiveThenDelete => "archive_then_delete",
    }
}

fn format_datetime(dt: DateTime<Utc>) -> String {
    format!(
        "\"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>",
        dt.to_rfc3339()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_record_to_rdf() {
        let consent = ConsentRecord {
            consent_id: "consent123".to_string(),
            data_subject: DataSubjectId::user("user456"),
            purpose: ConsentPurpose {
                purpose_id: "marketing".to_string(),
                description: "Marketing emails".to_string(),
                required: false,
            },
            status: ConsentStatus::Granted,
            granted_at: Some(Utc::now()),
            withdrawn_at: None,
            updated_at: Utc::now(),
            processing_basis: graphica_core::gdpr::ProcessingBasis::Consent,
        };

        let triples = consent_record_to_rdf(&consent);

        assert!(!triples.is_empty());
        assert!(triples.iter().any(|(_, p, _)| p.contains("hasConsent")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("consentId")));
        assert!(triples.iter().any(|(_, _, o)| o.contains("marketing")));
    }

    #[test]
    fn test_legal_hold_to_rdf() {
        let hold =
            LegalHold::new("Case 123", "Legal Team", "Litigation").add_data_subject("user789");

        let triples = legal_hold_to_rdf(&hold);

        assert!(!triples.is_empty());
        assert!(triples.iter().any(|(_, p, _)| p.contains("holdName")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("holdsData")));
    }

    #[test]
    fn test_retention_policy_to_rdf() {
        let policy = RetentionPolicy::new(
            "Financial Data",
            DataCategory::Financial,
            Some(2555), // 7 years
            Some(3650), // 10 years
        );

        let triples = retention_policy_to_rdf(&policy);

        assert!(!triples.is_empty());
        assert!(triples.iter().any(|(_, p, _)| p.contains("dataCategory")));
        assert!(triples
            .iter()
            .any(|(_, p, _)| p.contains("minRetentionDays")));
        assert!(triples.iter().any(|(_, _, o)| o.contains("2555")));
    }
}
