//! Ontology/shape synchronization helpers for SoS catalog entities.
//!
//! Phase 1 keeps governance pragmatic: interface schemas are mirrored into the
//! persisted ontology registry as stable SHACL-like assets so validation reports
//! can record concrete ontology/shape references that survive restarts.

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::api::sos_validation::storage::{Interface, SosStorageManager};
use crate::mapping::ontology_registry::PersistedOntologyRegistry;

const SOS_CORE_ONTOLOGY_ID: &str = "sos_core";
const SOS_CORE_NAMESPACE: &str = "http://graphica.io/sos#";
const INTERFACE_SHAPE_ONTOLOGY_PREFIX: &str = "sos_interface_shape_";

const SOS_CORE_ONTOLOGY: &str = r#"
@prefix sos: <http://graphica.io/sos#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

sos:System a rdfs:Class .
sos:Interface a rdfs:Class .
sos:Contract a rdfs:Class .
sos:ValidationActivity a rdfs:Class .
sos:ValidationReport a rdfs:Class .

sos:systemId a rdf:Property .
sos:interfaceId a rdf:Property .
sos:contractId a rdf:Property .
sos:shapeRef a rdf:Property .
sos:schemaHash a rdf:Property .
sos:validationType a rdf:Property .
sos:subjectKey a rdf:Property .

sh:NodeShape a rdfs:Class .
prov:Activity a rdfs:Class .
prov:Entity a rdfs:Class .
"#;

pub async fn reconcile_sos_ontology_assets(
    storage_manager: &SosStorageManager,
    registry: &PersistedOntologyRegistry,
) -> Result<()> {
    ensure_sos_core_ontology(registry).await?;

    for mut interface in storage_manager
        .list_all_interfaces(0, usize::MAX)
        .context("Failed to list SoS interfaces for ontology reconciliation")?
    {
        let previous_metadata = interface.metadata.clone();
        ensure_interface_ontology_assets(&mut interface, Some(registry)).await?;

        if interface.metadata != previous_metadata {
            storage_manager.put_interface(&interface).with_context(|| {
                format!(
                    "Failed to persist SoS ontology metadata for interface '{}'",
                    interface.interface_id
                )
            })?;
        }
    }

    Ok(())
}

pub async fn ensure_interface_ontology_assets(
    interface: &mut Interface,
    registry: Option<&PersistedOntologyRegistry>,
) -> Result<()> {
    let schema_hash = hash_json_value(&interface.schema)?;
    let shape_ref = interface_shape_ref(&interface.interface_id, &schema_hash);
    interface
        .metadata
        .insert("shape_ref".to_string(), Value::String(shape_ref.clone()));
    interface.metadata.insert(
        "schema_hash".to_string(),
        Value::String(schema_hash.clone()),
    );

    let Some(registry) = registry else {
        return Ok(());
    };

    ensure_sos_core_ontology(registry).await?;

    let ontology_id = interface_shape_ontology_id(&interface.interface_id, &schema_hash);
    let content = build_interface_shape_ontology(interface, &shape_ref, &schema_hash);
    upsert_ontology(
        registry,
        &ontology_id,
        &content,
        Some(format!("{}#", shape_ref)),
    )
    .await?;

    let interface_id = interface.interface_id.clone();
    let system_id = interface.system_id.clone();
    let schema_hash_for_metadata = schema_hash.clone();
    registry
        .update_metadata(&ontology_id, move |metadata| {
            metadata.name = format!("SoS Shape {}", interface_id);
            metadata.description = Some(format!(
                "Persisted SHACL-like shape asset for SoS interface '{}' on system '{}'",
                interface_id, system_id
            ));
            metadata.version = schema_hash_for_metadata.clone();
            metadata.tags = vec![
                "sos".to_string(),
                "interface-shape".to_string(),
                interface_id.clone(),
                system_id.clone(),
            ];
            metadata.active = true;
        })
        .await
        .with_context(|| {
            format!(
                "Failed to update metadata for SoS shape ontology '{}'",
                ontology_id
            )
        })?;

    sync_interface_ontology_metadata(&mut interface.metadata, &ontology_id);
    interface
        .metadata
        .insert("shape_ontology_id".to_string(), Value::String(ontology_id));

    Ok(())
}

