use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use super::extract_json_path_value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapEccAdapterField {
    pub name: String,
    pub abap_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapEccAdapterCapabilities {
    pub adapter_version: Option<String>,
    pub system_id: Option<String>,
    pub client: Option<String>,
    pub object_name: Option<String>,
    pub key_fields: Vec<String>,
    pub fields: Vec<SapEccAdapterField>,
    pub supports_record_projection: bool,
    pub supports_rowset_projection: bool,
    pub supports_key_lookup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SapEccProjectionFields {
    pub requested_fields: Vec<String>,
    pub declared_fields: Vec<String>,
    pub missing_fields: Vec<String>,
}

pub fn discover_sap_ecc_adapter_capabilities(
    capabilities_payload: &Value,
) -> Result<SapEccAdapterCapabilities> {
    let object = capabilities_payload
        .as_object()
        .ok_or_else(|| anyhow!("SAP ECC adapter capabilities must be a JSON object"))?;
    let capabilities = object
        .get("capabilities")
        .and_then(Value::as_object)
        .unwrap_or(object);

    let adapter_version = capabilities
        .get("adapter_version")
        .or_else(|| capabilities.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let system_id = capabilities
        .get("system_id")
        .or_else(|| capabilities.get("sid"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let client = capabilities
        .get("client")
        .and_then(Value::as_str)
        .map(str::to_string);
    let object_name = capabilities
        .get("object_name")
        .or_else(|| capabilities.get("business_object"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let key_fields = capabilities
        .get("key_fields")
        .or_else(|| capabilities.get("keys"))
        .and_then(Value::as_array)
        .map(|items| {
            let mut fields = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            fields
        })
        .unwrap_or_default();
    let fields = capabilities
        .get("fields")
        .and_then(Value::as_array)
        .map(|items| {
            let mut fields = items
                .iter()
                .filter_map(|item| {
                    let object = item.as_object()?;
                    Some(SapEccAdapterField {
                        name: object.get("name")?.as_str()?.to_string(),
                        abap_type: object
                            .get("abap_type")
                            .or_else(|| object.get("type"))
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                        nullable: object
                            .get("nullable")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    })
                })
                .collect::<Vec<_>>();
            fields.sort_by(|left, right| left.name.cmp(&right.name));
            fields
        })
        .unwrap_or_default();

    Ok(SapEccAdapterCapabilities {
        adapter_version,
        system_id,
        client,
        object_name,
        key_fields,
        supports_record_projection: capabilities
            .get("supports_record_projection")
            .and_then(Value::as_bool)
            .unwrap_or(!fields.is_empty()),
        supports_rowset_projection: capabilities
            .get("supports_rowset_projection")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        supports_key_lookup: capabilities
            .get("supports_key_lookup")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        fields,
    })
}

pub fn derive_sap_ecc_projection_fields(
    capabilities: Option<&SapEccAdapterCapabilities>,
    target_field_path: &str,
    expected_value: Option<&Value>,
    connection: &HashMap<String, String>,
) -> SapEccProjectionFields {
    let mut requested_fields = Vec::new();

    if let Some(field_list) = connection.get("ecc_requested_fields_json") {
        if let Ok(fields) = serde_json::from_str::<Vec<String>>(field_list) {
            requested_fields.extend(
                fields
                    .into_iter()
                    .map(|field| field.trim().to_string())
                    .filter(|field| !field.is_empty()),
            );
        }
    } else if let Some(field_list) = connection.get("ecc_requested_fields") {
        requested_fields.extend(
            field_list
                .split(',')
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty()),
        );
    }

    if let Some(field) = extract_terminal_projection_field(target_field_path) {
        requested_fields.push(field);
    }

    if let Some(fields) = expected_projection_fields(expected_value) {
        requested_fields.extend(fields);
    }

    requested_fields.sort();
    requested_fields.dedup();

    let declared_fields = capabilities
        .map(|caps| caps.fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    let missing_fields = if declared_fields.is_empty() {
        Vec::new()
    } else {
        requested_fields
            .iter()
            .filter(|field| !declared_fields.contains(*field))
            .cloned()
            .collect::<Vec<_>>()
    };

    SapEccProjectionFields {
        requested_fields,
        declared_fields,
        missing_fields,
    }
}

pub fn normalize_sap_ecc_adapter_payload(payload: Value) -> Value {
    if let Some(object) = payload.as_object() {
        if let Some(actual_value) = object.get("actual_value") {
            return actual_value.clone();
        }
        if let Some(record) = object.get("record") {
            return record.clone();
        }
        if let Some(rows) = object.get("rows") {
            return rows.clone();
        }
    }
    payload
}

pub fn resolve_sap_ecc_adapter_value(payload: Value, preferred_paths: &[&str]) -> Result<Value> {
    let normalized = normalize_sap_ecc_adapter_payload(payload);
    for path in preferred_paths {
        if let Some(value) = extract_json_path_value(&normalized, path) {
            return Ok(value);
        }
    }

    match normalized {
        Value::Array(items) if items.len() == 1 => Ok(items.into_iter().next().unwrap_or(Value::Null)),
        value => Ok(value),
    }
}

pub fn extract_sap_ecc_adapter_next_path(payload: &Value) -> Option<String> {
    payload
        .get("pagination")
        .and_then(Value::as_object)
        .and_then(|pagination| {
            pagination
                .get("next_path")
                .or_else(|| pagination.get("nextPagePath"))
                .or_else(|| pagination.get("next"))
        })
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("next_path")
                .or_else(|| payload.get("nextPagePath"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

pub fn merge_sap_ecc_adapter_page_payloads(current: Value, next_page: Value) -> Value {
    let current = normalize_sap_ecc_adapter_payload(current);
    let next = normalize_sap_ecc_adapter_payload(next_page);

    match (current, next) {
        (Value::Array(mut left), Value::Array(right)) => {
            left.extend(right);
            Value::Array(left)
        }
        (Value::Array(mut left), Value::Object(right)) => {
            left.push(Value::Object(right));
            Value::Array(left)
        }
        (Value::Object(left), Value::Array(mut right)) => {
            let mut merged = Vec::with_capacity(right.len() + 1);
            merged.push(Value::Object(left));
            merged.append(&mut right);
            Value::Array(merged)
        }
        (Value::Object(left), Value::Object(right)) => Value::Array(vec![
            Value::Object(left),
            Value::Object(right),
        ]),
        (_, next) => next,
    }
}

pub fn field_types_by_name(fields: &[SapEccAdapterField]) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|field| (field.name.clone(), field.abap_type.clone()))
        .collect()
}

fn extract_terminal_projection_field(target_field_path: &str) -> Option<String> {
    let trimmed = target_field_path.trim();
    if !trimmed.starts_with("$.") {
        return None;
    }
    trimmed
        .trim_start_matches("$.")
        .split('.')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .map(str::to_string)
        .filter(|field| !matches!(field.as_str(), "projection" | "value" | "rows" | "record"))
}

fn expected_projection_fields(expected_value: Option<&Value>) -> Option<Vec<String>> {
    match expected_value {
        Some(Value::Object(object)) => {
            let mut fields = object.keys().cloned().collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            Some(fields)
        }
        Some(Value::Array(items)) => items.first().and_then(|first| match first {
            Value::Object(object) => {
                let mut fields = object.keys().cloned().collect::<Vec<_>>();
                fields.sort();
                fields.dedup();
                Some(fields)
            }
            _ => None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn discovers_sap_ecc_adapter_capabilities() {
        let payload = json!({
            "capabilities": {
                "adapter_version": "0.1.0",
                "system_id": "PRD",
                "client": "100",
                "object_name": "VBAK",
                "key_fields": ["VBELN"],
                "supports_record_projection": true,
                "supports_rowset_projection": true,
                "supports_key_lookup": true,
                "fields": [
                    {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
                    {"name": "NETWR", "abap_type": "CURR", "nullable": true}
                ]
            }
        });

        let capabilities = discover_sap_ecc_adapter_capabilities(&payload).unwrap();
        assert_eq!(capabilities.object_name, Some("VBAK".to_string()));
        assert_eq!(capabilities.key_fields, vec!["VBELN".to_string()]);
        assert_eq!(capabilities.fields.len(), 2);
    }

    #[test]
    fn derives_projection_and_detects_missing_fields() {
        let capabilities = SapEccAdapterCapabilities {
            adapter_version: None,
            system_id: None,
            client: None,
            object_name: Some("VBAK".to_string()),
            key_fields: vec!["VBELN".to_string()],
            fields: vec![
                SapEccAdapterField {
                    name: "VBELN".to_string(),
                    abap_type: "CHAR".to_string(),
                    nullable: false,
                },
                SapEccAdapterField {
                    name: "NETWR".to_string(),
                    abap_type: "CURR".to_string(),
                    nullable: true,
                },
            ],
            supports_record_projection: true,
            supports_rowset_projection: true,
            supports_key_lookup: true,
        };

        let projection = derive_sap_ecc_projection_fields(
            Some(&capabilities),
            "$.projection",
            Some(&json!({"VBELN": "500000001", "MISSING": "x"})),
            &HashMap::new(),
        );

        assert_eq!(
            projection.requested_fields,
            vec!["MISSING".to_string(), "VBELN".to_string()]
        );
        assert_eq!(projection.missing_fields, vec!["MISSING".to_string()]);
    }

    #[test]
    fn resolves_record_and_rowset_payloads() {
        let record = resolve_sap_ecc_adapter_value(
            json!({"record": {"VBELN": "500000001", "NETWR": "100.00"}}),
            &["$.NETWR"],
        )
        .unwrap();
        assert_eq!(record, json!("100.00"));

        let rows = resolve_sap_ecc_adapter_value(
            json!({"rows": [{"VBELN": "500000001"}, {"VBELN": "500000002"}]}),
            &["$.VBELN"],
        )
        .unwrap();
        assert_eq!(rows, json!(["500000001", "500000002"]));
    }

    #[test]
    fn extracts_next_path_and_merges_pages() {
        let payload = json!({
            "rows": [{"VBELN": "500000001"}],
            "pagination": {"next_path": "/adapter/v1/records/VBAK?page=2"}
        });
        assert_eq!(
            extract_sap_ecc_adapter_next_path(&payload),
            Some("/adapter/v1/records/VBAK?page=2".to_string())
        );

        let merged = merge_sap_ecc_adapter_page_payloads(
            json!({"rows": [{"VBELN": "500000001"}]}),
            json!({"rows": [{"VBELN": "500000002"}]}),
        );
        assert_eq!(
            merged,
            json!([
                {"VBELN": "500000001"},
                {"VBELN": "500000002"}
            ])
        );
    }
}
