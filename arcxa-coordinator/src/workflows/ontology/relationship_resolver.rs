//! Relationship Resolution
//!
//! Resolves entity relationships to foreign key IDs, handling URIs, identifiers,
//! nested objects, and many-to-many relationships with junction table support.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::types::{Cardinality, EntityDefinition, RelationshipDefinition, TableSchema};

/// Trait for resolving entity relationships to foreign keys
#[async_trait]
pub trait RelationshipResolver: Send + Sync {
    /// Resolve all relationships for multiple rows, adding FK columns
    async fn resolve_relationships(
        &self,
        rows: &mut [Map<String, JsonValue>],
        entity_def: &EntityDefinition,
        schema: &TableSchema,
    ) -> Result<()>;

    /// Resolve a single relationship to a foreign key value
    async fn resolve_relationship(
        &self,
        source_row: &Map<String, JsonValue>,
        relationship: &RelationshipDefinition,
    ) -> Result<Option<String>>;

    /// Extract junction table data for many-to-many relationships
    async fn extract_junction_data(
        &self,
        rows: &[Map<String, JsonValue>],
        relationship: &RelationshipDefinition,
    ) -> Result<Vec<(String, String)>>;

    /// Lookup entity ID by URI or identifier
    async fn lookup_entity_id(&self, entity_uri: &str, identifier: &str) -> Result<Option<String>>;
}

/// Default in-memory relationship resolver with ID caching
#[derive(Debug)]
pub struct DefaultRelationshipResolver {
    /// Map of (entity_uri, identifier) -> ID
    id_cache: Arc<RwLock<HashMap<(String, String), String>>>,
    /// Auto-increment counter for generating IDs
    next_id: Arc<RwLock<i64>>,
    /// Whether to auto-generate IDs for unknown entities
    auto_generate_ids: bool,
}

impl DefaultRelationshipResolver {
    /// Create a new default relationship resolver
    pub fn new() -> Self {
        Self {
            id_cache: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
            auto_generate_ids: true,
        }
    }

    /// Set whether to auto-generate IDs for unknown entities
    pub fn with_auto_generate_ids(mut self, generate: bool) -> Self {
        self.auto_generate_ids = generate;
        self
    }

    /// Register an entity with an ID (for testing/pre-loading)
    pub async fn register_entity(&self, entity_uri: &str, identifier: &str, id: String) {
        let mut cache = self.id_cache.write().await;
        cache.insert((entity_uri.to_string(), identifier.to_string()), id);
    }

    /// Extract identifier from a JSON value (supports multiple formats)
    fn extract_identifier_from_value(&self, value: &JsonValue) -> Result<String> {
        match value {
            JsonValue::String(s) => Ok(s.clone()),
            JsonValue::Number(n) => Ok(n.to_string()),
            JsonValue::Object(obj) => {
                // Try various common identifier fields
                let fields = ["id", "identifier", "uri", "@id", "name", "key"];
                for field in &fields {
                    if let Some(val) = obj.get(*field) {
                        return self.extract_identifier_from_value(val);
                    }
                }
                Err(anyhow!(
                    "Cannot extract identifier from object: no recognized identifier field found (tried: {:?})",
                    fields
                ))
            }
            _ => Err(anyhow!(
                "Cannot extract identifier from value type: {}",
                match value {
                    JsonValue::Array(_) => "array",
                    JsonValue::Bool(_) => "boolean",
                    JsonValue::Null => "null",
                    _ => "unknown",
                }
            )),
        }
    }

    /// Get or create an ID for an entity
    async fn get_or_create_id(&self, entity_uri: &str, identifier: &str) -> Result<String> {
        // Check cache first
        {
            let cache = self.id_cache.read().await;
            if let Some(id) = cache.get(&(entity_uri.to_string(), identifier.to_string())) {
                return Ok(id.clone());
            }
        }

        // Generate new ID if auto-generation is enabled
        if self.auto_generate_ids {
            let mut next_id_lock = self.next_id.write().await;
            let id = next_id_lock.to_string();
            *next_id_lock += 1;
            drop(next_id_lock);

            // Store in cache
            let mut cache = self.id_cache.write().await;
            cache.insert((entity_uri.to_string(), identifier.to_string()), id.clone());

            debug!(
                "Auto-generated ID '{}' for entity '{}' with identifier '{}'",
                id, entity_uri, identifier
            );

            Ok(id)
        } else {
            Err(anyhow!(
                "Entity ID not found for '{}' with identifier '{}' and auto-generation is disabled",
                entity_uri,
                identifier
            ))
        }
    }