async fn ensure_sos_core_ontology(registry: &PersistedOntologyRegistry) -> Result<()> {
    upsert_ontology(
        registry,
        SOS_CORE_ONTOLOGY_ID,
        SOS_CORE_ONTOLOGY,
        Some(SOS_CORE_NAMESPACE.to_string()),
    )
    .await?;

    registry
        .update_metadata(SOS_CORE_ONTOLOGY_ID, |metadata| {
            metadata.name = "Systems-of-Systems Core Ontology".to_string();
            metadata.description = Some(
                "Core Systems-of-Systems vocabulary used by SoS validation, workflow governance, and validation lineage"
                    .to_string(),
            );
            metadata.version = "1.0.0".to_string();
            metadata.tags = vec![
                "sos".to_string(),
                "governance".to_string(),
                "validation-lineage".to_string(),
            ];
            metadata.active = true;
        })
        .await
        .context("Failed to update SoS core ontology metadata")?;

    Ok(())
}

async fn upsert_ontology(
    registry: &PersistedOntologyRegistry,
    ontology_id: &str,
    content: &str,
    namespace: Option<String>,
) -> Result<()> {
    if registry.get_ontology(ontology_id).is_some() {
        registry
            .update_ontology(ontology_id, content.to_string())
            .await
            .with_context(|| format!("Failed to update ontology '{}'", ontology_id))?;
    } else {
        registry
            .register_custom_ontology(ontology_id.to_string(), content.to_string(), namespace)
            .await
            .with_context(|| format!("Failed to register ontology '{}'", ontology_id))?;
    }

    Ok(())
}

fn interface_shape_ref(interface_id: &str, schema_hash: &str) -> String {
    format!(
        "http://graphica.io/sos/interface/{}/shape/{}",
        interface_id, schema_hash
    )
}

