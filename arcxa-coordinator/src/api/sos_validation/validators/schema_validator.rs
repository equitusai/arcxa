//! Schema validation and compatibility helpers for SoS validation.

use anyhow::{anyhow, Result};
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaCompatibilityReport {
    pub compatible: bool,
    pub issues: Vec<String>,
}

/// Validate a payload against a JSON schema and return user-facing errors.
pub fn validate_data_against_schema(schema: &Value, data: &Value) -> Result<Vec<String>> {
    let compiled = JSONSchema::compile(schema)
        .map_err(|error| anyhow!("Failed to compile JSON Schema: {}", error))?;
    let errors = match compiled.validate(data) {
        Ok(()) => Vec::new(),
        Err(iter) => iter.map(|error| error.to_string()).collect(),
    };

    Ok(errors)
}

/// Compare provider/consumer schemas pragmatically for phase-1 compatibility checks.
pub fn compare_interface_schemas(
    provider_schema: &Value,
    consumer_schema: &Value,
) -> Result<SchemaCompatibilityReport> {
    // Compile both schemas first so syntax issues surface deterministically.
    JSONSchema::compile(provider_schema)
        .map_err(|error| anyhow!("Failed to compile provider schema: {}", error))?;
    JSONSchema::compile(consumer_schema)
        .map_err(|error| anyhow!("Failed to compile consumer schema: {}", error))?;

    let mut issues = Vec::new();

    let provider_properties = extract_properties(provider_schema);
    let consumer_properties = extract_properties(consumer_schema);
    let consumer_required = extract_required(consumer_schema);

    for required_field in consumer_required {
        let Some(consumer_field_schema) = consumer_properties.get(required_field.as_str()) else {
            issues.push(format!(
                "Consumer schema marks '{}' as required but does not define it",
                required_field
            ));
            continue;
        };

        let Some(provider_field_schema) = provider_properties.get(required_field.as_str()) else {
            issues.push(format!(
                "Provider schema does not expose required consumer field '{}'",
                required_field
            ));
            continue;
        };

        if !schemas_are_type_compatible(provider_field_schema, consumer_field_schema) {
            issues.push(format!(
                "Field '{}' has incompatible types (provider: {}, consumer: {})",
                required_field,
                type_label(provider_field_schema),
                type_label(consumer_field_schema)
            ));
        }
    }

    Ok(SchemaCompatibilityReport {
        compatible: issues.is_empty(),
        issues,
    })
}

fn extract_properties(schema: &Value) -> HashMap<String, Value> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_required(schema: &Value) -> HashSet<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn extract_type_set(schema: &Value) -> HashSet<String> {
    match schema.get("type") {
        Some(Value::String(single)) => [single.clone()].into_iter().collect(),
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => HashSet::new(),
    }
}

fn schemas_are_type_compatible(provider: &Value, consumer: &Value) -> bool {
    let provider_types = extract_type_set(provider);
    let consumer_types = extract_type_set(consumer);

    if provider_types.is_empty() || consumer_types.is_empty() {
        return true;
    }

    if !provider_types.is_disjoint(&consumer_types) {
        return true;
    }

    provider_types.contains("integer") && consumer_types.contains("number")
}

fn type_label(schema: &Value) -> String {
    let types = extract_type_set(schema);
    if types.is_empty() {
        "unknown".to_string()
    } else {
        let mut values: Vec<_> = types.into_iter().collect();
        values.sort();
        values.join("|")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payload_validation_reports_errors() {
        let schema = json!({
            "type": "object",
            "required": ["sample_id"],
            "properties": {
                "sample_id": { "type": "string" },
                "score": { "type": "number" }
            }
        });
        let payload = json!({"score": "oops"});

        let errors = validate_data_against_schema(&schema, &payload).unwrap();
        assert!(!errors.is_empty());
    }

    #[test]
    fn schema_comparison_flags_missing_required_fields() {
        let provider = json!({
            "type": "object",
            "properties": {
                "sample_id": { "type": "string" }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["sample_id", "label"],
            "properties": {
                "sample_id": { "type": "string" },
                "label": { "type": "string" }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert_eq!(report.issues.len(), 1);
    }
}