    /// Extract URI from full ontology URI (e.g., "http://example.org#name" -> "name")
    fn extract_uri_local_name(&self, uri: &str) -> String {
        uri.split('#')
            .last()
            .or_else(|| uri.split('/').last())
            .unwrap_or(uri)
            .to_string()
    }

    /// Generate foreign key column name from relationship
    fn generate_fk_column_name(&self, relationship: &RelationshipDefinition) -> String {
        let label = self.extract_uri_local_name(&relationship.label);
        format!("{}_id", label.to_lowercase().replace('-', "_"))
    }

    /// Find relationship value in source row (case-insensitive)
    fn find_relationship_value<'a>(
        &self,
        row: &'a Map<String, JsonValue>,
        relationship: &RelationshipDefinition,
    ) -> Option<&'a JsonValue> {
        let label = &relationship.label;
        let local_name = self.extract_uri_local_name(label);

        // Try exact match first
        if let Some(val) = row.get(label) {
            return Some(val);
        }

        // Try local name
        if let Some(val) = row.get(&local_name) {
            return Some(val);
        }

        // Try case-insensitive match
        for (key, value) in row.iter() {
            if key.eq_ignore_ascii_case(label) || key.eq_ignore_ascii_case(&local_name) {
                return Some(value);
            }
        }

        None
    }

    /// Extract IDs from array value (for one-to-many/many-to-many)
    async fn extract_ids_from_array(
        &self,
        array: &[JsonValue],
        target_entity_uri: &str,
    ) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        for item in array {
            match self.extract_identifier_from_value(item) {
                Ok(identifier) => {
                    let id = self
                        .get_or_create_id(target_entity_uri, &identifier)
                        .await?;
                    ids.push(id);
                }
                Err(e) => {
                    warn!("Failed to extract identifier from array item: {}", e);
                }
            }
        }

        Ok(ids)
    }
}

impl Default for DefaultRelationshipResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RelationshipResolver for DefaultRelationshipResolver {
    async fn resolve_relationships(
        &self,
        rows: &mut [Map<String, JsonValue>],
        entity_def: &EntityDefinition,
        _schema: &TableSchema,
    ) -> Result<()> {
        // Resolve relationships for each row
        for row in rows.iter_mut() {
            for relationship in &entity_def.relationships {
                // Skip OneToMany and ManyToMany (handled via junction tables)
                if matches!(
                    relationship.cardinality,
                    Cardinality::OneToMany | Cardinality::ManyToMany
                ) {
                    continue;
                }

                // Resolve OneToOne and ManyToOne relationships
                if let Some(fk_id) = self.resolve_relationship(row, relationship).await? {
                    // Add FK column to row
                    let fk_column_name = format!("{}_id", relationship.label);
                    row.insert(fk_column_name, JsonValue::String(fk_id));
                }
            }
        }

        Ok(())
    }

    async fn resolve_relationship(
        &self,
        source_row: &Map<String, JsonValue>,
        relationship: &RelationshipDefinition,
    ) -> Result<Option<String>> {
        // Find relationship value in source row
        let rel_value = match self.find_relationship_value(source_row, relationship) {
            Some(val) => val,
            None => {
                debug!(
                    "Relationship '{}' not found in source row",
                    relationship.label
                );
                return Ok(None);
            }
        };

        // Handle null values
        if rel_value.is_null() {
            return Ok(None);
        }

        // Handle different value types based on cardinality
        match relationship.cardinality {
            Cardinality::ManyToOne | Cardinality::OneToOne => {
                // Single foreign key reference
                match rel_value {
                    JsonValue::String(_) | JsonValue::Number(_) | JsonValue::Object(_) => {
                        let identifier = self.extract_identifier_from_value(rel_value)?;
                        let id = self
                            .get_or_create_id(&relationship.target_entity_uri, &identifier)
                            .await?;
                        Ok(Some(id))
                    }
                    JsonValue::Array(arr) if !arr.is_empty() => {
                        // Take first element for ManyToOne
                        warn!(
                            "Array found for ManyToOne relationship '{}', taking first element",
                            relationship.label
                        );
                        let identifier = self.extract_identifier_from_value(&arr[0])?;
                        let id = self
                            .get_or_create_id(&relationship.target_entity_uri, &identifier)
                            .await?;
                        Ok(Some(id))
                    }
                    _ => Err(anyhow!(
                        "Invalid value type for relationship '{}'",
                        relationship.label
                    )),
                }
            }
            Cardinality::OneToMany | Cardinality::ManyToMany => {
                // For these, we don't store FK in main table (use junction table)
                // Return None to indicate no direct FK
                Ok(None)
            }
        }
    }

