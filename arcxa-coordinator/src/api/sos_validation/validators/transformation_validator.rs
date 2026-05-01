//! Contract transformation-rule validation for SoS compatibility checks.

use super::schema_validator::{SchemaCompatibilityIssueKind, SchemaCompatibilityReport};
use serde_json::{Map, Value};
use std::collections::HashMap;

const UNIT_RULE_KEYS: [&str; 4] = ["unit_transform", "unit_conversion", "unit_mapping", "unit"];
const COORDINATE_RULE_KEYS: [&str; 4] = [
    "coordinate_transform",
    "coordinate_conversion",
    "coordinate_mapping",
    "coordinate",
];
const FIELD_MAPPING_RULE_KEYS: [&str; 3] = ["field_mapping", "field_mappings", "field_transform"];
const FROM_FIELD_ALIASES: [&str; 3] = ["from", "source", "provider"];
const TO_FIELD_ALIASES: [&str; 3] = ["to", "target", "consumer"];
const STRATEGY_FIELD_ALIASES: [&str; 3] = ["strategy", "operation", "method"];
const MAPPINGS_FIELD_ALIASES: [&str; 2] = ["mappings", "rules"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTransformRule {
    pub key: String,
    pub from: String,
    pub to: String,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMappingRule {
    pub from: Option<String>,
    pub to: String,
    pub strategy: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalFieldMappingRuleSet {
    pub key: String,
    pub mappings: Vec<FieldMappingRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaTransformabilityReport {
    pub transformable: bool,
    pub covered_paths: Vec<String>,
    pub uncovered_issues: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransformationRulesValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub unit_rule: Option<CanonicalTransformRule>,
    pub coordinate_rule: Option<CanonicalTransformRule>,
    pub field_mapping_rule: Option<CanonicalFieldMappingRuleSet>,
}

pub fn validate_contract_transformation_rules(
    transformation_rules: &HashMap<String, Value>,
) -> TransformationRulesValidation {
    let mut issues = Vec::new();
    let unit_rule = extract_rule("unit", &UNIT_RULE_KEYS, transformation_rules, &mut issues);
    let coordinate_rule = extract_rule(
        "coordinate",
        &COORDINATE_RULE_KEYS,
        transformation_rules,
        &mut issues,
    );
    let field_mapping_rule = extract_field_mapping_rule(transformation_rules, &mut issues);

    TransformationRulesValidation {
        valid: issues.is_empty(),
        issues,
        unit_rule,
        coordinate_rule,
        field_mapping_rule,
    }
}

pub fn evaluate_schema_transformability(
    schema_report: &SchemaCompatibilityReport,
    field_mapping_rule: Option<&CanonicalFieldMappingRuleSet>,
) -> SchemaTransformabilityReport {
    if schema_report.compatible {
        return SchemaTransformabilityReport {
            transformable: true,
            covered_paths: Vec::new(),
            uncovered_issues: Vec::new(),
        };
    }

    let Some(field_mapping_rule) = field_mapping_rule else {
        return SchemaTransformabilityReport {
            transformable: false,
            covered_paths: Vec::new(),
            uncovered_issues: schema_report.issues.clone(),
        };
    };

    let mut covered_paths = Vec::new();
    let mut uncovered_issues = Vec::new();

    for issue in &schema_report.structured_issues {
        match issue.kind {
            SchemaCompatibilityIssueKind::MissingRequiredField => {
                let covered = field_mapping_rule
                    .mappings
                    .iter()
                    .any(|mapping| mapping.to == issue.path);
                if covered {
                    covered_paths.push(issue.path.clone());
                } else {
                    uncovered_issues.push(issue.message.clone());
                }
            }
            _ => uncovered_issues.push(issue.message.clone()),
        }
    }

    covered_paths.sort();
    covered_paths.dedup();

    SchemaTransformabilityReport {
        transformable: !covered_paths.is_empty() && uncovered_issues.is_empty(),
        covered_paths,
        uncovered_issues,
    }
}

fn extract_rule(
    kind: &str,
    aliases: &[&str],
    transformation_rules: &HashMap<String, Value>,
    issues: &mut Vec<String>,
) -> Option<CanonicalTransformRule> {
    let matches: Vec<(&str, &Value)> = aliases
        .iter()
        .filter_map(|alias| {
            transformation_rules
                .get(*alias)
                .map(|value| (*alias, value))
        })
        .collect();

    match matches.as_slice() {
        [] => None,
        [(key, value)] => parse_rule(kind, key, value, issues),
        multiple => {
            let keys = multiple
                .iter()
                .map(|(key, _)| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(", ");
            issues.push(format!(
                "Multiple {kind} transformation rules are defined ({keys}); keep a single canonical rule"
            ));
            None
        }
    }
}

fn parse_rule(
    kind: &str,
    key: &str,
    value: &Value,
    issues: &mut Vec<String>,
) -> Option<CanonicalTransformRule> {
    let object = match value {
        Value::Object(object) => object,
        other => {
            issues.push(format!(
                "Transformation rule '{key}' for {kind} compatibility must be an object with non-empty 'from' and 'to' fields, got {}",
                value_kind(other)
            ));
            return None;
        }
    };

    let from = match extract_string_field(object, &FROM_FIELD_ALIASES) {
        Ok(value) => value,
        Err(message) => {
            issues.push(format!("Transformation rule '{key}' {message}"));
            return None;
        }
    };
    let to = match extract_string_field(object, &TO_FIELD_ALIASES) {
        Ok(value) => value,
        Err(message) => {
            issues.push(format!("Transformation rule '{key}' {message}"));
            return None;
        }
    };
    let strategy = match extract_optional_string_field(object, &STRATEGY_FIELD_ALIASES) {
        Ok(value) => value,
        Err(message) => {
            issues.push(format!("Transformation rule '{key}' {message}"));
            return None;
        }
    };

    Some(CanonicalTransformRule {
        key: key.to_string(),
        from,
        to,
        strategy,
    })
}

fn extract_field_mapping_rule(
    transformation_rules: &HashMap<String, Value>,
    issues: &mut Vec<String>,
) -> Option<CanonicalFieldMappingRuleSet> {
    let matches: Vec<(&str, &Value)> = FIELD_MAPPING_RULE_KEYS
        .iter()
        .filter_map(|alias| {
            transformation_rules
                .get(*alias)
                .map(|value| (*alias, value))
        })
        .collect();

    match matches.as_slice() {
        [] => None,
        [(key, value)] => parse_field_mapping_rule(key, value, issues),
        multiple => {
            let keys = multiple
                .iter()
                .map(|(key, _)| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(", ");
            issues.push(format!(
                "Multiple field-mapping transformation rules are defined ({keys}); keep a single canonical rule"
            ));
            None
        }
    }
}

fn parse_field_mapping_rule(
    key: &str,
    value: &Value,
    issues: &mut Vec<String>,
) -> Option<CanonicalFieldMappingRuleSet> {
    let object = match value {
        Value::Object(object) => object,
        other => {
            issues.push(format!(
                "Transformation rule '{key}' for field-level compatibility must be an object with a 'mappings' array, got {}",
                value_kind(other)
            ));
            return None;
        }
    };

    let Some((mappings_field, mappings_value)) = MAPPINGS_FIELD_ALIASES
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        issues.push(format!(
            "Transformation rule '{key}' is missing a 'mappings' array"
        ));
        return None;
    };

    let mappings = match mappings_value {
        Value::Array(entries) if !entries.is_empty() => entries,
        Value::Array(_) => {
            issues.push(format!(
                "Transformation rule '{key}' must declare at least one field mapping entry"
            ));
            return None;
        }
        other => {
            issues.push(format!(
                "Transformation rule '{key}' field '{mappings_field}' must be an array, got {}",
                value_kind(other)
            ));
            return None;
        }
    };

    let mut parsed_mappings = Vec::new();
    let mut seen_targets = std::collections::HashSet::new();

    for (index, entry) in mappings.iter().enumerate() {
        let object = match entry {
            Value::Object(object) => object,
            other => {
                issues.push(format!(
                    "Transformation rule '{key}' mapping {} must be an object, got {}",
                    index,
                    value_kind(other)
                ));
                continue;
            }
        };

        let to = match extract_string_field(object, &TO_FIELD_ALIASES) {
            Ok(value) => value,
            Err(message) => {
                issues.push(format!(
                    "Transformation rule '{key}' mapping {} {message}",
                    index
                ));
                continue;
            }
        };

        let from = match extract_optional_string_field(object, &FROM_FIELD_ALIASES) {
            Ok(value) => value,
            Err(message) => {
                issues.push(format!(
                    "Transformation rule '{key}' mapping {} {message}",
                    index
                ));
                continue;
            }
        };
        let value = object.get("value").map(value_as_canonical_string);
        let strategy = match extract_optional_string_field(object, &STRATEGY_FIELD_ALIASES) {
            Ok(value) => value,
            Err(message) => {
                issues.push(format!(
                    "Transformation rule '{key}' mapping {} {message}",
                    index
                ));
                continue;
            }
        }
        .unwrap_or_else(|| {
            if value.is_some() {
                "constant".to_string()
            } else {
                "copy".to_string()
            }
        });

        if from.is_none() && value.is_none() {
            issues.push(format!(
                "Transformation rule '{key}' mapping {} must declare either 'from' or 'value'",
                index
            ));
            continue;
        }

        if from.is_some() && value.is_some() {
            issues.push(format!(
                "Transformation rule '{key}' mapping {} cannot declare both 'from' and 'value'",
                index
            ));
            continue;
        }

        if !seen_targets.insert(to.clone()) {
            issues.push(format!(
                "Transformation rule '{key}' declares multiple mappings for target '{}'",
                to
            ));
            continue;
        }

        parsed_mappings.push(FieldMappingRule {
            from,
            to,
            strategy,
            value,
        });
    }

    if parsed_mappings.is_empty() {
        return None;
    }

    Some(CanonicalFieldMappingRuleSet {
        key: key.to_string(),
        mappings: parsed_mappings,
    })
}

fn extract_string_field(object: &Map<String, Value>, aliases: &[&str]) -> Result<String, String> {
    let Some((field_name, value)) = aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Err(format!(
            "is missing a '{}' field",
            aliases.first().copied().unwrap_or("value")
        ));
    };

    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Ok(value.to_string()),
        None => Err(format!(
            "must declare a non-empty string for '{field_name}'"
        )),
    }
}

fn extract_optional_string_field(
    object: &Map<String, Value>,
    aliases: &[&str],
) -> Result<Option<String>, String> {
    let Some((field_name, value)) = aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Ok(None);
    };

    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Ok(Some(value.to_string())),
        None => Err(format!(
            "must declare a non-empty string for optional field '{field_name}'"
        )),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn value_as_canonical_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::sos_validation::validators::SchemaCompatibilityIssue;
    use serde_json::json;

    #[test]
    fn validates_canonical_unit_transform_rule() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "Imperial",
                "strategy": "linear_scale"
            }),
        )]));

        assert!(report.valid);
        let rule = report.unit_rule.expect("unit rule should parse");
        assert_eq!(rule.key, "unit_transform");
        assert_eq!(rule.from, "SI");
        assert_eq!(rule.to, "Imperial");
        assert_eq!(rule.strategy.as_deref(), Some("linear_scale"));
    }

    #[test]
    fn rejects_duplicate_unit_rule_aliases() {
        let report = validate_contract_transformation_rules(&HashMap::from([
            (
                "unit_transform".to_string(),
                json!({ "from": "SI", "to": "Imperial" }),
            ),
            (
                "unit_conversion".to_string(),
                json!({ "from": "SI", "to": "Imperial" }),
            ),
        ]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("Multiple unit transformation rules")));
        assert!(report.unit_rule.is_none());
    }

    #[test]
    fn rejects_rule_without_required_endpoints() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate_transform".to_string(),
            json!({ "from": "WGS84" }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("missing a 'to' field")));
        assert!(report.coordinate_rule.is_none());
    }

    #[test]
    fn accepts_provider_consumer_alias_fields() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate".to_string(),
            json!({
                "provider": "WGS84",
                "consumer": "ECI_J2000",
                "method": "helmert"
            }),
        )]));

        assert!(report.valid);
        let rule = report
            .coordinate_rule
            .expect("coordinate rule should parse");
        assert_eq!(rule.from, "WGS84");
        assert_eq!(rule.to, "ECI_J2000");
        assert_eq!(rule.strategy.as_deref(), Some("helmert"));
    }

    #[test]
    fn validates_field_mapping_rule() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "field_mapping".to_string(),
            json!({
                "mappings": [
                    { "from": "$.payload.rank", "to": "$.payload.priority" },
                    { "value": 1, "to": "$.payload.severity", "strategy": "constant" }
                ]
            }),
        )]));

        assert!(report.valid);
        let rule_set = report
            .field_mapping_rule
            .expect("field mapping rule should parse");
        assert_eq!(rule_set.mappings.len(), 2);
        assert_eq!(rule_set.mappings[0].from.as_deref(), Some("$.payload.rank"));
        assert_eq!(rule_set.mappings[0].to, "$.payload.priority");
        assert_eq!(rule_set.mappings[0].strategy, "copy");
        assert_eq!(rule_set.mappings[1].value.as_deref(), Some("1"));
        assert_eq!(rule_set.mappings[1].strategy, "constant");
    }

    #[test]
    fn rejects_field_mapping_without_source_or_value() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "field_mapping".to_string(),
            json!({
                "mappings": [
                    { "to": "$.payload.priority" }
                ]
            }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("must declare either 'from' or 'value'")));
    }

    #[test]
    fn rejects_duplicate_field_mapping_targets() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "field_mapping".to_string(),
            json!({
                "mappings": [
                    { "from": "$.payload.rank", "to": "$.payload.priority" },
                    { "value": 1, "to": "$.payload.priority" }
                ]
            }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("multiple mappings for target '$.payload.priority'")));
    }

    #[test]
    fn reports_transformability_when_field_mappings_cover_missing_required_fields() {
        let schema_report = SchemaCompatibilityReport {
            compatible: false,
            issues: vec![
                "Provider schema does not expose required consumer field '$.payload.priority'"
                    .to_string(),
            ],
            structured_issues: vec![SchemaCompatibilityIssue {
                kind: SchemaCompatibilityIssueKind::MissingRequiredField,
                path: "$.payload.priority".to_string(),
                message:
                    "Provider schema does not expose required consumer field '$.payload.priority'"
                        .to_string(),
            }],
        };
        let field_mapping_rule = CanonicalFieldMappingRuleSet {
            key: "field_mapping".to_string(),
            mappings: vec![FieldMappingRule {
                from: Some("$.payload.rank".to_string()),
                to: "$.payload.priority".to_string(),
                strategy: "copy".to_string(),
                value: None,
            }],
        };

        let report = evaluate_schema_transformability(&schema_report, Some(&field_mapping_rule));
        assert!(report.transformable);
        assert_eq!(report.covered_paths, vec!["$.payload.priority".to_string()]);
        assert!(report.uncovered_issues.is_empty());
    }
}
