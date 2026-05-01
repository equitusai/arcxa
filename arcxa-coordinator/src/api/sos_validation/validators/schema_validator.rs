//! Schema validation and compatibility helpers for SoS validation.

use anyhow::{anyhow, Result};
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchemaCompatibilityIssueKind {
    ConsumerRejectsAll,
    ProviderUnconstrained,
    TypeMismatch,
    EnumBroadening,
    MissingRequiredField,
    RequiredFieldNotGuaranteed,
    AdditionalPropertyConflict,
    AdditionalPropertiesUnconstrained,
    ArrayItemMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaCompatibilityIssue {
    pub kind: SchemaCompatibilityIssueKind,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaCompatibilityReport {
    pub compatible: bool,
    pub issues: Vec<String>,
    pub structured_issues: Vec<SchemaCompatibilityIssue>,
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

/// Compare provider and consumer schemas recursively.
///
/// The compatibility direction is intentionally provider -> consumer:
/// values that satisfy the provider schema should also satisfy the consumer schema.
///
/// This is still a pragmatic compatibility model rather than a full JSON Schema
/// subsumption engine, but it now reasons recursively about nested objects,
/// arrays, enum restrictions, nullability, requiredness, and selected
/// `additionalProperties` behavior.
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
    let mut structured_issues = Vec::new();
    compare_schema_nodes(
        provider_schema,
        consumer_schema,
        "$",
        &mut issues,
        &mut structured_issues,
    );

    Ok(SchemaCompatibilityReport {
        compatible: issues.is_empty(),
        issues,
        structured_issues,
    })
}

fn compare_schema_nodes(
    provider: &Value,
    consumer: &Value,
    path: &str,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    match (provider, consumer) {
        (Value::Bool(false), Value::Bool(false)) => return,
        (Value::Bool(false), _) => return,
        (_, Value::Bool(true)) => return,
        (_, Value::Bool(false)) => {
            push_issue(
                issues,
                structured_issues,
                SchemaCompatibilityIssueKind::ConsumerRejectsAll,
                path,
                "Consumer schema at '{}' rejects all instances, so provider output cannot satisfy it",
                &[path],
            );
            return;
        }
        (Value::Bool(true), _) => {
            push_issue(
                issues,
                structured_issues,
                SchemaCompatibilityIssueKind::ProviderUnconstrained,
                path,
                "Provider schema at '{}' is unconstrained while consumer expects {}",
                &[path, &type_label(consumer)],
            );
            return;
        }
        _ => {}
    }

    if !schemas_are_type_compatible(provider, consumer) {
        push_issue(
            issues,
            structured_issues,
            SchemaCompatibilityIssueKind::TypeMismatch,
            path,
            "Schema at '{}' has incompatible types (provider: {}, consumer: {})",
            &[path, &type_label(provider), &type_label(consumer)],
        );
        return;
    }

    compare_enum_constraints(provider, consumer, path, issues, structured_issues);

    if schema_may_be_object(provider) && schema_may_be_object(consumer) {
        compare_object_schemas(provider, consumer, path, issues, structured_issues);
    }

    if schema_may_be_array(provider) && schema_may_be_array(consumer) {
        compare_array_schemas(provider, consumer, path, issues, structured_issues);
    }
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

fn extract_additional_properties(schema: &Value) -> Option<Value> {
    schema.get("additionalProperties").cloned()
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
    let mut types = match schema.get("type") {
        Some(Value::String(single)) => [single.clone()].into_iter().collect(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => HashSet::new(),
    };

    if types.is_empty() {
        if schema.get("properties").is_some() || schema.get("required").is_some() {
            types.insert("object".to_string());
        }
        if schema.get("items").is_some() {
            types.insert("array".to_string());
        }
    }

    types
}

fn extract_enum_set(schema: &Value) -> Option<HashSet<String>> {
    schema.get("enum").and_then(Value::as_array).map(|values| {
        values
            .iter()
            .map(enum_value_key)
            .collect::<HashSet<String>>()
    })
}

fn schemas_are_type_compatible(provider: &Value, consumer: &Value) -> bool {
    let provider_types = extract_type_set(provider);
    let consumer_types = extract_type_set(consumer);

    if provider_types.is_empty() || consumer_types.is_empty() {
        return true;
    }

    provider_types.iter().all(|provider_type| {
        consumer_types
            .iter()
            .any(|consumer_type| type_is_accepted(provider_type, consumer_type))
    })
}

fn type_is_accepted(provider_type: &str, consumer_type: &str) -> bool {
    provider_type == consumer_type || (provider_type == "integer" && consumer_type == "number")
}

fn schema_may_be_object(schema: &Value) -> bool {
    let types = extract_type_set(schema);
    types.is_empty() || types.contains("object")
}

fn schema_may_be_array(schema: &Value) -> bool {
    let types = extract_type_set(schema);
    types.is_empty() || types.contains("array")
}

fn compare_enum_constraints(
    provider: &Value,
    consumer: &Value,
    path: &str,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    let Some(consumer_enum) = extract_enum_set(consumer) else {
        return;
    };
    let Some(provider_enum) = extract_enum_set(provider) else {
        push_issue(
            issues,
            structured_issues,
            SchemaCompatibilityIssueKind::EnumBroadening,
            path,
            "Schema at '{}' does not constrain values to the consumer enum set",
            &[path],
        );
        return;
    };

    let mut incompatible_values = provider_enum
        .difference(&consumer_enum)
        .cloned()
        .collect::<Vec<_>>();
    incompatible_values.sort();
    if !incompatible_values.is_empty() {
        push_issue(
            issues,
            structured_issues,
            SchemaCompatibilityIssueKind::EnumBroadening,
            path,
            "Schema at '{}' allows enum values not accepted by the consumer: {}",
            &[path, &incompatible_values.join(", ")],
        );
    }
}

fn compare_object_schemas(
    provider: &Value,
    consumer: &Value,
    path: &str,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    let provider_properties = extract_properties(provider);
    let consumer_properties = extract_properties(consumer);
    let provider_required = extract_required(provider);
    let consumer_required = extract_required(consumer);

    let mut consumer_keys = consumer_properties.keys().cloned().collect::<Vec<_>>();
    consumer_keys.sort();

    for key in consumer_keys {
        let child_path = property_path(path, &key);
        let Some(consumer_property_schema) = consumer_properties.get(&key) else {
            continue;
        };

        if consumer_required.contains(&key) {
            if !provider_properties.contains_key(&key) {
                push_issue(
                    issues,
                    structured_issues,
                    SchemaCompatibilityIssueKind::MissingRequiredField,
                    &child_path,
                    "Provider schema does not expose required consumer field '{}'",
                    &[&child_path],
                );
                continue;
            }
            if !provider_required.contains(&key) {
                push_issue(
                    issues,
                    structured_issues,
                    SchemaCompatibilityIssueKind::RequiredFieldNotGuaranteed,
                    &child_path,
                    "Provider schema does not guarantee required consumer field '{}'",
                    &[&child_path],
                );
            }
        }

        if let Some(provider_property_schema) = provider_properties.get(&key) {
            compare_schema_nodes(
                provider_property_schema,
                consumer_property_schema,
                &child_path,
                issues,
                structured_issues,
            );
        }
    }

    compare_additional_properties(
        provider,
        consumer,
        path,
        &provider_properties,
        &consumer_properties,
        issues,
        structured_issues,
    );
}

fn compare_additional_properties(
    provider: &Value,
    consumer: &Value,
    path: &str,
    provider_properties: &HashMap<String, Value>,
    consumer_properties: &HashMap<String, Value>,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    let consumer_additional = extract_additional_properties(consumer);
    let provider_additional = extract_additional_properties(provider);

    let mut provider_only_keys = provider_properties
        .keys()
        .filter(|key| !consumer_properties.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    provider_only_keys.sort();

    match consumer_additional.as_ref() {
        Some(Value::Bool(false)) => {
            for key in provider_only_keys {
                let property = property_path(path, &key);
                push_issue(
                    issues,
                    structured_issues,
                    SchemaCompatibilityIssueKind::AdditionalPropertyConflict,
                    &property,
                    "Provider schema exposes additional property '{}' that the consumer forbids",
                    &[&property],
                );
            }

            if !matches!(provider_additional.as_ref(), Some(Value::Bool(false))) {
                push_issue(
                    issues,
                    structured_issues,
                    SchemaCompatibilityIssueKind::AdditionalPropertiesUnconstrained,
                    path,
                    "Provider schema at '{}' allows undeclared additional properties while the consumer forbids them",
                    &[path],
                );
            }
        }
        Some(Value::Object(_)) => {
            let consumer_schema = consumer_additional
                .as_ref()
                .expect("consumer additional-properties branch should be available");

            for key in &provider_only_keys {
                if let Some(provider_property_schema) = provider_properties.get(key) {
                    compare_schema_nodes(
                        provider_property_schema,
                        consumer_schema,
                        &property_path(path, key),
                        issues,
                        structured_issues,
                    );
                }
            }

            match provider_additional.as_ref() {
                Some(Value::Bool(false)) => {}
                Some(Value::Object(_)) => compare_schema_nodes(
                    provider_additional
                        .as_ref()
                        .expect("provider additional-properties branch should be available"),
                    consumer_schema,
                    &format!("{path}.*"),
                    issues,
                    structured_issues,
                ),
                _ => push_issue(
                    issues,
                    structured_issues,
                    SchemaCompatibilityIssueKind::AdditionalPropertiesUnconstrained,
                    path,
                    "Provider schema at '{}' allows unconstrained additional properties while the consumer requires a specific additional-properties schema",
                    &[path],
                ),
            }
        }
        _ => {}
    }
}

fn compare_array_schemas(
    provider: &Value,
    consumer: &Value,
    path: &str,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    let provider_items = provider.get("items");
    let consumer_items = consumer.get("items");

    match (provider_items, consumer_items) {
        (_, None) => {}
        (None, Some(_)) => push_issue(
            issues,
            structured_issues,
            SchemaCompatibilityIssueKind::ArrayItemMismatch,
            path,
            "Array schema at '{}' does not constrain items while the consumer expects item compatibility",
            &[path],
        ),
        (Some(provider_items), Some(consumer_items)) => match (provider_items, consumer_items) {
            (Value::Object(_), Value::Object(_)) => {
                compare_schema_nodes(
                    provider_items,
                    consumer_items,
                    &array_item_path(path),
                    issues,
                    structured_issues,
                );
            }
            (Value::Array(provider_items), Value::Array(consumer_items)) => {
                compare_tuple_items(
                    provider_items,
                    consumer_items,
                    path,
                    issues,
                    structured_issues,
                );
            }
            (Value::Object(_), Value::Array(consumer_items)) => {
                for (index, consumer_item) in consumer_items.iter().enumerate() {
                    compare_schema_nodes(
                        provider_items,
                        consumer_item,
                        &tuple_item_path(path, index),
                        issues,
                        structured_issues,
                    );
                }
            }
            (Value::Array(provider_items), Value::Object(_)) => {
                for (index, provider_item) in provider_items.iter().enumerate() {
                    compare_schema_nodes(
                        provider_item,
                        consumer_items,
                        &tuple_item_path(path, index),
                        issues,
                        structured_issues,
                    );
                }
            }
            _ => {}
        },
    }
}

fn compare_tuple_items(
    provider_items: &[Value],
    consumer_items: &[Value],
    path: &str,
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
) {
    for (index, consumer_item) in consumer_items.iter().enumerate() {
        let Some(provider_item) = provider_items.get(index) else {
            push_issue(
                issues,
                structured_issues,
                SchemaCompatibilityIssueKind::ArrayItemMismatch,
                path,
                "Array schema at '{}' does not define tuple item {} required by the consumer",
                &[path, &index.to_string()],
            );
            continue;
        };
        compare_schema_nodes(
            provider_item,
            consumer_item,
            &tuple_item_path(path, index),
            issues,
            structured_issues,
        );
    }
}

fn push_issue(
    issues: &mut Vec<String>,
    structured_issues: &mut Vec<SchemaCompatibilityIssue>,
    kind: SchemaCompatibilityIssueKind,
    path: &str,
    template: &str,
    values: &[&str],
) {
    let mut message = template.to_string();
    for value in values {
        message = message.replacen("{}", value, 1);
    }
    issues.push(message.clone());
    structured_issues.push(SchemaCompatibilityIssue {
        kind,
        path: path.to_string(),
        message,
    });
}

fn property_path(parent: &str, property: &str) -> String {
    if parent == "$" {
        format!("$.{property}")
    } else {
        format!("{parent}.{property}")
    }
}

fn array_item_path(parent: &str) -> String {
    format!("{parent}[]")
}

fn tuple_item_path(parent: &str, index: usize) -> String {
    format!("{parent}[{index}]")
}

fn enum_value_key(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(string) => format!("\"{string}\""),
        _ => value.to_string(),
    }
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
        assert_eq!(report.issues.len(), 2);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("$.sample_id")));
        assert!(report.issues.iter().any(|issue| issue.contains("$.label")));
        assert!(report.structured_issues.iter().any(|issue| {
            issue.kind == SchemaCompatibilityIssueKind::RequiredFieldNotGuaranteed
                && issue.path == "$.sample_id"
        }));
        assert!(report.structured_issues.iter().any(|issue| {
            issue.kind == SchemaCompatibilityIssueKind::MissingRequiredField
                && issue.path == "$.label"
        }));
    }

    #[test]
    fn schema_comparison_recurses_into_nested_objects() {
        let provider = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "status": { "type": "string" }
                    }
                }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status", "priority"],
                    "properties": {
                        "status": { "type": "string" },
                        "priority": { "type": "integer" }
                    }
                }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("$.payload.priority")));
    }

    #[test]
    fn schema_comparison_flags_requiredness_weakening() {
        let provider = json!({
            "type": "object",
            "properties": {
                "sample_id": { "type": "string" }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["sample_id"],
            "properties": {
                "sample_id": { "type": "string" }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report.issues.iter().any(
            |issue| issue.contains("does not guarantee required consumer field '$.sample_id'")
        ));
    }

    #[test]
    fn schema_comparison_flags_nullable_provider_against_non_nullable_consumer() {
        let provider = json!({
            "type": "object",
            "required": ["sample_id"],
            "properties": {
                "sample_id": { "type": ["string", "null"] }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["sample_id"],
            "properties": {
                "sample_id": { "type": "string" }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("$.sample_id") && issue.contains("incompatible types")));
    }

    #[test]
    fn schema_comparison_flags_enum_broadening() {
        let provider = json!({
            "type": "object",
            "required": ["status"],
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["ok", "warning", "error"]
                }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["status"],
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["ok", "warning"]
                }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("$.status") && issue.contains("\"error\"")));
    }

    #[test]
    fn schema_comparison_flags_additional_properties_conflict() {
        let provider = json!({
            "type": "object",
            "required": ["sample_id", "extra"],
            "properties": {
                "sample_id": { "type": "string" },
                "extra": { "type": "string" }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["sample_id"],
            "additionalProperties": false,
            "properties": {
                "sample_id": { "type": "string" }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report.issues.iter().any(|issue| issue.contains("$.extra")));
    }

    #[test]
    fn schema_comparison_recurses_into_array_items() {
        let provider = json!({
            "type": "object",
            "required": ["events"],
            "properties": {
                "events": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["code"],
                        "properties": {
                            "code": { "type": "integer" }
                        }
                    }
                }
            }
        });
        let consumer = json!({
            "type": "object",
            "required": ["events"],
            "properties": {
                "events": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["code"],
                        "properties": {
                            "code": { "type": "string" }
                        }
                    }
                }
            }
        });

        let report = compare_interface_schemas(&provider, &consumer).unwrap();
        assert!(!report.compatible);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("$.events[].code")));
    }
}
