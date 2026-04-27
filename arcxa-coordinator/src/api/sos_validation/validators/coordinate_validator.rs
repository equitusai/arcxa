//! Coordinate-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
}

pub fn validate_coordinate_compatibility(
    provider_coordinate_system: Option<&str>,
    consumer_coordinate_system: Option<&str>,
    has_transform_rule: bool,
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
        (Some(provider), Some(consumer)) if has_transform_rule => {
            CoordinateCompatibilityResult {
                compatible: true,
                explanation: format!(
                    "Coordinate systems differ ({provider} -> {consumer}) but a contract transformation rule is present"
                ),
            }
        }
        (Some(provider), Some(consumer)) => CoordinateCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Coordinate systems differ ({provider} -> {consumer}) and no explicit transformation rule is defined"
            ),
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

    #[test]
    fn coordinate_mismatch_without_transform_fails() {
        let result = validate_coordinate_compatibility(Some("WGS84"), Some("ECI_J2000"), false);
        assert!(!result.compatible);
    }

    #[test]
    fn coordinate_mismatch_with_transform_passes() {
        let result = validate_coordinate_compatibility(Some("WGS84"), Some("ECI_J2000"), true);
        assert!(result.compatible);
    }
}
