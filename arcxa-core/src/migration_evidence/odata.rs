use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapS4ODataVersion {
    V2,
    V4,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapS4ODataProperty {
    pub name: String,
    pub edm_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapS4ODataCapabilities {
    pub service_root_path: String,
    pub metadata_path: String,
    pub version: SapS4ODataVersion,
    pub entity_set: Option<String>,
    pub entity_type: Option<String>,
    pub key_fields: Vec<String>,
    pub properties: Vec<SapS4ODataProperty>,
    pub supports_record_projection: bool,
    pub supports_rowset_projection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SapS4ODataProjectionFields {
    pub requested_fields: Vec<String>,
    pub select_fields: Vec<String>,
    pub metadata_fields: Vec<String>,
    pub missing_fields: Vec<String>,
}

pub fn infer_sap_s4_odata_service_root_path(request_path: &str) -> String {
    let path = request_path
        .split('?')
        .next()
        .unwrap_or(request_path)
        .trim_end_matches('/');
    if path.is_empty() {
        return "/".to_string();
    }

    let mut segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.last().copied() == Some("$metadata") {
        segments.pop();
    } else if !segments.is_empty() {
        segments.pop();
    }

    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

pub fn infer_sap_s4_odata_metadata_path(request_path: &str) -> String {
    let service_root = infer_sap_s4_odata_service_root_path(request_path);
    if service_root == "/" {
        "/$metadata".to_string()
    } else {
        format!("{service_root}/$metadata")
    }
}

pub fn discover_sap_s4_odata_capabilities(
    metadata_document: &str,
    request_path: &str,
) -> Result<SapS4ODataCapabilities> {
    let entity_set_from_path = request_path
        .split('?')
        .next()
        .unwrap_or(request_path)
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.is_empty() && *segment != "$metadata")
        .map(|segment| segment.split('(').next().unwrap_or(segment).to_string());

    let version = detect_odata_version(metadata_document);
    let mut reader = Reader::from_str(metadata_document);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_schema_namespace: Option<String> = None;
    let mut current_entity_type: Option<String> = None;
    let mut in_key_block = false;
    let mut entity_sets = HashMap::<String, String>::new();
    let mut entity_keys = HashMap::<String, Vec<String>>::new();
    let mut entity_properties = HashMap::<String, Vec<SapS4ODataProperty>>::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "Schema" => {
                        current_schema_namespace = attribute_value(&event, "Namespace");
                    }
                    "EntityType" => {
                        if let (Some(namespace), Some(name)) =
                            (current_schema_namespace.clone(), attribute_value(&event, "Name"))
                        {
                            let full_name = format!("{namespace}.{name}");
                            entity_keys.entry(full_name.clone()).or_default();
                            entity_properties.entry(full_name.clone()).or_default();
                            current_entity_type = Some(full_name);
                        }
                    }
                    "Key" => {
                        in_key_block = true;
                    }
                    "PropertyRef" => {
                        if in_key_block {
                            if let (Some(entity_type), Some(name)) =
                                (current_entity_type.as_ref(), attribute_value(&event, "Name"))
                            {
                                entity_keys
                                    .entry(entity_type.clone())
                                    .or_default()
                                    .push(name);
                            }
                        }
                    }
                    "Property" => {
                        if let Some(entity_type) = current_entity_type.as_ref() {
                            if let (Some(name), Some(edm_type)) = (
                                attribute_value(&event, "Name"),
                                attribute_value(&event, "Type"),
                            ) {
                                let nullable = attribute_value(&event, "Nullable")
                                    .map(|flag| flag != "false")
                                    .unwrap_or(true);
                                entity_properties
                                    .entry(entity_type.clone())
                                    .or_default()
                                    .push(SapS4ODataProperty {
                                        name,
                                        edm_type,
                                        nullable,
                                    });
                            }
                        }
                    }
                    "EntitySet" => {
                        if let (Some(name), Some(entity_type)) = (
                            attribute_value(&event, "Name"),
                            attribute_value(&event, "EntityType"),
                        ) {
                            entity_sets.insert(name, entity_type);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "PropertyRef" => {
                        if in_key_block {
                            if let (Some(entity_type), Some(name)) =
                                (current_entity_type.as_ref(), attribute_value(&event, "Name"))
                            {
                                entity_keys
                                    .entry(entity_type.clone())
                                    .or_default()
                                    .push(name);
                            }
                        }
                    }
                    "Property" => {
                        if let Some(entity_type) = current_entity_type.as_ref() {
                            if let (Some(name), Some(edm_type)) = (
                                attribute_value(&event, "Name"),
                                attribute_value(&event, "Type"),
                            ) {
                                let nullable = attribute_value(&event, "Nullable")
                                    .map(|flag| flag != "false")
                                    .unwrap_or(true);
                                entity_properties
                                    .entry(entity_type.clone())
                                    .or_default()
                                    .push(SapS4ODataProperty {
                                        name,
                                        edm_type,
                                        nullable,
                                    });
                            }
                        }
                    }
                    "EntitySet" => {
                        if let (Some(name), Some(entity_type)) = (
                            attribute_value(&event, "Name"),
                            attribute_value(&event, "EntityType"),
                        ) {
                            entity_sets.insert(name, entity_type);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "EntityType" => current_entity_type = None,
                    "Key" => in_key_block = false,
                    "Schema" => current_schema_namespace = None,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(anyhow!("failed to parse OData metadata: {error}")),
            _ => {}
        }
        buf.clear();
    }

    let entity_set = entity_set_from_path
        .or_else(|| entity_sets.keys().next().cloned());
    let entity_type = entity_set
        .as_ref()
        .and_then(|name| entity_sets.get(name))
        .cloned();
    let key_fields = entity_type
        .as_ref()
        .and_then(|name| entity_keys.get(name))
        .cloned()
        .unwrap_or_default();
    let properties = entity_type
        .as_ref()
        .and_then(|name| entity_properties.get(name))
        .cloned()
        .unwrap_or_default();

    Ok(SapS4ODataCapabilities {
        service_root_path: infer_sap_s4_odata_service_root_path(request_path),
        metadata_path: infer_sap_s4_odata_metadata_path(request_path),
        version,
        entity_set,
        entity_type,
        supports_record_projection: !properties.is_empty(),
        supports_rowset_projection: true,
        key_fields,
        properties,
    })
}

pub fn derive_sap_s4_odata_projection_fields(
    capabilities: Option<&SapS4ODataCapabilities>,
    request_path: &str,
    target_field_path: &str,
    expected_value: Option<&Value>,
    connection: &HashMap<String, String>,
) -> SapS4ODataProjectionFields {
    let mut requested_fields = Vec::new();
    let mut select_fields = parse_sap_s4_odata_select_fields(request_path);
    if select_fields.is_empty() {
        select_fields = parse_override_select_fields(connection);
    }
    requested_fields.extend(select_fields.iter().cloned());

    if let Some(field) = extract_terminal_projection_field(target_field_path) {
        requested_fields.push(field);
    }

    if let Some(expected_fields) = expected_projection_fields(expected_value) {
        requested_fields.extend(expected_fields);
    }

    requested_fields.sort();
    requested_fields.dedup();

    let metadata_fields = capabilities
        .map(|caps| {
            let mut fields = caps
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            fields
        })
        .unwrap_or_default();
    let missing_fields = if metadata_fields.is_empty() {
        Vec::new()
    } else {
        requested_fields
            .iter()
            .filter(|field| !metadata_fields.contains(*field))
            .cloned()
            .collect::<Vec<_>>()
    };

    SapS4ODataProjectionFields {
        requested_fields,
        select_fields,
        metadata_fields,
        missing_fields,
    }
}

pub fn extract_sap_s4_odata_next_link(payload: &Value) -> Option<String> {
    let object = payload.as_object()?;
    if let Some(next_link) = object
        .get("@odata.nextLink")
        .or_else(|| object.get("@odata.nextlink"))
        .or_else(|| object.get("__next"))
        .and_then(Value::as_str)
    {
        return Some(next_link.to_string());
    }
    object.get("d").and_then(extract_sap_s4_odata_next_link)
}

pub fn merge_sap_s4_odata_page_payloads(current: Value, next_page: Value) -> Value {
    let current = normalize_sap_s4_odata_payload(current);
    let next = normalize_sap_s4_odata_payload(next_page);

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

pub fn normalize_sap_s4_odata_payload(payload: Value) -> Value {
    if let Some(object) = payload.as_object() {
        if let Some(d) = object.get("d") {
            return normalize_sap_s4_odata_payload(d.clone());
        }
        if let Some(results) = object.get("results").and_then(Value::as_array) {
            return Value::Array(results.clone());
        }
        if let Some(value) = object.get("value") {
            return value.clone();
        }
    }
    payload
}

pub fn resolve_sap_s4_odata_value(payload: Value, preferred_paths: &[&str]) -> Result<Value> {
    let normalized = normalize_sap_s4_odata_payload(payload);

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

pub fn extract_json_path_value(value: &Value, path: &str) -> Option<Value> {
    let tokens = parse_json_path(path).ok()?;
    let mut current = value.clone();
    for token in tokens {
        current = match token {
            PathToken::Field(field) => match current {
                Value::Object(object) => object.get(&field).cloned()?,
                Value::Array(items) => {
                    let mut projected = Vec::new();
                    for item in items {
                        if let Some(projected_value) = item
                            .as_object()
                            .and_then(|object| object.get(&field))
                            .cloned()
                        {
                            projected.push(projected_value);
                        } else {
                            return None;
                        }
                    }
                    Value::Array(projected)
                }
                _ => return None,
            },
            PathToken::Index(index) => match current {
                Value::Array(items) => items.get(index).cloned()?,
                _ => return None,
            },
        };
    }

    match current {
        Value::Array(items) if items.len() == 1 => items.into_iter().next(),
        value => Some(value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathToken {
    Field(String),
    Index(usize),
}

fn parse_json_path(path: &str) -> Result<Vec<PathToken>> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "$" {
        return Ok(Vec::new());
    }
    let without_root = trimmed
        .strip_prefix("$.")
        .or_else(|| trimmed.strip_prefix('$'))
        .ok_or_else(|| anyhow!("JSON path must start with '$'"))?;

    let chars = without_root.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
            }
            '[' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(anyhow!("unterminated array index in path"));
                }
                let index = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .parse::<usize>()
                    .map_err(|_| anyhow!("array index must be numeric"))?;
                tokens.push(PathToken::Index(index));
                i += 1;
            }
            _ => {
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                    i += 1;
                }
                let field = chars[start..i].iter().collect::<String>();
                let field = field.trim();
                if field.is_empty() {
                    return Err(anyhow!("empty field segment in path"));
                }
                tokens.push(PathToken::Field(field.to_string()));
            }
        }
    }

    Ok(tokens)
}

fn local_name(name: &[u8]) -> String {
    let raw = String::from_utf8_lossy(name);
    raw.rsplit(':').next().unwrap_or(raw.as_ref()).to_string()
}

fn attribute_value(event: &quick_xml::events::BytesStart<'_>, attribute_name: &str) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find_map(|attribute| {
            let key = local_name(attribute.key.as_ref());
            if key == attribute_name {
                Some(String::from_utf8_lossy(attribute.value.as_ref()).to_string())
            } else {
                None
            }
        })
}

fn detect_odata_version(metadata_document: &str) -> SapS4ODataVersion {
    if metadata_document.contains("docs.oasis-open.org/odata/ns/edm")
        || metadata_document.contains("docs.oasis-open.org/odata/ns/edmx")
    {
        SapS4ODataVersion::V4
    } else if metadata_document.contains("schemas.microsoft.com/ado/2008/09/edm")
        || metadata_document.contains("schemas.microsoft.com/ado/2006/04/edm")
    {
        SapS4ODataVersion::V2
    } else {
        SapS4ODataVersion::Unknown
    }
}

fn parse_sap_s4_odata_select_fields(request_path: &str) -> Vec<String> {
    let query = request_path.split_once('?').map(|(_, query)| query).unwrap_or("");
    if query.is_empty() {
        return Vec::new();
    }

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if key == "$select" || key.eq_ignore_ascii_case("select") {
            return split_field_list(value);
        }
    }

    Vec::new()
}

