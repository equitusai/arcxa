//! Coordinate-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

use super::transformation_validator::{
    CanonicalCoordinateTransformRule, CoordinateTransformSemantics, DeclaredErrorBudget,
    TransformCompatibilityMode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
    pub compatibility_mode: TransformCompatibilityMode,
    pub declared_error_budget: Option<DeclaredErrorBudget>,
    pub confidence_score: f64,
}

impl CoordinateCompatibilityResult {
    pub fn severity(&self) -> &'static str {
        if !self.compatible {
            "error"
        } else {
            match self.compatibility_mode {
                TransformCompatibilityMode::DirectAlignment
                | TransformCompatibilityMode::MetadataAbsent
                | TransformCompatibilityMode::BoundedTransform => "info",
                TransformCompatibilityMode::UnboundedTransform => "warning",
            }
        }
    }
}

pub fn validate_coordinate_compatibility(
    provider_coordinate_system: Option<&str>,
    consumer_coordinate_system: Option<&str>,
    transform_rule: Option<&CanonicalCoordinateTransformRule>,
) -> CoordinateCompatibilityResult {
    match (provider_coordinate_system, consumer_coordinate_system) {
        (Some(provider), Some(consumer)) if provider.eq_ignore_ascii_case(consumer) => {
            CoordinateCompatibilityResult {
                compatible: true,
                explanation: format!(
                    "Coordinate systems are aligned ({provider} -> {consumer})"
                ),
                compatibility_mode: TransformCompatibilityMode::DirectAlignment,
                declared_error_budget: None,
                confidence_score: 1.0,
            }
        }
        (Some(provider), Some(consumer)) => match transform_rule {
            Some(rule)
                if rule.from.eq_ignore_ascii_case(provider)
                    && rule.to.eq_ignore_ascii_case(consumer) =>
            {
                let (semantics_description, compatibility_mode, declared_error_budget, confidence_score) = match &rule.semantics {
                    CoordinateTransformSemantics::Identity => {
                        (
                            " using identity semantics".to_string(),
                            TransformCompatibilityMode::DirectAlignment,
                            None,
                            1.0,
                        )
                    }
                    CoordinateTransformSemantics::Helmert {
                        translation_m,
                        rotation_arcsec,
                        scale_ppm,
                        tolerance_m,
                    } => {
                        let tolerance_suffix = tolerance_m
                            .map(|tolerance| format!(", tolerance_m={tolerance}"))
                            .unwrap_or_else(|| ", no declared error budget".to_string());
                        (
                            format!(
                                " using {} semantics (translation_m={translation_m:?}, rotation_arcsec={rotation_arcsec:?}, scale_ppm={scale_ppm}{tolerance_suffix})",
                                rule.strategy_name()
                            ),
                            if tolerance_m.is_some() {
                                TransformCompatibilityMode::BoundedTransform
                            } else {
                                TransformCompatibilityMode::UnboundedTransform
                            },
                            tolerance_m.map(|value| DeclaredErrorBudget {
                                value,
                                label: "m".to_string(),
                            }),
                            if tolerance_m.is_some() { 0.9 } else { 0.75 },
                        )
                    }
                    CoordinateTransformSemantics::LocalTangentPlane {
                        origin_lat_deg,
                        origin_lon_deg,
                        origin_alt_m,
                        tolerance_m,
                    } => {
                        let tolerance_suffix = tolerance_m
                            .map(|tolerance| format!(", tolerance_m={tolerance}"))
                            .unwrap_or_else(|| ", no declared error budget".to_string());
                        (
                            format!(
                                " using {} semantics (origin=({origin_lat_deg}, {origin_lon_deg}, {origin_alt_m}){tolerance_suffix})",
                                rule.strategy_name()
                            ),
                            if tolerance_m.is_some() {
                                TransformCompatibilityMode::BoundedTransform
                            } else {
                                TransformCompatibilityMode::UnboundedTransform
                            },
                            tolerance_m.map(|value| DeclaredErrorBudget {
                                value,
                                label: "m".to_string(),
                            }),
                            if tolerance_m.is_some() { 0.9 } else { 0.75 },
                        )
                    }
                };
                CoordinateCompatibilityResult {
                    compatible: true,
                    explanation: format!(
                        "Coordinate systems differ ({provider} -> {consumer}) but contract rule '{}' explicitly maps that conversion{}",
                        rule.key, semantics_description
                    ),
                    compatibility_mode,
                    declared_error_budget,
                    confidence_score,
                }
            }
            Some(rule) => CoordinateCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Coordinate systems differ ({provider} -> {consumer}) and contract rule '{}' maps {} -> {} instead",
                    rule.key, rule.from, rule.to
                ),
                compatibility_mode: TransformCompatibilityMode::UnboundedTransform,
                declared_error_budget: None,
                confidence_score: 0.0,
            },
            None => CoordinateCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Coordinate systems differ ({provider} -> {consumer}) and no semantically valid transformation rule is defined"
                ),
                compatibility_mode: TransformCompatibilityMode::UnboundedTransform,
                declared_error_budget: None,
                confidence_score: 0.0,
            },
        },
        (None, None) => CoordinateCompatibilityResult {
            compatible: true,
            explanation: "Neither interface declares a coordinate system".to_string(),
            compatibility_mode: TransformCompatibilityMode::MetadataAbsent,
            declared_error_budget: None,
            confidence_score: 0.85,
        },
        (provider, consumer) => CoordinateCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Coordinate metadata is incomplete (provider: {}, consumer: {})",
                provider.unwrap_or("missing"),
                consumer.unwrap_or("missing")
            ),
            compatibility_mode: TransformCompatibilityMode::UnboundedTransform,
            declared_error_budget: None,
            confidence_score: 0.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::sos_validation::validators::{
        CanonicalCoordinateTransformRule, CoordinateTransformSemantics,
    };

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
            Some(&CanonicalCoordinateTransformRule {
                key: "coordinate_transform".to_string(),
                from: "WGS84".to_string(),
                to: "ECI_J2000".to_string(),
                semantics: CoordinateTransformSemantics::Helmert {
                    translation_m: [1.0, 2.0, 3.0],
                    rotation_arcsec: [0.1, 0.2, 0.3],
                    scale_ppm: 0.0,
                    tolerance_m: Some(5.0),
                },
            }),
        );
        assert!(result.compatible);
        assert!(result.explanation.contains("translation_m"));
        assert_eq!(result.severity(), "info");
        assert_eq!(result.confidence_score, 0.9);
        assert_eq!(
            result
                .declared_error_budget
                .as_ref()
                .map(|budget| budget.label.as_str()),
            Some("m")
        );
    }

    #[test]
    fn coordinate_mismatch_with_wrong_transform_fails() {
        let result = validate_coordinate_compatibility(
            Some("WGS84"),
            Some("ECI_J2000"),
            Some(&CanonicalCoordinateTransformRule {
                key: "coordinate_transform".to_string(),
                from: "ECI_J2000".to_string(),
                to: "WGS84".to_string(),
                semantics: CoordinateTransformSemantics::Helmert {
                    translation_m: [1.0, 2.0, 3.0],
                    rotation_arcsec: [0.1, 0.2, 0.3],
                    scale_ppm: 0.0,
                    tolerance_m: None,
                },
            }),
        );
        assert!(!result.compatible);
        assert!(result
            .explanation
            .contains("maps ECI_J2000 -> WGS84 instead"));
    }

    #[test]
    fn coordinate_mismatch_with_unbounded_transform_warns() {
        let result = validate_coordinate_compatibility(
            Some("WGS84"),
            Some("ECI_J2000"),
            Some(&CanonicalCoordinateTransformRule {
                key: "coordinate_transform".to_string(),
                from: "WGS84".to_string(),
                to: "ECI_J2000".to_string(),
                semantics: CoordinateTransformSemantics::Helmert {
                    translation_m: [1.0, 2.0, 3.0],
                    rotation_arcsec: [0.1, 0.2, 0.3],
                    scale_ppm: 0.0,
                    tolerance_m: None,
                },
            }),
        );
        assert!(result.compatible);
        assert_eq!(result.severity(), "warning");
        assert_eq!(result.confidence_score, 0.75);
        assert!(result.declared_error_budget.is_none());
        assert!(matches!(
            result.compatibility_mode,
            TransformCompatibilityMode::UnboundedTransform
        ));
    }
}
