use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use super::extract_json_path_value;
use super::{SapEccBackendAuthMode, SapEccSessionMode};

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
    pub required_parameters: Vec<String>,
    pub supported_auth_modes: Vec<SapEccBackendAuthMode>,
    pub supported_session_modes: Vec<SapEccSessionMode>,
    pub health_path: Option<String>,
    pub session_id_path: Option<String>,
    pub session_id_parameter_name: Option<String>,
    pub close_session_path: Option<String>,
    pub close_session_method: Option<String>,
    pub requires_explicit_session_close: bool,
    pub session_ttl_seconds: Option<u64>,
    pub max_page_size: Option<usize>,
    pub page_size_parameter_name: Option<String>,
    pub language_parameter_name: Option<String>,
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
    let required_parameters = capabilities
        .get("required_parameters")
        .or_else(|| capabilities.get("request_parameters"))
        .or_else(|| capabilities.get("parameters"))
        .map(extract_required_parameters)
        .transpose()?
        .unwrap_or_default();
    let supported_auth_modes = capabilities
        .get("supported_auth_modes")
        .or_else(|| capabilities.get("backend_auth_modes"))
        .or_else(|| capabilities.get("auth_modes"))
        .map(extract_supported_auth_modes)
        .transpose()?
        .unwrap_or_default();
    let supported_session_modes = capabilities
        .get("supported_session_modes")
        .or_else(|| capabilities.get("session_modes"))
        .map(extract_supported_session_modes)
        .transpose()?
        .unwrap_or_default();
    let health_path = capabilities
        .get("health_path")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_id_path = capabilities
        .get("session_id_path")
        .or_else(|| capabilities.get("session_path"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_id_parameter_name = capabilities
        .get("session_id_parameter_name")
        .or_else(|| capabilities.get("session_parameter_name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let close_session_path = capabilities
        .get("close_session_path")
        .and_then(Value::as_str)
        .map(str::to_string);
    let close_session_method = capabilities
        .get("close_session_method")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_uppercase());
    let requires_explicit_session_close = capabilities
        .get("requires_explicit_session_close")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let session_ttl_seconds = capabilities
        .get("session_ttl_seconds")
        .and_then(Value::as_u64);
    let max_page_size = capabilities
        .get("max_page_size")
        .or_else(|| capabilities.get("page_size_limit"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0);
    let page_size_parameter_name = capabilities
        .get("page_size_parameter_name")
        .or_else(|| capabilities.get("limit_parameter_name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let language_parameter_name = capabilities
        .get("language_parameter_name")
        .or_else(|| capabilities.get("locale_parameter_name"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(SapEccAdapterCapabilities {
        adapter_version,
        system_id,
        client,
        object_name,
        key_fields,
        required_parameters,
        supported_auth_modes,
        supported_session_modes,
        health_path,
        session_id_path,
        session_id_parameter_name,
        close_session_path,
        close_session_method,
        requires_explicit_session_close,
        session_ttl_seconds,
        max_page_size,
        page_size_parameter_name,
        language_parameter_name,
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

fn extract_required_parameters(value: &Value) -> Result<Vec<String>> {
    let mut parameters = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(name) if !name.trim().is_empty() => {
                        parameters.push(name.trim().to_string());
                    }
                    Value::Object(object) => {
                        let required = object
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        if required {
                            if let Some(name) = object.get("name").and_then(Value::as_str) {
                                if !name.trim().is_empty() {
                                    parameters.push(name.trim().to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {
            return Err(anyhow!(
                "SAP ECC adapter required_parameters must be an array"
            ))
        }
    }
    parameters.sort();
    parameters.dedup();
    Ok(parameters)
}

fn extract_supported_auth_modes(value: &Value) -> Result<Vec<SapEccBackendAuthMode>> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("SAP ECC adapter supported_auth_modes must be an array"))?;
    let mut modes = items
        .iter()
        .filter_map(Value::as_str)
        .map(parse_backend_auth_mode)
        .collect::<Result<Vec<_>>>()?;
    modes.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    modes.dedup();
    Ok(modes)
}

fn extract_supported_session_modes(value: &Value) -> Result<Vec<SapEccSessionMode>> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("SAP ECC adapter supported_session_modes must be an array"))?;
    let mut modes = items
        .iter()
        .filter_map(Value::as_str)
        .map(parse_session_mode)
        .collect::<Result<Vec<_>>>()?;
    modes.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    modes.dedup();
    Ok(modes)
}

fn parse_backend_auth_mode(value: &str) -> Result<SapEccBackendAuthMode> {
    match value {
        "user_password" | "username_password" | "basic" => Ok(SapEccBackendAuthMode::UserPassword),
        "snc" => Ok(SapEccBackendAuthMode::Snc),
        "sso2" | "sap_logon_ticket" => Ok(SapEccBackendAuthMode::Sso2),
        "x509" | "certificate" => Ok(SapEccBackendAuthMode::X509),
        "destination" | "destination_service" => Ok(SapEccBackendAuthMode::Destination),
        other => Err(anyhow!(
            "unsupported SAP ECC adapter backend auth mode '{other}'"
        )),
    }
}

fn parse_session_mode(value: &str) -> Result<SapEccSessionMode> {
    match value {
        "stateless" => Ok(SapEccSessionMode::Stateless),
        "stateful" => Ok(SapEccSessionMode::Stateful),
        "cached" | "pooled" => Ok(SapEccSessionMode::Cached),
        other => Err(anyhow!(
            "unsupported SAP ECC adapter session mode '{other}'"
        )),
    }
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
        .map(|caps| {
            caps.fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<_>>()
        })
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
        Value::Array(items) if items.len() == 1 => {
            Ok(items.into_iter().next().unwrap_or(Value::Null))
        }
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
        (Value::Object(left), Value::Object(right)) => {
            Value::Array(vec![Value::Object(left), Value::Object(right)])
        }
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
                "required_parameters": ["VBELN"],
                "supported_auth_modes": ["destination"],
                "supported_session_modes": ["stateful"],
                "health_path": "/adapter/v1/health",
                "session_id_path": "$.session.id",
                "session_id_parameter_name": "sessionId",
                "close_session_path": "/adapter/v1/session/close",
                "close_session_method": "post",
                "requires_explicit_session_close": true,
                "session_ttl_seconds": 900,
                "max_page_size": 500,
                "page_size_parameter_name": "pageSize",
                "language_parameter_name": "language",
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
        assert_eq!(capabilities.required_parameters, vec!["VBELN".to_string()]);
        assert_eq!(capabilities.max_page_size, Some(500));
        assert_eq!(
            capabilities.session_id_path.as_deref(),
            Some("$.session.id")
        );
        assert_eq!(
            capabilities.session_id_parameter_name.as_deref(),
            Some("sessionId")
        );
        assert_eq!(
            capabilities.close_session_path.as_deref(),
            Some("/adapter/v1/session/close")
        );
        assert_eq!(capabilities.close_session_method.as_deref(), Some("POST"));
        assert!(capabilities.requires_explicit_session_close);
        assert_eq!(capabilities.session_ttl_seconds, Some(900));
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
            required_parameters: vec!["VBELN".to_string()],
            supported_auth_modes: vec![SapEccBackendAuthMode::Destination],
            supported_session_modes: vec![SapEccSessionMode::Stateful],
            health_path: Some("/adapter/v1/health".to_string()),
            session_id_path: Some("$.session.id".to_string()),
            session_id_parameter_name: Some("sessionId".to_string()),
            close_session_path: Some("/adapter/v1/session/close".to_string()),
            close_session_method: Some("POST".to_string()),
            requires_explicit_session_close: true,
            session_ttl_seconds: Some(900),
            max_page_size: Some(500),
            page_size_parameter_name: Some("pageSize".to_string()),
            language_parameter_name: Some("language".to_string()),
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