fn parse_override_select_fields(connection: &HashMap<String, String>) -> Vec<String> {
    if let Some(raw) = connection.get("odata_select_fields_json") {
        if let Ok(fields) = serde_json::from_str::<Vec<String>>(raw) {
            let mut fields = fields
                .into_iter()
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())
                .collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            return fields;
        }
    }

    connection
        .get("odata_select_fields")
        .map(|raw| split_field_list(raw))
        .unwrap_or_default()
}

fn split_field_list(raw: &str) -> Vec<String> {
    let mut fields = raw
        .split(',')
        .map(|field| field.trim())
        .filter(|field| !field.is_empty())
        .map(|field| field.to_string())
        .collect::<Vec<_>>();
    fields.sort();
    fields.dedup();
    fields
}

fn extract_terminal_projection_field(target_field_path: &str) -> Option<String> {
    let tokens = parse_json_path(target_field_path).ok()?;
    let field = tokens.iter().rev().find_map(|token| match token {
        PathToken::Field(field) => Some(field.clone()),
        PathToken::Index(_) => None,
    })?;
    if matches!(field.as_str(), "projection" | "value" | "results") {
        None
    } else {
        Some(field)
    }
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
    fn normalizes_odata_v2_payloads() {
        let payload = json!({
            "d": {
                "results": [
                    {"SalesOrder": "500000001", "NetAmount": "100.00"}
                ]
            }
        });

        assert_eq!(
            normalize_sap_s4_odata_payload(payload),
            json!([
                {"SalesOrder": "500000001", "NetAmount": "100.00"}
            ])
        );
    }

    #[test]
    fn extracts_field_from_normalized_entity() {
        let payload = json!({
            "value": [
                {"SalesOrder": "500000001", "NetAmount": "100.00"}
            ]
        });

        let actual = resolve_sap_s4_odata_value(payload, &["$.NetAmount"]).unwrap();
        assert_eq!(actual, json!("100.00"));
    }

    #[test]
    fn projects_field_across_rowsets() {
        let payload = json!({
            "value": [
                {"SalesOrder": "500000001", "NetAmount": 100.0},
                {"SalesOrder": "500000002", "NetAmount": 250.5}
            ]
        });

        let actual = resolve_sap_s4_odata_value(payload, &["$.NetAmount"]).unwrap();
        assert_eq!(actual, json!([100.0, 250.5]));
    }

    #[test]
    fn falls_back_to_first_single_entity_when_no_path_matches() {
        let payload = json!({
            "d": {
                "results": [
                    {"SalesOrder": "500000001", "NetAmount": "100.00"}
                ]
            }
        });

        let actual = resolve_sap_s4_odata_value(payload, &["$.DoesNotExist"]).unwrap();
        assert_eq!(
            actual,
            json!({"SalesOrder": "500000001", "NetAmount": "100.00"})
        );
    }

    #[test]
    fn discovers_v4_capabilities_for_entity_set_path() {
        let metadata = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="4.0" xmlns:edmx="http://docs.oasis-open.org/odata/ns/edmx">
  <edmx:DataServices>
    <Schema Namespace="API_SALES_ORDER" xmlns="http://docs.oasis-open.org/odata/ns/edm">
      <EntityType Name="A_SalesOrderType">
        <Key><PropertyRef Name="SalesOrder"/></Key>
        <Property Name="SalesOrder" Type="Edm.String" Nullable="false"/>
        <Property Name="NetAmount" Type="Edm.Decimal" Nullable="true"/>
        <Property Name="TransactionCurrency" Type="Edm.String" Nullable="true"/>
      </EntityType>
      <EntityContainer Name="Container">
        <EntitySet Name="A_SalesOrder" EntityType="API_SALES_ORDER.A_SalesOrderType"/>
      </EntityContainer>
    </Schema>
  </edmx:DataServices>
</edmx:Edmx>"#;

        let capabilities = discover_sap_s4_odata_capabilities(
            metadata,
            "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1",
        )
        .unwrap();

        assert_eq!(capabilities.version, SapS4ODataVersion::V4);
        assert_eq!(capabilities.service_root_path, "/sap/opu/odata4/API_SALES_ORDER");
        assert_eq!(capabilities.metadata_path, "/sap/opu/odata4/API_SALES_ORDER/$metadata");
        assert_eq!(capabilities.entity_set, Some("A_SalesOrder".to_string()));
        assert_eq!(
            capabilities.entity_type,
            Some("API_SALES_ORDER.A_SalesOrderType".to_string())
        );
        assert_eq!(capabilities.key_fields, vec!["SalesOrder".to_string()]);
        assert_eq!(capabilities.properties.len(), 3);
    }

    #[test]
    fn discovers_v2_capabilities_for_service_path() {
        let metadata = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="1.0" xmlns:edmx="http://schemas.microsoft.com/ado/2007/06/edmx">
  <edmx:DataServices>
    <Schema Namespace="API_SALES_ORDER_SRV" xmlns="http://schemas.microsoft.com/ado/2008/09/edm">
      <EntityType Name="A_SalesOrderType">
        <Key><PropertyRef Name="SalesOrder"/></Key>
        <Property Name="SalesOrder" Type="Edm.String" Nullable="false"/>
        <Property Name="NetAmount" Type="Edm.Decimal" Nullable="true"/>
      </EntityType>
      <EntityContainer Name="Container" m:IsDefaultEntityContainer="true" xmlns:m="http://schemas.microsoft.com/ado/2007/08/dataservices/metadata">
        <EntitySet Name="A_SalesOrder" EntityType="API_SALES_ORDER_SRV.A_SalesOrderType"/>
      </EntityContainer>
    </Schema>
  </edmx:DataServices>
</edmx:Edmx>"#;

        let capabilities = discover_sap_s4_odata_capabilities(
            metadata,
            "/sap/opu/odata/sap/API_SALES_ORDER_SRV/A_SalesOrder",
        )
        .unwrap();

        assert_eq!(capabilities.version, SapS4ODataVersion::V2);
        assert_eq!(
            capabilities.service_root_path,
            "/sap/opu/odata/sap/API_SALES_ORDER_SRV"
        );
        assert_eq!(
            capabilities.metadata_path,
            "/sap/opu/odata/sap/API_SALES_ORDER_SRV/$metadata"
        );
        assert_eq!(capabilities.entity_set, Some("A_SalesOrder".to_string()));
        assert_eq!(
            capabilities.entity_type,
            Some("API_SALES_ORDER_SRV.A_SalesOrderType".to_string())
        );
    }

    #[test]
    fn derives_requested_projection_fields_from_select_and_expected_payload() {
        let capabilities = SapS4ODataCapabilities {
            service_root_path: "/sap/opu/odata4/API_SALES_ORDER".to_string(),
            metadata_path: "/sap/opu/odata4/API_SALES_ORDER/$metadata".to_string(),
            version: SapS4ODataVersion::V4,
            entity_set: Some("A_SalesOrder".to_string()),
            entity_type: Some("API_SALES_ORDER.A_SalesOrderType".to_string()),
            key_fields: vec!["SalesOrder".to_string()],
            properties: vec![
                SapS4ODataProperty {
                    name: "SalesOrder".to_string(),
                    edm_type: "Edm.String".to_string(),
                    nullable: false,
                },
                SapS4ODataProperty {
                    name: "NetAmount".to_string(),
                    edm_type: "Edm.Decimal".to_string(),
                    nullable: true,
                },
                SapS4ODataProperty {
                    name: "TransactionCurrency".to_string(),
                    edm_type: "Edm.String".to_string(),
                    nullable: true,
                },
            ],
            supports_record_projection: true,
            supports_rowset_projection: true,
        };

        let projection = derive_sap_s4_odata_projection_fields(
            Some(&capabilities),
            "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$select=SalesOrder,NetAmount",
            "$.projection",
            Some(&json!({
                "SalesOrder": "500000001",
                "NetAmount": 100.0,
                "TransactionCurrency": "USD"
            })),
            &HashMap::new(),
        );

        assert_eq!(
            projection.requested_fields,
            vec![
                "NetAmount".to_string(),
                "SalesOrder".to_string(),
                "TransactionCurrency".to_string()
            ]
        );
        assert_eq!(
            projection.select_fields,
            vec!["NetAmount".to_string(), "SalesOrder".to_string()]
        );
        assert!(projection.missing_fields.is_empty());
    }

    #[test]
    fn detects_missing_projection_fields_from_metadata() {
        let capabilities = SapS4ODataCapabilities {
            service_root_path: "/sap/opu/odata4/API_SALES_ORDER".to_string(),
            metadata_path: "/sap/opu/odata4/API_SALES_ORDER/$metadata".to_string(),
            version: SapS4ODataVersion::V4,
            entity_set: Some("A_SalesOrder".to_string()),
            entity_type: Some("API_SALES_ORDER.A_SalesOrderType".to_string()),
            key_fields: vec!["SalesOrder".to_string()],
            properties: vec![SapS4ODataProperty {
                name: "SalesOrder".to_string(),
                edm_type: "Edm.String".to_string(),
                nullable: false,
            }],
            supports_record_projection: true,
            supports_rowset_projection: true,
        };

        let projection = derive_sap_s4_odata_projection_fields(
            Some(&capabilities),
            "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder",
            "$.projection",
            Some(&json!({"UnknownField": "x"})),
            &HashMap::new(),
        );

        assert_eq!(projection.requested_fields, vec!["UnknownField".to_string()]);
        assert_eq!(projection.missing_fields, vec!["UnknownField".to_string()]);
    }

    #[test]
    fn extracts_next_link_from_v2_and_v4_payloads() {
        let v4 = json!({
            "value": [{"SalesOrder": "500000001"}],
            "@odata.nextLink": "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$skiptoken=opaque"
        });
        let v2 = json!({
            "d": {
                "results": [{"SalesOrder": "500000001"}],
                "__next": "https://sap.example.test/next"
            }
        });

        assert_eq!(
            extract_sap_s4_odata_next_link(&v4),
            Some("/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$skiptoken=opaque".to_string())
        );
        assert_eq!(
            extract_sap_s4_odata_next_link(&v2),
            Some("https://sap.example.test/next".to_string())
        );
    }

    #[test]
    fn merges_paged_rowsets_into_one_array() {
        let merged = merge_sap_s4_odata_page_payloads(
            json!({"value": [{"SalesOrder": "500000001"}]}),
            json!({"value": [{"SalesOrder": "500000002"}]}),
        );

        assert_eq!(
            merged,
            json!([
                {"SalesOrder": "500000001"},
                {"SalesOrder": "500000002"}
            ])
        );
    }
}
