use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use super::extract_json_path_value;
use super::{SapEccBackendAuthMode, SapEccSessionMode};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapEccRfcBapiField {
    pub name: String,
    pub abap_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapEccRfcBapiProfile {
    BapiRecordLookup,
    FunctionModuleExport,
    TableReadRowset,
    QueryBridge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapEccRfcBapiCapabilities {
    pub profile: SapEccRfcBapiProfile,
    pub bridge_version: Option<String>,
    pub system_id: Option<String>,
    pub client: Option<String>,
    pub function_module: Option<String>,
    pub bapi_name: Option<String>,
    pub export_structure: Option<String>,
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
    pub cursor_parameter_name: Option<String>,
    pub next_cursor_path: Option<String>,
    pub fields: Vec<SapEccRfcBapiField>,
    pub supports_record_projection: bool,
    pub supports_rowset_projection: bool,
    pub supports_key_lookup: bool,
    pub supports_cursor_pagination: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SapEccRfcBapiProjectionFields {
    pub requested_fields: Vec<String>,
    pub declared_fields: Vec<String>,
    pub missing_fields: Vec<String>,
}

pub fn discover_sap_ecc_rfc_bapi_capabilities(
    capabilities_payload: &Value,
) -> Result<SapEccRfcBapiCapabilities> {
    let object = capabilities_payload
        .as_object()
        .ok_or_else(|| anyhow!("SAP ECC RFC/BAPI bridge capabilities must be a JSON object"))?;
    let capabilities = object
        .get("capabilities")
        .and_then(Value::as_object)
        .unwrap_or(object);

    let bridge_version = capabilities
        .get("bridge_version")
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
    let function_module = capabilities
        .get("function_module")
        .or_else(|| capabilities.get("function"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let bapi_name = capabilities
        .get("bapi_name")
        .or_else(|| capabilities.get("bapi"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let export_structure = capabilities
        .get("export_structure")
        .or_else(|| capabilities.get("export_parameter"))
        .or_else(|| capabilities.get("table_parameter"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let profile = capabilities
        .get("profile")
        .or_else(|| capabilities.get("adapter_profile"))
        .and_then(Value::as_str)
        .map(parse_rfc_profile)
        .transpose()?
        .unwrap_or_else(|| {
            infer_rfc_profile(
                bapi_name.as_deref(),
                function_module.as_deref(),
                export_structure.as_deref(),
            )
        });
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
                    Some(SapEccRfcBapiField {
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
    let cursor_parameter_name = capabilities
        .get("cursor_parameter_name")
        .or_else(|| capabilities.get("next_cursor_parameter"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let next_cursor_path = capabilities
        .get("next_cursor_path")
        .or_else(|| capabilities.get("cursor_response_path"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(SapEccRfcBapiCapabilities {
        profile,
        bridge_version,
        system_id,
        client,
        function_module,
        bapi_name,
        export_structure,
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
        cursor_parameter_name,
        next_cursor_path,
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
        supports_cursor_pagination: capabilities
            .get("supports_cursor_pagination")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        fields,
    })
}

fn extract_supported_auth_modes(value: &Value) -> Result<Vec<SapEccBackendAuthMode>> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("SAP ECC RFC/BAPI supported_auth_modes must be an array"))?;
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
        .ok_or_else(|| anyhow!("SAP ECC RFC/BAPI supported_session_modes must be an array"))?;
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
            "unsupported SAP ECC RFC/BAPI backend auth mode '{other}'"
        )),
    }
}

fn parse_session_mode(value: &str) -> Result<SapEccSessionMode> {
    match value {
        "stateless" => Ok(SapEccSessionMode::Stateless),
        "stateful" => Ok(SapEccSessionMode::Stateful),
        "cached" | "pooled" => Ok(SapEccSessionMode::Cached),
        other => Err(anyhow!(
            "unsupported SAP ECC RFC/BAPI session mode '{other}'"
        )),
    }
}

pub fn derive_sap_ecc_rfc_bapi_projection_fields(
    capabilities: Option<&SapEccRfcBapiCapabilities>,
    target_field_path: &str,
    expected_value: Option<&Value>,
    connection: &HashMap<String, String>,
) -> SapEccRfcBapiProjectionFields {
    let mut requested_fields = Vec::new();

    if let Some(field_list) = connection.get("ecc_rfc_requested_fields_json") {
        if let Ok(fields) = serde_json::from_str::<Vec<String>>(field_list) {
            requested_fields.extend(
                fields
                    .into_iter()
                    .map(|field| field.trim().to_string())
                    .filter(|field| !field.is_empty()),
            );
        }
    } else if let Some(field_list) = connection.get("ecc_rfc_requested_fields") {
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

    SapEccRfcBapiProjectionFields {
        requested_fields,
        declared_fields,
        missing_fields,
    }
}

pub fn normalize_sap_ecc_rfc_bapi_payload(payload: Value) -> Value {
    if let Some(object) = payload.as_object() {
        for key in [
            "actual_value",
            "result",
            "export_data",
            "table_rows",
            "rows",
            "record",
        ] {
            if let Some(value) = object.get(key) {
                return value.clone();
            }
        }
    }
    payload
}

pub fn resolve_sap_ecc_rfc_bapi_value(payload: Value, preferred_paths: &[&str]) -> Result<Value> {
    let normalized = normalize_sap_ecc_rfc_bapi_payload(payload);
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

pub fn extract_sap_ecc_rfc_bapi_next_cursor_from_path(
    payload: &Value,
    path: Option<&str>,
) -> Option<String> {
    if let Some(path) = path {
        if let Some(Value::String(cursor)) = extract_json_path_value(payload, path) {
            return Some(cursor);
        }
    }
    extract_sap_ecc_rfc_bapi_next_cursor(payload)
}

pub fn extract_sap_ecc_rfc_bapi_next_cursor(payload: &Value) -> Option<String> {
    payload
        .get("pagination")
        .and_then(Value::as_object)
        .and_then(|pagination| {
            pagination
                .get("next_cursor")
                .or_else(|| pagination.get("nextCursor"))
                .or_else(|| pagination.get("cursor"))
        })
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("next_cursor")
                .or_else(|| payload.get("nextCursor"))
                .or_else(|| payload.get("cursor").and_then(|cursor| cursor.get("next")))
                .or_else(|| payload.get("meta").and_then(|meta| meta.get("next_cursor")))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

pub fn merge_sap_ecc_rfc_bapi_page_payloads(current: Value, next_page: Value) -> Value {
    let current = normalize_sap_ecc_rfc_bapi_payload(current);
    let next = normalize_sap_ecc_rfc_bapi_payload(next_page);

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

pub fn rfc_field_types_by_name(fields: &[SapEccRfcBapiField]) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|field| (field.name.clone(), field.abap_type.clone()))
        .collect()
}

fn parse_rfc_profile(value: &str) -> Result<SapEccRfcBapiProfile> {
    match value {
        "bapi_record_lookup" | "bapi_lookup" | "bapi" => Ok(SapEccRfcBapiProfile::BapiRecordLookup),
        "function_module_export" | "function_module" | "rfc_function" => {
            Ok(SapEccRfcBapiProfile::FunctionModuleExport)
        }
        "table_read_rowset" | "table_read" | "rowset" => Ok(SapEccRfcBapiProfile::TableReadRowset),
        "query_bridge" | "generic_bridge" => Ok(SapEccRfcBapiProfile::QueryBridge),
        other => Err(anyhow!("unsupported SAP ECC RFC/BAPI profile '{}'", other)),
    }
}

fn infer_rfc_profile(
    bapi_name: Option<&str>,
    function_module: Option<&str>,
    export_structure: Option<&str>,
) -> SapEccRfcBapiProfile {
    if bapi_name.is_some() {
        SapEccRfcBapiProfile::BapiRecordLookup
    } else if function_module.is_some() && export_structure.is_some() {
        SapEccRfcBapiProfile::FunctionModuleExport
    } else if export_structure.is_some() {
        SapEccRfcBapiProfile::TableReadRowset
    } else {
        SapEccRfcBapiProfile::QueryBridge
    }
}

fn extract_required_parameters(value: &Value) -> Result<Vec<String>> {
    match value {
        Value::Array(items) => {
            let mut params = Vec::new();
            for item in items {
                match item {
                    Value::String(value) => params.push(value.trim().to_string()),
                    Value::Object(object) => {
                        let required = object
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        if required {
                            let name = object
                                .get("name")
                                .or_else(|| object.get("parameter"))
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    anyhow!("RFC parameter objects must include a 'name'")
                                })?;
                            params.push(name.trim().to_string());
                        }
                    }
                    _ => {
                        return Err(anyhow!(
                            "RFC required_parameters must contain strings or parameter objects"
                        ))
                    }
                }
            }
            params.retain(|param| !param.is_empty());
            params.sort();
            params.dedup();
            Ok(params)
        }
        _ => Err(anyhow!(
            "RFC required_parameters must be an array of strings or parameter objects"
        )),
    }
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
        .filter(|field| {
            !matches!(
                field.as_str(),
                "projection" | "value" | "rows" | "record" | "result"
            )
        })
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
    fn discovers_sap_ecc_rfc_bapi_capabilities() {
        let payload = json!({
            "capabilities": {
                "profile": "bapi_record_lookup",
                "bridge_version": "0.2.0",
                "system_id": "PRD",
                "client": "100",
                "function_module": "RFC_READ_TABLE",
                "bapi_name": "BAPI_SALESORDER_GETDETAIL",
                "export_structure": "ORDER_ITEMS_OUT",
                "key_fields": ["VBELN"],
                "required_parameters": [
                    "VBELN",
                    {"name": "ITEMS", "required": false},
                    {"name": "DETAIL", "required": true}
                ],
                "supported_auth_modes": ["destination"],
                "supported_session_modes": ["stateful"],
                "health_path": "/bridge/v1/health",
                "session_id_path": "$.session.id",
                "session_id_parameter_name": "sessionId",
                "close_session_path": "/bridge/v1/session/close",
                "close_session_method": "post",
                "requires_explicit_session_close": true,
                "session_ttl_seconds": 600,
                "max_page_size": 500,
                "page_size_parameter_name": "pageSize",
                "language_parameter_name": "language",
                "cursor_parameter_name": "cursorToken",
                "next_cursor_path": "$.pagination.token",
                "supports_record_projection": true,
                "supports_rowset_projection": true,
                "supports_key_lookup": true,
                "supports_cursor_pagination": true,
                "fields": [
                    {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
                    {"name": "NETWR", "abap_type": "CURR", "nullable": true}
                ]
            }
        });

        let capabilities = discover_sap_ecc_rfc_bapi_capabilities(&payload).unwrap();
        assert_eq!(capabilities.profile, SapEccRfcBapiProfile::BapiRecordLookup);
        assert_eq!(
            capabilities.function_module,
            Some("RFC_READ_TABLE".to_string())
        );
        assert_eq!(
            capabilities.bapi_name,
            Some("BAPI_SALESORDER_GETDETAIL".to_string())
        );
        assert_eq!(
            capabilities.required_parameters,
            vec!["DETAIL".to_string(), "VBELN".to_string()]
        );
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
            Some("/bridge/v1/session/close")
        );
        assert_eq!(capabilities.close_session_method.as_deref(), Some("POST"));
        assert!(capabilities.requires_explicit_session_close);
        assert_eq!(capabilities.session_ttl_seconds, Some(600));
        assert_eq!(
            capabilities.cursor_parameter_name,
            Some("cursorToken".to_string())
        );
        assert_eq!(
            capabilities.next_cursor_path,
            Some("$.pagination.token".to_string())
        );
        assert_eq!(capabilities.fields.len(), 2);
    }

    #[test]
    fn derives_projection_and_detects_missing_fields() {
        let capabilities = SapEccRfcBapiCapabilities {
            profile: SapEccRfcBapiProfile::TableReadRowset,
            bridge_version: None,
            system_id: None,
            client: None,
            function_module: Some("RFC_READ_TABLE".to_string()),
            bapi_name: None,
            export_structure: Some("DATA".to_string()),
            key_fields: vec!["VBELN".to_string()],
            required_parameters: vec!["VBELN".to_string()],
            supported_auth_modes: vec![SapEccBackendAuthMode::Destination],
            supported_session_modes: vec![SapEccSessionMode::Stateful],
            health_path: Some("/bridge/v1/health".to_string()),
            session_id_path: Some("$.session.id".to_string()),
            session_id_parameter_name: Some("sessionId".to_string()),
            close_session_path: Some("/bridge/v1/session/close".to_string()),
            close_session_method: Some("POST".to_string()),
            requires_explicit_session_close: true,
            session_ttl_seconds: Some(600),
            max_page_size: Some(500),
            page_size_parameter_name: Some("pageSize".to_string()),
            language_parameter_name: Some("language".to_string()),
            cursor_parameter_name: Some("cursor".to_string()),
            next_cursor_path: Some("$.pagination.next_cursor".to_string()),
            fields: vec![
                SapEccRfcBapiField {
                    name: "VBELN".to_string(),
                    abap_type: "CHAR".to_string(),
                    nullable: false,
                },
                SapEccRfcBapiField {
                    name: "NETWR".to_string(),
                    abap_type: "CURR".to_string(),
                    nullable: true,
                },
            ],
            supports_record_projection: true,
            supports_rowset_projection: true,
            supports_key_lookup: true,
            supports_cursor_pagination: true,
        };

        let projection = derive_sap_ecc_rfc_bapi_projection_fields(
            Some(&capabilities),
            "$.projection",
            Some(&json!({"VBELN": "500000001", "MISSING": "x"})),
            &HashMap::new(),
        );

        assert_eq!(projection.missing_fields, vec!["MISSING".to_string()]);
    }

    #[test]
    fn resolves_payloads_and_cursor() {
        let record = resolve_sap_ecc_rfc_bapi_value(
            json!({"result": {"VBELN": "500000001", "NETWR": "100.00"}}),
            &["$.NETWR"],
        )
        .unwrap();
        assert_eq!(record, json!("100.00"));

        assert_eq!(
            extract_sap_ecc_rfc_bapi_next_cursor(
                &json!({"pagination": {"next_cursor": "cursor-2"}})
            ),
            Some("cursor-2".to_string())
        );
        assert_eq!(
            extract_sap_ecc_rfc_bapi_next_cursor_from_path(
                &json!({"pagination": {"token": "cursor-3"}}),
                Some("$.pagination.token")
            ),
            Some("cursor-3".to_string())
        );
    }

    #[test]
    fn merges_pages() {
        let merged = merge_sap_ecc_rfc_bapi_page_payloads(
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
