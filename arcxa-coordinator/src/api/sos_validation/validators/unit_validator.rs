//! Unit-system compatibility checks for SoS validation.

use serde::{Deserialize, Serialize};

use super::transformation_validator::{
    CanonicalUnitTransformRule, DeclaredErrorBudget, TransformCompatibilityMode,
    UnitTransformSemantics,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCompatibilityResult {
    pub compatible: bool,
    pub explanation: String,
    pub compatibility_mode: TransformCompatibilityMode,
    pub declared_error_budget: Option<DeclaredErrorBudget>,
    pub confidence_score: f64,
}

impl UnitCompatibilityResult {
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

pub fn validate_unit_compatibility(
    provider_unit_system: Option<&str>,
    consumer_unit_system: Option<&str>,
    transform_rule: Option<&CanonicalUnitTransformRule>,
) -> UnitCompatibilityResult {
    match (provider_unit_system, consumer_unit_system) {
        (Some(provider), Some(consumer)) if provider.eq_ignore_ascii_case(consumer) => {
            UnitCompatibilityResult {
                compatible: true,
                explanation: format!(
                    "Unit systems are aligned ({provider} -> {consumer})"
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
                    UnitTransformSemantics::Identity => {
                        (
                            " using identity semantics".to_string(),
                            TransformCompatibilityMode::DirectAlignment,
                            None,
                            1.0,
                        )
                    }
                    UnitTransformSemantics::LinearScale {
                        scale,
                        offset,
                        tolerance,
                    } => {
                        let tolerance_suffix = tolerance
                            .map(|tolerance| format!(", tolerance={tolerance}"))
                            .unwrap_or_else(|| ", no declared error budget".to_string());
                        (
                            format!(
                                " using {} semantics (scale={scale}, offset={offset}{tolerance_suffix})",
                                rule.strategy_name()
                            ),
                            if tolerance.is_some() {
                                TransformCompatibilityMode::BoundedTransform
                            } else {
                                TransformCompatibilityMode::UnboundedTransform
                            },
                            tolerance.map(|value| DeclaredErrorBudget {
                                value,
                                label: format!("consumer-unit-system:{consumer}"),
                            }),
                            if tolerance.is_some() { 0.9 } else { 0.75 },
                        )
                    }
                };
                UnitCompatibilityResult {
                    compatible: true,
                    explanation: format!(
                        "Unit systems differ ({provider} -> {consumer}) but contract rule '{}' explicitly maps that conversion{}",
                        rule.key, semantics_description
                    ),
                    compatibility_mode,
                    declared_error_budget,
                    confidence_score,
                }
            }
            Some(rule) => UnitCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Unit systems differ ({provider} -> {consumer}) and contract rule '{}' maps {} -> {} instead",
                    rule.key, rule.from, rule.to
                ),
                compatibility_mode: TransformCompatibilityMode::UnboundedTransform,
                declared_error_budget: None,
                confidence_score: 0.0,
            },
            None => UnitCompatibilityResult {
                compatible: false,
                explanation: format!(
                    "Unit systems differ ({provider} -> {consumer}) and no semantically valid transformation rule is defined"
                ),
                compatibility_mode: TransformCompatibilityMode::UnboundedTransform,
                declared_error_budget: None,
                confidence_score: 0.0,
            },
        },
        (None, None) => UnitCompatibilityResult {
            compatible: true,
            explanation: "Neither interface declares a unit system".to_string(),
            compatibility_mode: TransformCompatibilityMode::MetadataAbsent,
            declared_error_budget: None,
            confidence_score: 0.85,
        },
        (provider, consumer) => UnitCompatibilityResult {
            compatible: false,
            explanation: format!(
                "Unit metadata is incomplete (provider: {}, consumer: {})",
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
        CanonicalUnitTransformRule, UnitTransformSemantics,
    };

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
            Some(&CanonicalUnitTransformRule {
                key: "unit_transform".to_string(),
                from: "SI".to_string(),
                to: "Imperial".to_string(),
                semantics: UnitTransformSemantics::LinearScale {
                    scale: 3.28084,
                    offset: 0.0,
                    tolerance: Some(0.01),
                },
            }),
        );
        assert!(result.compatible);
        assert!(result.explanation.contains("scale=3.28084"));
        assert_eq!(result.severity(), "info");
        assert_eq!(result.confidence_score, 0.9);
        assert_eq!(
            result
                .declared_error_budget
                .as_ref()
                .map(|budget| budget.label.as_str()),
            Some("consumer-unit-system:Imperial")
        );
    }

    #[test]
    fn mismatched_units_with_wrong_transform_fail() {
        let result = validate_unit_compatibility(
            Some("SI"),
            Some("Imperial"),
            Some(&CanonicalUnitTransformRule {
                key: "unit_transform".to_string(),
                from: "Imperial".to_string(),
                to: "SI".to_string(),
                semantics: UnitTransformSemantics::LinearScale {
                    scale: 0.3048,
                    offset: 0.0,
                    tolerance: None,
                },
            }),
        );
        assert!(!result.compatible);
        assert!(result.explanation.contains("maps Imperial -> SI instead"));
    }

    #[test]
    fn mismatched_units_with_unbounded_transform_warn() {
        let result = validate_unit_compatibility(
            Some("SI"),
            Some("Imperial"),
            Some(&CanonicalUnitTransformRule {
                key: "unit_transform".to_string(),
                from: "SI".to_string(),
                to: "Imperial".to_string(),
                semantics: UnitTransformSemantics::LinearScale {
                    scale: 3.28084,
                    offset: 0.0,
                    tolerance: None,
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
