//! Unit-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

use super::transformation_validator::CanonicalTransformRule;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
}

pub fn validate_unit_compatibility(
    provider_unit_system: Option<&str>,
    consumer_unit_system: Option<&str>,
    transform_rule: Option<&CanonicalTransformRule>,
) -> UnitCompatibilityResult {
    match (provider_unit_system, consumer_unit_system) {
        (Some(provider), Some(consumer)) if provider.eq_ignore_ascii_case(consumer) => {
            UnitCompatibilityResult {
                compatible: true,
                explanation: format!(
                    "Unit systems are aligned ({provider} -> {consumer})"
                ),
            }
        }
        (Some(provider), Some(consumer)) => match transform_rule {
            Some(rule)
                if rule.from.eq_ignore_ascii_case(provider)
                    && rule.to.eq_ignore_ascii_case(consumer) =>
            {
                UnitCompatibilityResult {
                    compatible: true,
                    explanation: format!(
                        "Unit systems differ ({provider} -> {consumer}) but contract rule '{}' explicitly maps that conversion{}",
                        rule.key,
                        rule.strategy
                            .as_deref()
                            .map(|strategy| format!(" using strategy '{strategy}'"))
                            .unwrap_or_default()
                    ),
                }
            }
            Some(rule) => UnitCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Unit systems differ ({provider} -> {consumer}) and contract rule '{}' maps {} -> {} instead",
                    rule.key, rule.from, rule.to
                ),
            },
            None => UnitCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Unit systems differ ({provider} -> {consumer}) and no explicit transformation rule is defined"
                ),
            },
        },
        (None, None) => UnitCompatibilityResult {
            compatible: true,
            explanation: "Neither interface declares a unit system".to_string(),
        },
        (provider, consumer) => UnitCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Unit metadata is incomplete (provider: {}, consumer: {})",
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
    fn mismatched_units_without_transform_fail() {
        let result = validate_unit_compatibility(Some("SI"), Some("Imperial"), None);
        assert!(!result.compatible);
    }

    #[test]
    fn mismatched_units_with_transform_pass() {
        let result = validate_unit_compatibility(
            Some("SI"),
            Some("Imperial"),
            Some(&CanonicalTransformRule {
                key: "unit_transform".to_string(),
                from: "SI".to_string(),
                to: "Imperial".to_string(),
                strategy: Some("linear_scale".to_string()),
            }),
        );
        assert!(result.compatible);
    }

    #[test]
    fn mismatched_units_with_wrong_transform_fail() {
        let result = validate_unit_compatibility(
            Some("SI"),
            Some("Imperial"),
            Some(&CanonicalTransformRule {
                key: "unit_transform".to_string(),
                from: "Imperial".to_string(),
                to: "SI".to_string(),
                strategy: None,
            }),
        );
        assert!(!result.compatible);
        assert!(result.explanation.contains("maps Imperial -> SI instead"));
    }
}