    async fn extract_junction_data(
        &self,
        rows: &[Map<String, JsonValue>],
        relationship: &RelationshipDefinition,
    ) -> Result<Vec<(String, String)>> {
        let mut junction_data = Vec::new();

        // Only process if this is a many-to-many or one-to-many relationship
        if !matches!(
            relationship.cardinality,
            Cardinality::ManyToMany | Cardinality::OneToMany
        ) {
            return Ok(junction_data);
        }

        for row in rows {
            // Extract source entity ID (assuming "id" field exists)
            let source_id = match row.get("id") {
                Some(JsonValue::String(s)) => s.clone(),
                Some(JsonValue::Number(n)) => n.to_string(),
                _ => {
                    warn!("Source row missing 'id' field, skipping junction data extraction");
                    continue;
                }
            };

            // Find relationship value in row
            let rel_value = match self.find_relationship_value(row, relationship) {
                Some(val) => val,
                None => continue,
            };

            // Extract target IDs
            match rel_value {
                JsonValue::Array(arr) => {
                    let target_ids = self
                        .extract_ids_from_array(arr, &relationship.target_entity_uri)
                        .await?;

                    for target_id in target_ids {
                        junction_data.push((source_id.clone(), target_id));
                    }
                }
                JsonValue::String(_) | JsonValue::Number(_) | JsonValue::Object(_) => {
                    // Single value - treat as array of one
                    let identifier = self.extract_identifier_from_value(rel_value)?;
                    let target_id = self
                        .get_or_create_id(&relationship.target_entity_uri, &identifier)
                        .await?;
                    junction_data.push((source_id.clone(), target_id));
                }
                _ => {
                    warn!(
                        "Unexpected value type for relationship '{}'",
                        relationship.label
                    );
                }
            }
        }

        info!(
            "Extracted {} junction table rows for relationship '{}'",
            junction_data.len(),
            relationship.label
        );

        Ok(junction_data)
    }

    async fn lookup_entity_id(&self, entity_uri: &str, identifier: &str) -> Result<Option<String>> {
        let cache = self.id_cache.read().await;
        Ok(cache
            .get(&(entity_uri.to_string(), identifier.to_string()))
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_simple_string_relationship() {
        let resolver = DefaultRelationshipResolver::new();

        // Pre-register a doctor
        resolver
            .register_entity("http://example.org/Doctor", "Dr. Smith", "100".to_string())
            .await;

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "doctor".to_string(),
            JsonValue::String("Dr. Smith".to_string()),
        );

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, Some("100".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_nested_object_relationship() {
        let resolver = DefaultRelationshipResolver::new();

        resolver
            .register_entity(
                "http://example.org/Department",
                "Cardiology",
                "50".to_string(),
            )
            .await;

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/worksIn".to_string(),
            label: "department".to_string(),
            target_entity_uri: "http://example.org/Department".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut dept_obj = Map::new();
        dept_obj.insert(
            "name".to_string(),
            JsonValue::String("Cardiology".to_string()),
        );

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert("department".to_string(), JsonValue::Object(dept_obj));

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, Some("50".to_string()));
    }

    #[tokio::test]
    async fn test_auto_generate_id() {
        let resolver = DefaultRelationshipResolver::new();

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "doctor".to_string(),
            JsonValue::String("Dr. New".to_string()),
        );

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, Some("1".to_string()));

        // Same identifier should return same ID
        let mut row2 = Map::new();
        row2.insert("id".to_string(), JsonValue::String("2".to_string()));
        row2.insert(
            "doctor".to_string(),
            JsonValue::String("Dr. New".to_string()),
        );

        let result2 = resolver
            .resolve_relationship(&row2, &relationship)
            .await
            .unwrap();

        assert_eq!(result2, Some("1".to_string()));
    }

    #[tokio::test]
    async fn test_extract_junction_data_many_to_many() {
        let resolver = DefaultRelationshipResolver::new();

        resolver
            .register_entity("http://example.org/Course", "Math", "10".to_string())
            .await;
        resolver
            .register_entity("http://example.org/Course", "Science", "11".to_string())
            .await;

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/enrolledIn".to_string(),
            label: "courses".to_string(),
            target_entity_uri: "http://example.org/Course".to_string(),
            cardinality: Cardinality::ManyToMany,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "courses".to_string(),
            JsonValue::Array(vec![
                JsonValue::String("Math".to_string()),
                JsonValue::String("Science".to_string()),
            ]),
        );

        let rows = vec![row];
        let junction_data = resolver
            .extract_junction_data(&rows, &relationship)
            .await
            .unwrap();

