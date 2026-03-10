//! Fusion Validation
//!
//! Validation logic for entity fusion and resolution operations.

use crate::api::dto::ApiError;

/// Validate match rule is supported
pub fn validate_match_rule(rule: &str) -> Result<(), ApiError> {
    tracing::debug!("Validating match rule: {}", rule);
    let supported_rules = ["email", "phone", "ssn", "name", "address", "tax_id"];

    if !supported_rules.contains(&rule) {
        tracing::warn!("Unsupported match rule: {}", rule);
        return Err(ApiError::bad_request(format!(
            "Unsupported match rule '{}'. Supported rules: {}",
            rule,
            supported_rules.join(", ")
        )));
    }

    tracing::debug!("Match rule validation passed: {}", rule);
    Ok(())
}

/// Validate minimum entity count for fusion
pub fn validate_entity_count(
    entities: &[serde_json::Map<String, serde_json::Value>],
) -> Result<(), ApiError> {
    tracing::debug!("Validating entity count: {} entities", entities.len());

    if entities.len() < 2 {
        tracing::warn!("Insufficient entities for fusion: {}", entities.len());
        return Err(ApiError::bad_request(format!(
            "Fusion requires at least 2 entities, got {}",
            entities.len()
        )));
    }

    if entities.len() > 100 {
        tracing::warn!("Too many entities for fusion: {}", entities.len());
        return Err(ApiError::bad_request(format!(
            "Fusion supports maximum 100 entities, got {}",
            entities.len()
        )));
    }

    tracing::debug!(
        "Entity count validation passed: {} entities",
        entities.len()
    );
    Ok(())
}
