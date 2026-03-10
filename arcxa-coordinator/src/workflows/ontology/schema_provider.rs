//! Ontology Schema Provider
//!
//! Provides an interface for querying entity definitions, properties, and relationships
//! from the RDF ontology store. This module bridges the gap between the RDF-based ontology
//! and the workflow-based ETL system.
//!
//! ## Architecture
//!
//! The schema provider abstracts SPARQL queries over the RDF store, extracting:
//! - Entity class definitions with their properties
//! - Property definitions with XSD datatypes
//! - Relationship definitions (object properties) between entities
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::workflows::ontology::{OntologySchemaProvider, SparqlSchemaProvider};
//! use std::sync::Arc;
//!
//! let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
//! let provider = SparqlSchemaProvider::new(rdf_store);
//!
//! // Query entity definition
//! let entity = provider.get_entity_definition("http://example.org/Patient").await?;
//! println!("Entity: {}, Properties: {}", entity.label, entity.properties.len());
//! ```

use crate::governance::ontology::{RDFS_NS, XSD_NS};
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use crate::workflows::ontology::types::{
    Cardinality, EntityDefinition, PropertyDefinition, RelationshipDefinition,
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Ontology schema provider trait
///
/// Defines the interface for querying entity definitions from an RDF ontology.
/// Implementations must handle SPARQL query execution and result parsing.
#[async_trait]
pub trait OntologySchemaProvider: Send + Sync {
    /// Get entity definition by URI
    ///
    /// Queries the ontology for a specific entity class and returns its properties
    /// and relationships.
    ///
    /// # Arguments
    /// * `entity_uri` - Full URI of the entity class (e.g., "http://example.org/Patient")
    ///
    /// # Returns
    /// EntityDefinition containing properties and relationships
    ///
    /// # Errors
    /// Returns error if entity not found or SPARQL query fails
    async fn get_entity_definition(&self, entity_uri: &str) -> Result<EntityDefinition>;

    /// Get all entity URIs in the ontology
    ///
    /// Returns a list of all entity class URIs defined in the ontology.
    /// Useful for discovering available entities for mapping.
    ///
    /// # Returns
    /// Vector of entity URIs
    async fn get_all_entities(&self) -> Result<Vec<String>>;

    /// Resolve relationships for an entity
    ///
    /// Queries all object properties where the given entity is the domain.
    ///
    /// # Arguments
    /// * `entity_uri` - Full URI of the entity class
    ///
    /// # Returns
    /// Vector of relationship definitions
    async fn resolve_relationships(&self, entity_uri: &str) -> Result<Vec<RelationshipDefinition>>;

    /// Check if entity exists in ontology
    ///
    /// Verifies that an entity URI is defined as a class in the ontology.
    ///
    /// # Arguments
    /// * `entity_uri` - Full URI of the entity class
    ///
    /// # Returns
    /// True if entity exists, false otherwise
    async fn entity_exists(&self, entity_uri: &str) -> Result<bool>;
}

/// SPARQL-based ontology schema provider
///
/// Implements OntologySchemaProvider using SPARQL queries over a GraphicaRdfStore.
/// Extracts entity definitions from RDF triples following RDFS/OWL conventions.
///
/// ## Query Patterns
///
/// - Properties: `?prop rdfs:domain <entity_uri> ; rdfs:range ?range`
/// - Relationships: `?prop rdf:type owl:ObjectProperty ; rdfs:domain <entity_uri>`
/// - Cardinality: Inferred from OWL restrictions or defaults to OneToMany
///
/// ## Example
///
/// ```ignore
/// let provider = SparqlSchemaProvider::new(rdf_store);
/// let patient = provider.get_entity_definition("http://healthcare.org/Patient").await?;
///
/// for prop in patient.properties {
///     println!("Property: {}, Type: {}", prop.label, prop.range);
/// }
/// ```
#[derive(Clone)]
pub struct SparqlSchemaProvider {
    /// RDF store for SPARQL query execution
    rdf_store: Arc<GraphicaRdfStore>,
}

impl SparqlSchemaProvider {
    /// Create a new SPARQL schema provider
    ///
    /// # Arguments
    /// * `rdf_store` - Arc-wrapped GraphicaRdfStore for querying the ontology
    pub fn new(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self { rdf_store }
    }

    /// Execute SPARQL query and return results
    ///
    /// Wrapper around rdf_store.query() with error context.
    fn query_sparql(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        tracing::debug!("Executing SPARQL query:\n{}", sparql);
        self.rdf_store
            .query(sparql)
            .context("Failed to execute SPARQL query")
    }

    /// Extract string value from SPARQL binding
    ///
    /// Helper to safely extract string values from JSON SPARQL results.
    fn extract_string(binding: &JsonValue, key: &str) -> Option<String> {
        binding
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extract URI from SPARQL binding (strips angle brackets if present)
    fn extract_uri(binding: &JsonValue, key: &str) -> Option<String> {
        Self::extract_string(binding, key).map(|s| {
            if s.starts_with('<') && s.ends_with('>') {
                s[1..s.len() - 1].to_string()
            } else {
                s
            }
        })
    }

    /// Extract boolean from SPARQL binding
    fn extract_bool(binding: &JsonValue, key: &str) -> bool {
        binding
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false)
    }

    /// Extract optional boolean from SPARQL binding
    fn extract_optional_bool(binding: &JsonValue, key: &str) -> Option<bool> {
        binding.get(key).and_then(|v| {
            if let Some(b) = v.as_bool() {
                Some(b)
            } else if let Some(s) = v.as_str() {
                match s.to_ascii_lowercase().as_str() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    /// Extract a raw triple from simplified in-memory query results.
    ///
    /// The in-memory test store currently returns rows with `subject`, `predicate`,
    /// and `object` keys regardless of SELECT variables.
    fn extract_raw_triple(binding: &JsonValue) -> Option<(String, String, String)> {
        let subject = binding.get("subject").and_then(|v| v.as_str())?;
        let predicate = binding.get("predicate").and_then(|v| v.as_str())?;
        let object = binding.get("object").and_then(|v| v.as_str())?;
        Some((
            subject.to_string(),
            predicate.to_string(),
            object.trim_matches('"').to_string(),
        ))
    }

    /// Query all raw triples for fallback parsing in in-memory tests.
    fn query_all_triples(&self) -> Result<Vec<(String, String, String)>> {
        let rows = self
            .rdf_store
            .query("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")
            .context("Failed to query raw triples")?;

        Ok(rows
            .into_iter()
            .filter_map(|row| Self::extract_raw_triple(&row))
            .collect())
    }

    /// Parse XSD datatype URI to determine if property is multi-valued
    ///
    /// Multi-valued properties are typically arrays or lists in the ontology.
    fn is_multi_valued(range_uri: &str) -> bool {
        range_uri.contains("List") || range_uri.contains("Array") || range_uri.contains("Set")
    }

    /// Infer cardinality from OWL restrictions or property characteristics
    ///
    /// Default to OneToMany if no specific cardinality is defined.
    fn infer_cardinality(
        max_cardinality: Option<u32>,
        functional: bool,
        inverse_functional: bool,
    ) -> Cardinality {
        match (max_cardinality, functional, inverse_functional) {
            (Some(1), _, _) | (_, true, true) => Cardinality::OneToOne,
            (_, true, false) => Cardinality::ManyToOne,
            (_, false, true) => Cardinality::OneToMany,
            _ => Cardinality::OneToMany, // Default
        }
    }

    /// Extract label from entity URI (fallback to local name if no rdfs:label)
    fn extract_label_from_uri(uri: &str) -> String {
        uri.split(&['#', '/'][..])
            .last()
            .unwrap_or("UnknownEntity")
            .to_string()
    }

    /// Query properties (datatype properties) for an entity
    async fn query_properties(&self, entity_uri: &str) -> Result<Vec<PropertyDefinition>> {
        let sparql = format!(
            r#"
PREFIX rdfs: <{RDFS_NS}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX xsd: <{XSD_NS}>

SELECT DISTINCT ?property ?label ?range ?required ?multiValued
WHERE {{
  # Property must have our entity as domain
  ?property rdfs:domain <{entity_uri}> .

  # Property must be a DatatypeProperty (not ObjectProperty)
  ?property rdf:type owl:DatatypeProperty .

  # Optional: Get property label
  OPTIONAL {{ ?property rdfs:label ?label }}

  # Optional: Get range (XSD datatype)
  OPTIONAL {{ ?property rdfs:range ?range }}

  # Optional: Check if required (via SHACL or OWL cardinality)
  OPTIONAL {{
    ?restriction owl:onProperty ?property ;
                 owl:minCardinality ?minCard .
    BIND(?minCard > 0 AS ?required)
  }}

  # Optional: Check if multi-valued
  OPTIONAL {{
    ?restriction owl:onProperty ?property ;
                 owl:maxCardinality ?maxCard .
    BIND(?maxCard > 1 AS ?multiValued)
  }}
}}
"#
        );

        let results = self.query_sparql(&sparql)?;
        let mut properties = Vec::new();

        for binding in results {
            let Some(property_uri) = Self::extract_uri(&binding, "property") else {
                tracing::debug!(
                    "Skipping property binding without 'property' field: {:?}",
                    binding
                );
                continue;
            };

            let label = Self::extract_string(&binding, "label")
                .unwrap_or_else(|| Self::extract_label_from_uri(&property_uri));

            let range =
                Self::extract_uri(&binding, "range").unwrap_or_else(|| format!("{}string", XSD_NS));

            let required = Self::extract_bool(&binding, "required");
            let multi_valued =
                Self::extract_bool(&binding, "multiValued") || Self::is_multi_valued(&range);

            properties.push(PropertyDefinition {
                property_uri,
                label,
                range,
                required,
                multi_valued,
            });
        }

        if properties.is_empty() {
            let triples = self.query_all_triples()?;
            let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
            let datatype_property = "http://www.w3.org/2002/07/owl#DatatypeProperty";
            let rdfs_domain = format!("{}domain", RDFS_NS);
            let rdfs_range = format!("{}range", RDFS_NS);
            let rdfs_label = format!("{}label", RDFS_NS);

            let datatype_props: HashSet<String> = triples
                .iter()
                .filter_map(|(s, p, o)| {
                    if p == rdf_type && (o == datatype_property || o == "owl:DatatypeProperty") {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let mut fallback = Vec::new();
            for prop_uri in datatype_props {
                let has_domain = triples
                    .iter()
                    .any(|(s, p, o)| s == &prop_uri && p == &rdfs_domain && o == entity_uri);
                if !has_domain {
                    continue;
                }

                let range = triples
                    .iter()
                    .find(|(s, p, _)| s == &prop_uri && p == &rdfs_range)
                    .map(|(_, _, o)| o.clone())
                    .unwrap_or_else(|| format!("{}string", XSD_NS));

                let label = triples
                    .iter()
                    .find(|(s, p, _)| s == &prop_uri && p == &rdfs_label)
                    .map(|(_, _, o)| o.clone())
                    .unwrap_or_else(|| Self::extract_label_from_uri(&prop_uri));

                fallback.push(PropertyDefinition {
                    property_uri: prop_uri,
                    label,
                    range: range.clone(),
                    required: false,
                    multi_valued: Self::is_multi_valued(&range),
                });
            }

            properties = fallback;
        }

        tracing::debug!(
            "Found {} properties for entity {}",
            properties.len(),
            entity_uri
        );
        Ok(properties)
    }

    /// Query relationships (object properties) for an entity
    async fn query_relationships(&self, entity_uri: &str) -> Result<Vec<RelationshipDefinition>> {
        let sparql = format!(
            r#"
PREFIX rdfs: <{RDFS_NS}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>

SELECT DISTINCT ?relationship ?label ?targetEntity ?maxCard ?functional ?inverseFunctional
WHERE {{
  # Relationship must have our entity as domain
  ?relationship rdfs:domain <{entity_uri}> .

  # Relationship must be an ObjectProperty
  ?relationship rdf:type owl:ObjectProperty .

  # Get target entity (range)
  ?relationship rdfs:range ?targetEntity .

  # Optional: Get relationship label
  OPTIONAL {{ ?relationship rdfs:label ?label }}

  # Optional: Get cardinality constraints
  OPTIONAL {{
    ?restriction owl:onProperty ?relationship ;
                 owl:maxCardinality ?maxCard .
  }}

  # Optional: Check if functional (many-to-one)
  OPTIONAL {{
    ?relationship rdf:type owl:FunctionalProperty .
    BIND(true AS ?functional)
  }}

  # Optional: Check if inverse functional (one-to-many)
  OPTIONAL {{
    ?relationship rdf:type owl:InverseFunctionalProperty .
    BIND(true AS ?inverseFunctional)
  }}
}}
"#
        );

        let results = self.query_sparql(&sparql)?;
        let mut relationships = Vec::new();

        for binding in results {
            let Some(relationship_uri) = Self::extract_uri(&binding, "relationship") else {
                tracing::debug!(
                    "Skipping relationship binding without 'relationship' field: {:?}",
                    binding
                );
                continue;
            };

            let Some(target_entity_uri) = Self::extract_uri(&binding, "targetEntity") else {
                tracing::debug!(
                    "Skipping relationship binding without 'targetEntity' field: {:?}",
                    binding
                );
                continue;
            };

            let label = Self::extract_string(&binding, "label")
                .unwrap_or_else(|| Self::extract_label_from_uri(&relationship_uri));

            let max_card =
                Self::extract_string(&binding, "maxCard").and_then(|s| s.parse::<u32>().ok());

            let functional = Self::extract_bool(&binding, "functional");
            let inverse_functional = Self::extract_bool(&binding, "inverseFunctional");

            let cardinality = Self::infer_cardinality(max_card, functional, inverse_functional);

            relationships.push(RelationshipDefinition {
                relationship_uri,
                label,
                target_entity_uri,
                cardinality,
            });
        }

        if relationships.is_empty() {
            let triples = self.query_all_triples()?;
            let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
            let object_property = "http://www.w3.org/2002/07/owl#ObjectProperty";
            let rdfs_domain = format!("{}domain", RDFS_NS);
            let rdfs_range = format!("{}range", RDFS_NS);
            let rdfs_label = format!("{}label", RDFS_NS);

            let object_props: HashSet<String> = triples
                .iter()
                .filter_map(|(s, p, o)| {
                    if p == rdf_type && (o == object_property || o == "owl:ObjectProperty") {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let mut fallback = Vec::new();
            for rel_uri in object_props {
                let has_domain = triples
                    .iter()
                    .any(|(s, p, o)| s == &rel_uri && p == &rdfs_domain && o == entity_uri);
                if !has_domain {
                    continue;
                }

                let Some(target_entity_uri) = triples
                    .iter()
                    .find(|(s, p, _)| s == &rel_uri && p == &rdfs_range)
                    .map(|(_, _, o)| o.clone())
                else {
                    continue;
                };

                let label = triples
                    .iter()
                    .find(|(s, p, _)| s == &rel_uri && p == &rdfs_label)
                    .map(|(_, _, o)| o.clone())
                    .unwrap_or_else(|| Self::extract_label_from_uri(&rel_uri));

                fallback.push(RelationshipDefinition {
                    relationship_uri: rel_uri,
                    label,
                    target_entity_uri,
                    cardinality: Cardinality::OneToMany,
                });
            }

            relationships = fallback;
        }

        tracing::debug!(
            "Found {} relationships for entity {}",
            relationships.len(),
            entity_uri
        );
        Ok(relationships)
    }

    /// Query entity label from ontology
    async fn query_entity_label(&self, entity_uri: &str) -> Result<String> {
        let sparql = format!(
            r#"
PREFIX rdfs: <{RDFS_NS}>

SELECT ?label
WHERE {{
  <{entity_uri}> rdfs:label ?label .
}}
LIMIT 1
"#
        );

        let results = self.query_sparql(&sparql)?;
        if let Some(label) = results
            .first()
            .and_then(|b| Self::extract_string(b, "label"))
        {
            return Ok(label);
        }

        let label_predicate = format!("{}label", RDFS_NS);
        let triples = self.query_all_triples()?;
        Ok(triples
            .iter()
            .find(|(s, p, _)| s == entity_uri && p == &label_predicate)
            .map(|(_, _, o)| o.clone())
            .unwrap_or_else(|| Self::extract_label_from_uri(entity_uri)))
    }
}

#[async_trait]
impl OntologySchemaProvider for SparqlSchemaProvider {
    async fn get_entity_definition(&self, entity_uri: &str) -> Result<EntityDefinition> {
        tracing::info!("Fetching entity definition for: {}", entity_uri);

        // Fail fast for invalid entities so callers can separate ontology errors from DB errors.
        let exists = self.entity_exists(entity_uri).await.with_context(|| {
            format!("Failed to verify ontology entity existence: {}", entity_uri)
        })?;

        if !exists {
            return Err(anyhow!("Entity not found in ontology: {}", entity_uri));
        }

        // Query entity label
        let label = self
            .query_entity_label(entity_uri)
            .await
            .context("Failed to query entity label")?;

        // Query properties (datatype properties)
        let properties = self
            .query_properties(entity_uri)
            .await
            .context("Failed to query entity properties")?;

        // Query relationships (object properties)
        let relationships = self
            .query_relationships(entity_uri)
            .await
            .context("Failed to query entity relationships")?;

        Ok(EntityDefinition {
            entity_uri: entity_uri.to_string(),
            label,
            properties,
            relationships,
        })
    }

    async fn get_all_entities(&self) -> Result<Vec<String>> {
        tracing::info!("Querying all entities from ontology");

        let sparql = format!(
            r#"
PREFIX rdfs: <{RDFS_NS}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>

SELECT DISTINCT ?entity
WHERE {{
  ?entity rdf:type owl:Class .

  # Filter out OWL/RDFS built-in classes
  FILTER(!STRSTARTS(STR(?entity), "http://www.w3.org/2002/07/owl#"))
  FILTER(!STRSTARTS(STR(?entity), "http://www.w3.org/2000/01/rdf-schema#"))
  FILTER(!STRSTARTS(STR(?entity), "http://www.w3.org/1999/02/22-rdf-syntax-ns#"))
}}
ORDER BY ?entity
"#
        );

        let results = self.query_sparql(&sparql)?;
        let mut entities: Vec<String> = results
            .iter()
            .filter_map(|b| Self::extract_uri(b, "entity"))
            .collect();

        if entities.is_empty() {
            let triples = self.query_all_triples()?;
            let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

            entities = triples
                .into_iter()
                .filter_map(|(s, p, o)| {
                    if p == rdf_type
                        && (o == "http://www.w3.org/2002/07/owl#Class" || o == "owl:Class")
                        && !s.starts_with("http://www.w3.org/2002/07/owl#")
                        && !s.starts_with("http://www.w3.org/2000/01/rdf-schema#")
                        && !s.starts_with("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
                    {
                        Some(s)
                    } else {
                        None
                    }
                })
                .collect();
            entities.sort();
            entities.dedup();
        }

        tracing::info!("Found {} entities in ontology", entities.len());
        Ok(entities)
    }

    async fn resolve_relationships(&self, entity_uri: &str) -> Result<Vec<RelationshipDefinition>> {
        tracing::info!("Resolving relationships for entity: {}", entity_uri);
        self.query_relationships(entity_uri)
            .await
            .context("Failed to resolve relationships")
    }

    async fn entity_exists(&self, entity_uri: &str) -> Result<bool> {
        let sparql = format!(
            r#"
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>

ASK {{
  <{entity_uri}> rdf:type owl:Class .
}}
"#
        );

        let results = self.query_sparql(&sparql)?;

        // Standard ASK response (SPARQL JSON): {"boolean": true/false}
        if let Some(exists) = results
            .iter()
            .find_map(|r| Self::extract_optional_bool(r, "boolean"))
        {
            tracing::debug!("Entity {} exists (ASK): {}", entity_uri, exists);
            return Ok(exists);
        }

        // Fallback for simplified/in-memory query engines returning raw triples.
        let exists_from_triples = results.iter().any(|r| {
            let subject_matches = Self::extract_string(r, "subject")
                .map(|s| s == entity_uri)
                .unwrap_or(false);
            let predicate_matches = Self::extract_string(r, "predicate")
                .map(|p| p == "rdf:type" || p.ends_with("rdf-syntax-ns#type"))
                .unwrap_or(false);
            let object_matches = Self::extract_string(r, "object")
                .map(|o| {
                    o == "owl:Class"
                        || o.ends_with("owl#Class")
                        || o.contains("owl:Class")
                        || o.contains("owl#Class")
                })
                .unwrap_or(false);

            subject_matches && predicate_matches && object_matches
        });

        tracing::debug!(
            "Entity {} exists (fallback triple parsing): {}",
            entity_uri,
            exists_from_triples
        );
        Ok(exists_from_triples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::{NamedGraph, RdfStore};

    /// Create a test RDF store with sample ontology
    fn create_test_store() -> Result<Arc<GraphicaRdfStore>> {
        let store = Arc::new(GraphicaRdfStore::new_in_memory()?);
        store.insert_triples(
            vec![
                (
                    "http://example.org/Patient".to_string(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://www.w3.org/2002/07/owl#Class".to_string(),
                ),
                (
                    "http://example.org/Patient".to_string(),
                    format!("{}label", RDFS_NS),
                    "\"Patient\"".to_string(),
                ),
                (
                    "http://example.org/patientId".to_string(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://www.w3.org/2002/07/owl#DatatypeProperty".to_string(),
                ),
                (
                    "http://example.org/patientId".to_string(),
                    format!("{}domain", RDFS_NS),
                    "http://example.org/Patient".to_string(),
                ),
                (
                    "http://example.org/patientId".to_string(),
                    format!("{}range", RDFS_NS),
                    format!("{}string", XSD_NS),
                ),
                (
                    "http://example.org/patientId".to_string(),
                    format!("{}label", RDFS_NS),
                    "\"Patient ID\"".to_string(),
                ),
                (
                    "http://example.org/birthDate".to_string(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://www.w3.org/2002/07/owl#DatatypeProperty".to_string(),
                ),
                (
                    "http://example.org/birthDate".to_string(),
                    format!("{}domain", RDFS_NS),
                    "http://example.org/Patient".to_string(),
                ),
                (
                    "http://example.org/birthDate".to_string(),
                    format!("{}range", RDFS_NS),
                    format!("{}date", XSD_NS),
                ),
                (
                    "http://example.org/birthDate".to_string(),
                    format!("{}label", RDFS_NS),
                    "\"Birth Date\"".to_string(),
                ),
                (
                    "http://example.org/Department".to_string(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://www.w3.org/2002/07/owl#Class".to_string(),
                ),
                (
                    "http://example.org/Department".to_string(),
                    format!("{}label", RDFS_NS),
                    "\"Department\"".to_string(),
                ),
                (
                    "http://example.org/assignedDepartment".to_string(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://www.w3.org/2002/07/owl#ObjectProperty".to_string(),
                ),
                (
                    "http://example.org/assignedDepartment".to_string(),
                    format!("{}domain", RDFS_NS),
                    "http://example.org/Patient".to_string(),
                ),
                (
                    "http://example.org/assignedDepartment".to_string(),
                    format!("{}range", RDFS_NS),
                    "http://example.org/Department".to_string(),
                ),
                (
                    "http://example.org/assignedDepartment".to_string(),
                    format!("{}label", RDFS_NS),
                    "\"Assigned Department\"".to_string(),
                ),
            ],
            Some(&NamedGraph::current()),
        )?;
        Ok(store)
    }

    #[tokio::test]
    async fn test_get_entity_definition() {
        let store = create_test_store().unwrap();
        let provider = SparqlSchemaProvider::new(store);

        let entity = provider
            .get_entity_definition("http://example.org/Patient")
            .await
            .unwrap();

        assert_eq!(entity.entity_uri, "http://example.org/Patient");
        assert_eq!(entity.label, "Patient");
        assert!(entity.properties.len() >= 2);
        assert!(entity.relationships.len() >= 1);
    }

    #[tokio::test]
    async fn test_get_all_entities() {
        let store = create_test_store().unwrap();
        let provider = SparqlSchemaProvider::new(store);

        let entities = provider.get_all_entities().await.unwrap();

        assert!(entities.len() >= 2);
        assert!(entities.contains(&"http://example.org/Patient".to_string()));
        assert!(entities.contains(&"http://example.org/Department".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_relationships() {
        let store = create_test_store().unwrap();
        let provider = SparqlSchemaProvider::new(store);

        let relationships = provider
            .resolve_relationships("http://example.org/Patient")
            .await
            .unwrap();

        assert!(relationships.len() >= 1);
        let dept_rel = relationships
            .iter()
            .find(|r| r.relationship_uri == "http://example.org/assignedDepartment");
        assert!(dept_rel.is_some());
        assert_eq!(
            dept_rel.unwrap().target_entity_uri,
            "http://example.org/Department"
        );
    }

    #[tokio::test]
    async fn test_entity_exists() {
        let store = create_test_store().unwrap();
        let provider = SparqlSchemaProvider::new(store);

        assert!(provider
            .entity_exists("http://example.org/Patient")
            .await
            .unwrap());
        assert!(!provider
            .entity_exists("http://example.org/NonExistent")
            .await
            .unwrap());
    }

    #[test]
    fn test_extract_label_from_uri() {
        assert_eq!(
            SparqlSchemaProvider::extract_label_from_uri("http://example.org/Patient"),
            "Patient"
        );
        assert_eq!(
            SparqlSchemaProvider::extract_label_from_uri("http://example.org/ontology#Doctor"),
            "Doctor"
        );
    }

    #[test]
    fn test_infer_cardinality() {
        assert_eq!(
            SparqlSchemaProvider::infer_cardinality(Some(1), false, false),
            Cardinality::OneToOne
        );
        assert_eq!(
            SparqlSchemaProvider::infer_cardinality(None, true, false),
            Cardinality::ManyToOne
        );
        assert_eq!(
            SparqlSchemaProvider::infer_cardinality(None, false, true),
            Cardinality::OneToMany
        );
        assert_eq!(
            SparqlSchemaProvider::infer_cardinality(None, false, false),
            Cardinality::OneToMany
        );
    }

    #[test]
    fn test_is_multi_valued() {
        assert!(SparqlSchemaProvider::is_multi_valued(
            "http://example.org/StringList"
        ));
        assert!(SparqlSchemaProvider::is_multi_valued(
            "http://example.org/Array"
        ));
        assert!(!SparqlSchemaProvider::is_multi_valued(
            "http://www.w3.org/2001/XMLSchema#string"
        ));
    }
}
