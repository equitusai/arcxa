//! Unit-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
}

pub fn validate_unit_compatibility(
    provider_unit_system: Option<&str>,
    consumer_unit_system: Option<&str>,
    has_transform_rule: bool,
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
        (Some(provider), Some(consumer)) if has_transform_rule => UnitCompatibilityResult {
            compatible: true,
            explanation: format!(
                "Unit systems differ ({provider} -> {consumer}) but a contract transformation rule is present"
            ),
        },
        (Some(provider), Some(consumer)) => UnitCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Unit systems differ ({provider} -> {consumer}) and no explicit transformation rule is defined"
            ),
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

    #[test]
    fn mismatched_units_without_transform_fail() {
        let result = validate_unit_compatibility(Some("SI"), Some("Imperial"), false);
        assert!(!result.compatible);
    }

    #[test]
    fn mismatched_units_with_transform_pass() {
        let result = validate_unit_compatibility(Some("SI"), Some("Imperial"), true);
        assert!(result.compatible);
    }
}
