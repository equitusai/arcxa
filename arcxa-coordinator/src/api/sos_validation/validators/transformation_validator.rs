//! Contract transformation-rule validation for SoS compatibility checks.

use super::schema_validator::{SchemaCompatibilityIssueKind, SchemaCompatibilityReport};
use serde::{Deserialize, Serialize};
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
const SCALE_FIELD_ALIASES: [&str; 2] = ["scale", "multiplier"];
const OFFSET_FIELD_ALIASES: [&str; 2] = ["offset", "bias"];
const TOLERANCE_FIELD_ALIASES: [&str; 2] = ["tolerance", "max_error"];
const TRANSLATION_FIELD_ALIASES: [&str; 2] = ["translation_m", "translation"];
const ROTATION_FIELD_ALIASES: [&str; 2] = ["rotation_arcsec", "rotation"];
const SCALE_PPM_FIELD_ALIASES: [&str; 2] = ["scale_ppm", "ppm_scale"];
const COORDINATE_TOLERANCE_FIELD_ALIASES: [&str; 2] = ["tolerance_m", "max_error_m"];
const ORIGIN_FIELD_ALIASES: [&str; 2] = ["origin", "reference_origin"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTransformRule {
    pub key: String,
    pub from: String,
    pub to: String,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnitTransformSemantics {
    Identity,
    LinearScale {
        scale: f64,
        offset: f64,
        tolerance: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalUnitTransformRule {
    pub key: String,
    pub from: String,
    pub to: String,
    pub semantics: UnitTransformSemantics,
}

impl CanonicalUnitTransformRule {
    pub fn strategy_name(&self) -> &'static str {
        match self.semantics {
            UnitTransformSemantics::Identity => "identity",
            UnitTransformSemantics::LinearScale { .. } => "linear_scale",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoordinateTransformSemantics {
    Identity,
    Helmert {
        translation_m: [f64; 3],
        rotation_arcsec: [f64; 3],
        scale_ppm: f64,
        tolerance_m: Option<f64>,
    },
    LocalTangentPlane {
        origin_lat_deg: f64,
        origin_lon_deg: f64,
        origin_alt_m: f64,
        tolerance_m: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalCoordinateTransformRule {
    pub key: String,
    pub from: String,
    pub to: String,
    pub semantics: CoordinateTransformSemantics,
}

impl CanonicalCoordinateTransformRule {
    pub fn strategy_name(&self) -> &'static str {
        match self.semantics {
            CoordinateTransformSemantics::Identity => "identity",
            CoordinateTransformSemantics::Helmert { .. } => "helmert",
            CoordinateTransformSemantics::LocalTangentPlane { .. } => "local_tangent_plane",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransformCompatibilityMode {
    DirectAlignment,
    MetadataAbsent,
    BoundedTransform,
    UnboundedTransform,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeclaredErrorBudget {
    pub value: f64,
    pub label: String,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransformationRulesValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub unit_rule: Option<CanonicalUnitTransformRule>,
    pub coordinate_rule: Option<CanonicalCoordinateTransformRule>,
    pub field_mapping_rule: Option<CanonicalFieldMappingRuleSet>,
}

pub fn validate_contract_transformation_rules(
    transformation_rules: &HashMap<String, Value>,
) -> TransformationRulesValidation {
    let mut issues = Vec::new();
    let unit_rule = extract_unit_rule(transformation_rules, &mut issues);
    let coordinate_rule = extract_coordinate_rule(transformation_rules, &mut issues);
    let field_mapping_rule = extract_field_mapping_rule(transformation_rules, &mut issues);

    TransformationRulesValidation {
        valid: issues.is_empty(),
        issues,
        unit_rule,
        coordinate_rule,
        field_mapping_rule,
    }
}

fn extract_coordinate_rule(
    transformation_rules: &HashMap<String, Value>,
    issues: &mut Vec<String>,
) -> Option<CanonicalCoordinateTransformRule> {
    let matches: Vec<(&str, &Value)> = COORDINATE_RULE_KEYS
        .iter()
        .filter_map(|alias| {
            transformation_rules
                .get(*alias)
                .map(|value| (*alias, value))
        })
        .collect();

    match matches.as_slice() {
        [] => None,
        [(key, value)] => parse_coordinate_rule(key, value, issues),
        multiple => {
            let keys = multiple
                .iter()
                .map(|(key, _)| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(", ");
            issues.push(format!(
                "Multiple coordinate transformation rules are defined ({keys}); keep a single canonical rule"
            ));
            None
        }
    }
}

fn extract_unit_rule(
    transformation_rules: &HashMap<String, Value>,
    issues: &mut Vec<String>,
) -> Option<CanonicalUnitTransformRule> {
    let matches: Vec<(&str, &Value)> = UNIT_RULE_KEYS
        .iter()
        .filter_map(|alias| {
            transformation_rules
                .get(*alias)
                .map(|value| (*alias, value))
        })
        .collect();

    match matches.as_slice() {
        [] => None,
        [(key, value)] => parse_unit_rule(key, value, issues),
        multiple => {
            let keys = multiple
                .iter()
                .map(|(key, _)| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(", ");
            issues.push(format!(
                "Multiple unit transformation rules are defined ({keys}); keep a single canonical rule"
            ));
            None
        }
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

fn parse_unit_rule(
    key: &str,
    value: &Value,
    issues: &mut Vec<String>,
) -> Option<CanonicalUnitTransformRule> {
    let object = match value {
        Value::Object(object) => object,
        other => {
            issues.push(format!(
                "Transformation rule '{key}' for unit compatibility must be an object with non-empty 'from' and 'to' fields, got {}",
                value_kind(other)
            ));
            return None;
        }
    };

    let base_rule = parse_rule("unit", key, value, issues)?;
    let same_endpoint = base_rule.from.eq_ignore_ascii_case(&base_rule.to);
    let strategy = base_rule
        .strategy
        .as_deref()
        .map(normalize_strategy_name);

    let semantics = match strategy.as_deref() {
        None if same_endpoint => UnitTransformSemantics::Identity,
        None => {
            issues.push(format!(
                "Transformation rule '{key}' must declare a unit conversion strategy for {} -> {} (supported: 'identity', 'linear_scale')",
                base_rule.from, base_rule.to
            ));
            return None;
        }
        Some("identity") | Some("noop") => {
            if !same_endpoint {
                issues.push(format!(
                    "Transformation rule '{key}' cannot use identity semantics for differing unit systems ({} -> {})",
                    base_rule.from, base_rule.to
                ));
                return None;
            }
            UnitTransformSemantics::Identity
        }
        Some("linear_scale") | Some("affine") => {
            let scale = match extract_numeric_field(object, &SCALE_FIELD_ALIASES) {
                Ok(scale) => scale,
                Err(message) => {
                    issues.push(format!("Transformation rule '{key}' {message}"));
                    return None;
                }
            };
            if !scale.is_finite() || scale <= 0.0 {
                issues.push(format!(
                    "Transformation rule '{key}' must declare a positive finite scale for linear unit conversion"
                ));
                return None;
            }

            let offset = match extract_optional_numeric_field(object, &OFFSET_FIELD_ALIASES) {
                Ok(offset) => offset.unwrap_or(0.0),
                Err(message) => {
                    issues.push(format!("Transformation rule '{key}' {message}"));
                    return None;
                }
            };
            if !offset.is_finite() {
                issues.push(format!(
                    "Transformation rule '{key}' must declare a finite numeric offset for linear unit conversion"
                ));
                return None;
            }

            let tolerance = match extract_optional_numeric_field(object, &TOLERANCE_FIELD_ALIASES) {
                Ok(tolerance) => tolerance,
                Err(message) => {
                    issues.push(format!("Transformation rule '{key}' {message}"));
                    return None;
                }
            };
            if let Some(tolerance) = tolerance {
                if !tolerance.is_finite() || tolerance < 0.0 {
                    issues.push(format!(
                        "Transformation rule '{key}' must declare a non-negative finite tolerance"
                    ));
                    return None;
                }
            }

            if same_endpoint && ((scale - 1.0).abs() > f64::EPSILON || offset.abs() > f64::EPSILON)
            {
                issues.push(format!(
                    "Transformation rule '{key}' cannot change values when provider and consumer unit systems are both '{}'; expected scale=1 and offset=0",
                    base_rule.from
                ));
                return None;
            }

            UnitTransformSemantics::LinearScale {
                scale,
                offset,
                tolerance,
            }
        }
        Some(strategy) => {
            issues.push(format!(
                "Transformation rule '{key}' uses unsupported unit conversion strategy '{strategy}' (supported: 'identity', 'linear_scale')"
            ));
            return None;
        }
    };

    Some(CanonicalUnitTransformRule {
        key: base_rule.key,
        from: base_rule.from,
        to: base_rule.to,
        semantics,
    })
}

fn normalize_strategy_name(strategy: &str) -> String {
    strategy.trim().to_ascii_lowercase()
}

fn parse_coordinate_rule(
    key: &str,
    value: &Value,
    issues: &mut Vec<String>,
) -> Option<CanonicalCoordinateTransformRule> {
    let object = match value {
        Value::Object(object) => object,
        other => {
            issues.push(format!(
                "Transformation rule '{key}' for coordinate compatibility must be an object with non-empty 'from' and 'to' fields, got {}",
                value_kind(other)
            ));
            return None;
        }
    };

    let base_rule = parse_rule("coordinate", key, value, issues)?;
    let same_endpoint = base_rule.from.eq_ignore_ascii_case(&base_rule.to);
    let strategy = base_rule
        .strategy
        .as_deref()
        .map(normalize_strategy_name);

    let semantics = match strategy.as_deref() {
        None if same_endpoint => CoordinateTransformSemantics::Identity,
        None => {
            issues.push(format!(
                "Transformation rule '{key}' must declare a coordinate conversion strategy for {} -> {} (supported: 'identity', 'helmert', 'local_tangent_plane')",
                base_rule.from, base_rule.to
            ));
            return None;
        }
        Some("identity") | Some("noop") => {
            if !same_endpoint {
                issues.push(format!(
                    "Transformation rule '{key}' cannot use identity semantics for differing coordinate systems ({} -> {})",
                    base_rule.from, base_rule.to
                ));
                return None;
            }
            CoordinateTransformSemantics::Identity
        }
        Some("helmert") | Some("seven_parameter") => {
            let translation_m =
                match extract_fixed_length_numeric_array_field(object, &TRANSLATION_FIELD_ALIASES, 3)
                {
                    Ok(values) => values,
                    Err(message) => {
                        issues.push(format!("Transformation rule '{key}' {message}"));
                        return None;
                    }
                };
            let rotation_arcsec =
                match extract_fixed_length_numeric_array_field(object, &ROTATION_FIELD_ALIASES, 3) {
                    Ok(values) => values,
                    Err(message) => {
                        issues.push(format!("Transformation rule '{key}' {message}"));
                        return None;
                    }
                };
            let scale_ppm = match extract_optional_numeric_field(object, &SCALE_PPM_FIELD_ALIASES) {
                Ok(scale_ppm) => scale_ppm.unwrap_or(0.0),
                Err(message) => {
                    issues.push(format!("Transformation rule '{key}' {message}"));
                    return None;
                }
            };
            if !scale_ppm.is_finite() {
                issues.push(format!(
                    "Transformation rule '{key}' must declare a finite numeric scale_ppm for helmert conversion"
                ));
                return None;
            }
            let tolerance_m =
                match extract_optional_numeric_field(object, &COORDINATE_TOLERANCE_FIELD_ALIASES) {
                    Ok(tolerance_m) => tolerance_m,
                    Err(message) => {
                        issues.push(format!("Transformation rule '{key}' {message}"));
                        return None;
                    }
                };
            if let Some(tolerance_m) = tolerance_m {
                if !tolerance_m.is_finite() || tolerance_m < 0.0 {
                    issues.push(format!(
                        "Transformation rule '{key}' must declare a non-negative finite tolerance_m"
                    ));
                    return None;
                }
            }

            if same_endpoint
                && (translation_m.iter().any(|value| value.abs() > f64::EPSILON)
                    || rotation_arcsec.iter().any(|value| value.abs() > f64::EPSILON)
                    || scale_ppm.abs() > f64::EPSILON)
            {
                issues.push(format!(
                    "Transformation rule '{key}' cannot change coordinates when provider and consumer coordinate systems are both '{}'; expected zero translation, zero rotation, and scale_ppm=0",
                    base_rule.from
                ));
                return None;
            }

            CoordinateTransformSemantics::Helmert {
                translation_m,
                rotation_arcsec,
                scale_ppm,
                tolerance_m,
            }
        }
        Some("local_tangent_plane") | Some("enu") | Some("ned") => {
            if same_endpoint {
                issues.push(format!(
                    "Transformation rule '{key}' cannot use local tangent plane semantics when provider and consumer coordinate systems are both '{}'",
                    base_rule.from
                ));
                return None;
            }

            let (origin_lat_deg, origin_lon_deg, origin_alt_m) =
                match extract_coordinate_origin(object) {
                    Ok(origin) => origin,
                    Err(message) => {
                        issues.push(format!("Transformation rule '{key}' {message}"));
                        return None;
                    }
                };
            let tolerance_m =
                match extract_optional_numeric_field(object, &COORDINATE_TOLERANCE_FIELD_ALIASES) {
                    Ok(tolerance_m) => tolerance_m,
                    Err(message) => {
                        issues.push(format!("Transformation rule '{key}' {message}"));
                        return None;
                    }
                };
            if let Some(tolerance_m) = tolerance_m {
                if !tolerance_m.is_finite() || tolerance_m < 0.0 {
                    issues.push(format!(
                        "Transformation rule '{key}' must declare a non-negative finite tolerance_m"
                    ));
                    return None;
                }
            }

            CoordinateTransformSemantics::LocalTangentPlane {
                origin_lat_deg,
                origin_lon_deg,
                origin_alt_m,
                tolerance_m,
            }
        }
        Some(strategy) => {
            issues.push(format!(
                "Transformation rule '{key}' uses unsupported coordinate conversion strategy '{strategy}' (supported: 'identity', 'helmert', 'local_tangent_plane')"
            ));
            return None;
        }
    };

    Some(CanonicalCoordinateTransformRule {
        key: base_rule.key,
        from: base_rule.from,
        to: base_rule.to,
        semantics,
    })
}

fn extract_fixed_length_numeric_array_field(
    object: &Map<String, Value>,
    aliases: &[&str],
    expected_len: usize,
) -> Result<[f64; 3], String> {
    let Some((field_name, value)) = aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Err(format!(
            "is missing a '{}' field",
            aliases.first().copied().unwrap_or("value")
        ));
    };

    let values = value.as_array().ok_or_else(|| {
        format!("must declare an array for '{field_name}'")
    })?;
    if values.len() != expected_len {
        return Err(format!(
            "must declare exactly {expected_len} numeric values for '{field_name}'"
        ));
    }

    let mut parsed = [0.0; 3];
    for (index, entry) in values.iter().enumerate() {
        let number = entry.as_f64().ok_or_else(|| {
            format!("must declare numeric values for '{field_name}'")
        })?;
        if !number.is_finite() {
            return Err(format!(
                "must declare finite numeric values for '{field_name}'"
            ));
        }
        parsed[index] = number;
    }

    Ok(parsed)
}

fn extract_coordinate_origin(object: &Map<String, Value>) -> Result<(f64, f64, f64), String> {
    let Some((field_name, value)) = ORIGIN_FIELD_ALIASES
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Err(format!(
            "is missing an '{}' object",
            ORIGIN_FIELD_ALIASES.first().copied().unwrap_or("origin")
        ));
    };

    let origin = value.as_object().ok_or_else(|| {
        format!("must declare an object for '{field_name}'")
    })?;

    let origin_lat_deg = extract_numeric_field(origin, &["lat_deg", "latitude_deg"])?;
    let origin_lon_deg = extract_numeric_field(origin, &["lon_deg", "longitude_deg"])?;
    let origin_alt_m = extract_optional_numeric_field(origin, &["alt_m", "altitude_m"])?
        .unwrap_or(0.0);

    if !origin_lat_deg.is_finite()
        || !(-90.0..=90.0).contains(&origin_lat_deg)
    {
        return Err("must declare a finite origin latitude between -90 and 90 degrees".to_string());
    }
    if !origin_lon_deg.is_finite()
        || !(-180.0..=180.0).contains(&origin_lon_deg)
    {
        return Err(
            "must declare a finite origin longitude between -180 and 180 degrees".to_string(),
        );
    }
    if !origin_alt_m.is_finite() {
        return Err("must declare a finite origin altitude".to_string());
    }

    Ok((origin_lat_deg, origin_lon_deg, origin_alt_m))
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

fn extract_numeric_field(object: &Map<String, Value>, aliases: &[&str]) -> Result<f64, String> {
    let Some((field_name, value)) = aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Err(format!(
            "is missing a '{}' field",
            aliases.first().copied().unwrap_or("value")
        ));
    };

    value
        .as_f64()
        .ok_or_else(|| format!("must declare a numeric value for '{field_name}'"))
}

fn extract_optional_numeric_field(
    object: &Map<String, Value>,
    aliases: &[&str],
) -> Result<Option<f64>, String> {
    let Some((field_name, value)) = aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| (*alias, value)))
    else {
        return Ok(None);
    };

    value.as_f64().map(Some).ok_or_else(|| {
        format!("must declare a numeric value for optional field '{field_name}'")
    })
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
    fn validates_linear_scale_unit_transform_rule() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "Imperial",
                "strategy": "linear_scale",
                "scale": 3.28084,
                "offset": 0.0,
                "tolerance": 0.01
            }),
        )]));

        assert!(report.valid);
        let rule = report.unit_rule.expect("unit rule should parse");
        assert_eq!(rule.key, "unit_transform");
        assert_eq!(rule.from, "SI");
        assert_eq!(rule.to, "Imperial");
        match rule.semantics {
            UnitTransformSemantics::LinearScale {
                scale,
                offset,
                tolerance,
            } => {
                assert!((scale - 3.28084).abs() < f64::EPSILON);
                assert_eq!(offset, 0.0);
                assert_eq!(tolerance, Some(0.01));
            }
            other => panic!("expected linear scale semantics, got {other:?}"),
        }
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
    fn rejects_unit_transform_without_strategy_for_mismatched_systems() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({ "from": "SI", "to": "Imperial" }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("must declare a unit conversion strategy")));
        assert!(report.unit_rule.is_none());
    }

    #[test]
    fn rejects_identity_unit_transform_for_mismatched_systems() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "Imperial",
                "strategy": "identity"
            }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("cannot use identity semantics")));
        assert!(report.unit_rule.is_none());
    }

    #[test]
    fn rejects_same_endpoint_linear_scale_that_changes_values() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "SI",
                "strategy": "linear_scale",
                "scale": 2.0
            }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("expected scale=1 and offset=0")));
        assert!(report.unit_rule.is_none());
    }

    #[test]
    fn accepts_identity_unit_transform_for_same_system() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "SI",
                "strategy": "identity"
            }),
        )]));

        assert!(report.valid);
        let rule = report.unit_rule.expect("unit rule should parse");
        assert!(matches!(rule.semantics, UnitTransformSemantics::Identity));
    }

    #[test]
    fn accepts_provider_consumer_alias_fields() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate".to_string(),
            json!({
                "provider": "WGS84",
                "consumer": "ECI_J2000",
                "method": "helmert",
                "translation_m": [1.0, 2.0, 3.0],
                "rotation_arcsec": [0.1, 0.2, 0.3],
                "scale_ppm": 0.0,
                "tolerance_m": 5.0
            }),
        )]));

        assert!(report.valid);
        let rule = report
            .coordinate_rule
            .expect("coordinate rule should parse");
        assert_eq!(rule.from, "WGS84");
        assert_eq!(rule.to, "ECI_J2000");
        match rule.semantics {
            CoordinateTransformSemantics::Helmert {
                translation_m,
                rotation_arcsec,
                scale_ppm,
                tolerance_m,
            } => {
                assert_eq!(translation_m, [1.0, 2.0, 3.0]);
                assert_eq!(rotation_arcsec, [0.1, 0.2, 0.3]);
                assert_eq!(scale_ppm, 0.0);
                assert_eq!(tolerance_m, Some(5.0));
            }
            other => panic!("expected helmert semantics, got {other:?}"),
        }
    }

    #[test]
    fn rejects_coordinate_transform_without_strategy_for_mismatched_systems() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate_transform".to_string(),
            json!({ "from": "WGS84", "to": "ECI_J2000" }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("must declare a coordinate conversion strategy")));
        assert!(report.coordinate_rule.is_none());
    }

    #[test]
    fn rejects_coordinate_transform_with_missing_helmert_parameters() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate_transform".to_string(),
            json!({
                "from": "WGS84",
                "to": "ECI_J2000",
                "strategy": "helmert",
                "translation_m": [1.0, 2.0, 3.0]
            }),
        )]));

        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("rotation_arcsec")));
        assert!(report.coordinate_rule.is_none());
    }

    #[test]
    fn validates_local_tangent_plane_coordinate_rule() {
        let report = validate_contract_transformation_rules(&HashMap::from([(
            "coordinate_transform".to_string(),
            json!({
                "from": "WGS84",
                "to": "ENU",
                "strategy": "local_tangent_plane",
                "origin": {
                    "lat_deg": 38.8895,
                    "lon_deg": -77.0353,
                    "alt_m": 15.0
                },
                "tolerance_m": 0.5
            }),
        )]));

        assert!(report.valid);
        let rule = report
            .coordinate_rule
            .expect("coordinate rule should parse");
        match rule.semantics {
            CoordinateTransformSemantics::LocalTangentPlane {
                origin_lat_deg,
                origin_lon_deg,
                origin_alt_m,
                tolerance_m,
            } => {
                assert_eq!(origin_lat_deg, 38.8895);
                assert_eq!(origin_lon_deg, -77.0353);
                assert_eq!(origin_alt_m, 15.0);
                assert_eq!(tolerance_m, Some(0.5));
            }
            other => panic!("expected local tangent plane semantics, got {other:?}"),
        }
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