fn interface_shape_ontology_id(interface_id: &str, schema_hash: &str) -> String {
    format!(
        "sos_interface_shape_{}_{}",
        sanitize_identifier(interface_id),
        sanitize_identifier(schema_hash)
    )
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn hash_json_value(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("Failed to serialize interface schema")?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn build_interface_shape_ontology(
    interface: &Interface,
    shape_ref: &str,
    schema_hash: &str,
) -> String {
    let schema_json = serde_json::to_string_pretty(&interface.schema)
        .unwrap_or_else(|_| interface.schema.to_string())
        .replace('\\', "\\\\")
        .replace('\"', "\\\"");
    let coordinate_clause = interface
        .coordinate_system
        .as_ref()
        .map(|coordinate_system| {
            format!(
                "    sos:coordinateSystem \"{}\" ;\n",
                escape_turtle_literal(coordinate_system)
            )
        })
        .unwrap_or_default();
    let unit_clause = interface
        .unit_system
        .as_ref()
        .map(|unit_system| {
            format!(
                "    sos:unitSystem \"{}\" ;\n",
                escape_turtle_literal(unit_system)
            )
        })
        .unwrap_or_default();

    format!(
        "@prefix sos: <http://graphica.io/sos#> .\n\
@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
\n\
<{shape_ref}> a sh:NodeShape ;\n\
    sh:targetClass sos:Interface ;\n\
    sh:name \"{interface_name}\" ;\n\
    sos:interfaceId \"{interface_id}\" ;\n\
    sos:systemId \"{system_id}\" ;\n\
    sos:dataFormat \"{data_format}\" ;\n\
{coordinate_clause}\
{unit_clause}\
    sos:schemaHash \"{schema_hash}\" ;\n\
    sos:jsonSchema \"{schema_json}\" .\n",
        shape_ref = shape_ref,
        interface_name = escape_turtle_literal(&interface.interface_name),
        interface_id = escape_turtle_literal(&interface.interface_id),
        system_id = escape_turtle_literal(&interface.system_id),
        data_format = escape_turtle_literal(&interface.data_format),
        coordinate_clause = coordinate_clause,
        unit_clause = unit_clause,
        schema_hash = escape_turtle_literal(schema_hash),
        schema_json = schema_json,
    )
}

fn escape_turtle_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn sync_interface_ontology_metadata(
    metadata: &mut std::collections::HashMap<String, Value>,
    current_shape_ontology_id: &str,
) {
    let mut ontology_ids = read_string_list(metadata, "ontology_ids")
        .into_iter()
        .filter(|candidate| {
            !candidate.starts_with(INTERFACE_SHAPE_ONTOLOGY_PREFIX)
                || candidate == current_shape_ontology_id
        })
        .collect::<Vec<_>>();

    if let Some(stale_singleton) = metadata
        .get("ontology_id")
        .and_then(Value::as_str)
        .filter(|candidate| {
            candidate.starts_with(INTERFACE_SHAPE_ONTOLOGY_PREFIX)
                && *candidate != current_shape_ontology_id
        })
        .map(str::to_string)
    {
        let _ = stale_singleton;
        metadata.remove("ontology_id");
    }

    if !ontology_ids
        .iter()
        .any(|candidate| candidate == SOS_CORE_ONTOLOGY_ID)
    {
        ontology_ids.push(SOS_CORE_ONTOLOGY_ID.to_string());
    }
    if !ontology_ids
        .iter()
        .any(|candidate| candidate == current_shape_ontology_id)
    {
        ontology_ids.push(current_shape_ontology_id.to_string());
    }

    metadata.insert(
        "ontology_ids".to_string(),
        Value::Array(
            ontology_ids
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
}

fn read_string_list(metadata: &std::collections::HashMap<String, Value>, key: &str) -> Vec<String> {
    match metadata.get(key) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn sample_interface() -> Interface {
        Interface {
            interface_id: "provider-if".to_string(),
            system_id: "provider-system".to_string(),
            interface_name: "Provider Interface".to_string(),
            direction: "outbound".to_string(),
            protocol: "https".to_string(),
            data_format: "json".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "sample_id": {"type": "string"}
                }
            }),
            coordinate_system: Some("WGS84".to_string()),
            unit_system: Some("SI".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn ensure_interface_assets_registers_shape_and_metadata() {
        let temp_dir = TempDir::new().expect("temporary directory should exist");
        let registry = PersistedOntologyRegistry::open(temp_dir.path())
            .await
            .expect("registry should open");
        let mut interface = sample_interface();

        ensure_interface_ontology_assets(&mut interface, Some(&registry))
            .await
            .expect("interface assets should be created");

        let shape_ref = interface
            .metadata
            .get("shape_ref")
            .and_then(Value::as_str)
            .expect("shape_ref should be stored");
        assert!(shape_ref.contains("/sos/interface/provider-if/shape/"));
        assert!(registry.get_ontology(SOS_CORE_ONTOLOGY_ID).is_some());

        let shape_ontology_id = interface
            .metadata
            .get("shape_ontology_id")
            .and_then(Value::as_str)
            .expect("shape ontology id should be stored");
        assert!(registry.get_ontology(shape_ontology_id).is_some());

        let ontology_ids = read_string_list(&interface.metadata, "ontology_ids");
        assert!(ontology_ids.contains(&SOS_CORE_ONTOLOGY_ID.to_string()));
        assert!(ontology_ids.contains(&shape_ontology_id.to_string()));
    }

    #[tokio::test]
    async fn ensure_interface_assets_replace_stale_shape_metadata_on_schema_change() {
        let temp_dir = TempDir::new().expect("temporary directory should exist");
        let registry = PersistedOntologyRegistry::open(temp_dir.path())
            .await
            .expect("registry should open");
        let mut interface = sample_interface();
        interface.metadata.insert(
            "ontology_ids".to_string(),
            Value::Array(vec![Value::String("custom_domain_ontology".to_string())]),
        );

        ensure_interface_ontology_assets(&mut interface, Some(&registry))
            .await
            .expect("initial interface assets should be created");

        let first_shape_ref = interface
            .metadata
            .get("shape_ref")
            .and_then(Value::as_str)
            .expect("initial shape_ref should be stored")
            .to_string();
        let first_shape_ontology_id = interface
            .metadata
            .get("shape_ontology_id")
            .and_then(Value::as_str)
            .expect("initial shape ontology id should be stored")
            .to_string();

        interface.schema = serde_json::json!({
            "type": "object",
            "properties": {
                "sample_id": {"type": "string"},
                "temperature_c": {"type": "number"}
            },
            "required": ["sample_id", "temperature_c"]
        });
        interface.metadata.insert(
            "ontology_id".to_string(),
            Value::String(first_shape_ontology_id.clone()),
        );

        ensure_interface_ontology_assets(&mut interface, Some(&registry))
            .await
            .expect("updated interface assets should be created");

        let second_shape_ref = interface
            .metadata
            .get("shape_ref")
            .and_then(Value::as_str)
            .expect("updated shape_ref should be stored")
            .to_string();
        let second_shape_ontology_id = interface
            .metadata
            .get("shape_ontology_id")
            .and_then(Value::as_str)
            .expect("updated shape ontology id should be stored")
            .to_string();

        assert_ne!(first_shape_ref, second_shape_ref);
        assert_ne!(first_shape_ontology_id, second_shape_ontology_id);

        let ontology_ids = read_string_list(&interface.metadata, "ontology_ids");
        assert!(ontology_ids.contains(&"custom_domain_ontology".to_string()));
        assert!(ontology_ids.contains(&SOS_CORE_ONTOLOGY_ID.to_string()));
        assert!(ontology_ids.contains(&second_shape_ontology_id));
        assert!(!ontology_ids.contains(&first_shape_ontology_id));
        assert_eq!(interface.metadata.get("ontology_id"), None);
    }
}