        assert_eq!(junction_data.len(), 2);
        assert!(junction_data.contains(&("1".to_string(), "10".to_string())));
        assert!(junction_data.contains(&("1".to_string(), "11".to_string())));
    }

    #[tokio::test]
    async fn test_extract_junction_data_single_value() {
        let resolver = DefaultRelationshipResolver::new();

        resolver
            .register_entity("http://example.org/Course", "Math", "10".to_string())
            .await;

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/enrolledIn".to_string(),
            label: "courses".to_string(),
            target_entity_uri: "http://example.org/Course".to_string(),
            cardinality: Cardinality::ManyToMany,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert("courses".to_string(), JsonValue::String("Math".to_string()));

        let rows = vec![row];
        let junction_data = resolver
            .extract_junction_data(&rows, &relationship)
            .await
            .unwrap();

        assert_eq!(junction_data.len(), 1);
        assert_eq!(junction_data[0], ("1".to_string(), "10".to_string()));
    }

    #[tokio::test]
    async fn test_lookup_entity_id() {
        let resolver = DefaultRelationshipResolver::new();

        resolver
            .register_entity("http://example.org/Doctor", "Dr. Smith", "100".to_string())
            .await;

        let id = resolver
            .lookup_entity_id("http://example.org/Doctor", "Dr. Smith")
            .await
            .unwrap();

        assert_eq!(id, Some("100".to_string()));

        let missing = resolver
            .lookup_entity_id("http://example.org/Doctor", "Dr. Unknown")
            .await
            .unwrap();

        assert_eq!(missing, None);
    }

    #[tokio::test]
    async fn test_extract_identifier_variants() {
        let resolver = DefaultRelationshipResolver::new();

        // Test string
        let s = JsonValue::String("test_id".to_string());
        assert_eq!(
            resolver.extract_identifier_from_value(&s).unwrap(),
            "test_id"
        );

        // Test number
        let n = JsonValue::Number(42.into());
        assert_eq!(resolver.extract_identifier_from_value(&n).unwrap(), "42");

        // Test object with "id" field
        let mut obj = Map::new();
        obj.insert("id".to_string(), JsonValue::String("obj_id".to_string()));
        assert_eq!(
            resolver
                .extract_identifier_from_value(&JsonValue::Object(obj))
                .unwrap(),
            "obj_id"
        );

        // Test object with "identifier" field
        let mut obj2 = Map::new();
        obj2.insert(
            "identifier".to_string(),
            JsonValue::String("obj_id2".to_string()),
        );
        assert_eq!(
            resolver
                .extract_identifier_from_value(&JsonValue::Object(obj2))
                .unwrap(),
            "obj_id2"
        );
    }

    #[tokio::test]
    async fn test_resolve_numeric_id_directly() {
        let resolver = DefaultRelationshipResolver::new();

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert("doctor".to_string(), JsonValue::Number(999.into()));

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        // Should auto-generate ID for "999"
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_case_insensitive_field_matching() {
        let resolver = DefaultRelationshipResolver::new();

        resolver
            .register_entity("http://example.org/Doctor", "Dr. Smith", "100".to_string())
            .await;

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "Doctor".to_string(), // Capital D
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "doctor".to_string(),
            JsonValue::String("Dr. Smith".to_string()),
        ); // lowercase

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, Some("100".to_string()));
    }

    #[tokio::test]
    async fn test_null_relationship_value() {
        let resolver = DefaultRelationshipResolver::new();

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert("doctor".to_string(), JsonValue::Null);

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_missing_relationship_field() {
        let resolver = DefaultRelationshipResolver::new();

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        // doctor field missing

        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_one_to_many_returns_none() {
        let resolver = DefaultRelationshipResolver::new();

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasOrders".to_string(),
            label: "orders".to_string(),
            target_entity_uri: "http://example.org/Order".to_string(),
            cardinality: Cardinality::OneToMany,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "orders".to_string(),
            JsonValue::Array(vec![JsonValue::String("order1".to_string())]),
        );

        // OneToMany should return None (FK is on the other side)
        let result = resolver
            .resolve_relationship(&row, &relationship)
            .await
            .unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_auto_generate_disabled() {
        let resolver = DefaultRelationshipResolver::new().with_auto_generate_ids(false);

        let relationship = RelationshipDefinition {
            relationship_uri: "http://example.org/hasDoctor".to_string(),
            label: "doctor".to_string(),
            target_entity_uri: "http://example.org/Doctor".to_string(),
            cardinality: Cardinality::ManyToOne,
        };

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("1".to_string()));
        row.insert(
            "doctor".to_string(),
            JsonValue::String("Dr. Unknown".to_string()),
        );

        let result = resolver.resolve_relationship(&row, &relationship).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("auto-generation is disabled"));
    }
}
