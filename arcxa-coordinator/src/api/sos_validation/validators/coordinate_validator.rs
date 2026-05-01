//! Coordinate-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

use super::transformation_validator::CanonicalTransformRule;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
}

pub fn validate_coordinate_compatibility(
    provider_coordinate_system: Option<&str>,
    consumer_coordinate_system: Option<&str>,
    transform_rule: Option<&CanonicalTransformRule>,
) -> CoordinateCompatibilityResult {
    match (provider_coordinate_system, consumer_coordinate_system) {
        (Some(provider), Some(consumer)) if provider.eq_ignore_ascii_case(consumer) => {
            CoordinateCompatibilityResult {
                compatible: true,
                explanation: format!(
                    "Coordinate systems are aligned ({provider} -> {consumer})"
                ),
            }
        }
        (Some(provider), Some(consumer)) => match transform_rule {
            Some(rule)
                if rule.from.eq_ignore_ascii_case(provider)
                    && rule.to.eq_ignore_ascii_case(consumer) =>
            {
                CoordinateCompatibilityResult {
                    compatible: true,
                    explanation: format!(
                        "Coordinate systems differ ({provider} -> {consumer}) but contract rule '{}' explicitly maps that conversion{}",
                        rule.key,
                        rule.strategy
                            .as_deref()
                            .map(|strategy| format!(" using strategy '{strategy}'"))
                            .unwrap_or_default()
                    ),
                }
            }
            Some(rule) => CoordinateCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Coordinate systems differ ({provider} -> {consumer}) and contract rule '{}' maps {} -> {} instead",
                    rule.key, rule.from, rule.to
                ),
            },
            None => CoordinateCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Coordinate systems differ ({provider} -> {consumer}) and no explicit transformation rule is defined"
                ),
            },
        },
        (None, None) => CoordinateCompatibilityResult {
            compatible: true,
            explanation: "Neither interface declares a coordinate system".to_string(),
        },
        (provider, consumer) => CoordinateCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Coordinate metadata is incomplete (provider: {}, consumer: {})",
                provider.unwrap_or("missing"),
                consumer.unwrap_or("missing")
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::sos_validation::validators::CanonicalTransformRule;

    #[test]
    fn coordinate_mismatch_without_transform_fails() {
        let result = validate_coordinate_compatibility(Some("WGS84"), Some("ECI_J2000"), None);
        assert!(!result.compatible);
    }

    #[test]
    fn coordinate_mismatch_with_transform_passes() {
        let result = validate_coordinate_compatibility(
            Some("WGS84"),
            Some("ECI_J2000"),
            Some(&CanonicalTransformRule {
                key: "coordinate_transform".to_string(),
                from: "WGS84".to_string(),
                to: "ECI_J2000".to_string(),
                strategy: Some("helmert".to_string()),
            }),
        );
        assert!(result.compatible);
    }

    #[test]
    fn coordinate_mismatch_with_wrong_transform_fails() {
        let result = validate_coordinate_compatibility(
            Some("WGS84"),
            Some("ECI_J2000"),
            Some(&CanonicalTransformRule {
                key: "coordinate_transform".to_string(),
                from: "ECI_J2000".to_string(),
                to: "WGS84".to_string(),
                strategy: None,
            }),
        );
        assert!(!result.compatible);
        assert!(result
            .explanation
            .contains("maps ECI_J2000 -> WGS84 instead"));
    }
}
