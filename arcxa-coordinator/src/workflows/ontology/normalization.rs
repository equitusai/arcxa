//! Table Normalization Strategies for Ontology-Driven Schema Generation
//!
//! Provides three normalization strategies:
//! - **Denormalized**: Single table with all properties and relationships as FK columns
//! - **Normalized**: Separate tables for entities and junction tables for relationships
//! - **Hybrid**: Mix of denormalized and normalized based on cardinality
//!
//! Each strategy generates database schemas from ontology entity definitions,
//! handling properties, relationships, and constraints appropriately.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::{debug, info, warn};

use super::schema_provider::OntologySchemaProvider;
use super::types::*;

/// Maximum identifier length for database objects
const MAX_IDENTIFIER_LENGTH: usize = 128;

/// Reserved SQL keywords to avoid in identifiers
const RESERVED_WORDS: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "DROP",
    "ALTER",
    "TABLE",
    "INDEX",
    "VIEW",
    "FROM",
    "WHERE",
    "JOIN",
    "ORDER",
    "GROUP",
    "BY",
    "HAVING",
    "UNION",
    "INTO",
    "VALUES",
    "SET",
    "PRIMARY",
    "FOREIGN",
    "KEY",
    "REFERENCES",
    "CONSTRAINT",
    "UNIQUE",
    "NOT",
    "NULL",
    "DEFAULT",
    "CASCADE",
    "RESTRICT",
    "USER",
    "DATE",
    "TIME",
    "TIMESTAMP",
    "INTERVAL",
];

/// Trait for table normalization strategies
#[async_trait]
pub trait NormalizationStrategy: Send + Sync {
    /// Generate table schemas from entity definition
    async fn generate_schemas(&self, entity_def: &EntityDefinition) -> Result<Vec<TableSchema>>;

    /// Get the normalization mode
    fn get_mode(&self) -> NormalizationMode;
}

/// Denormalized strategy: single flat table
pub struct DenormalizedStrategy;

impl DenormalizedStrategy {
    /// Create a new denormalized strategy
    pub fn new() -> Self {
        Self
    }
}

impl Default for DenormalizedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NormalizationStrategy for DenormalizedStrategy {
    async fn generate_schemas(&self, entity_def: &EntityDefinition) -> Result<Vec<TableSchema>> {
        info!(
            "Generating denormalized schema for entity: {}",
            entity_def.label
        );

        let schema = create_denormalized_schema(entity_def)?;
        Ok(vec![schema])
    }

    fn get_mode(&self) -> NormalizationMode {
        NormalizationMode::Denormalized
    }
}

/// Normalized strategy: separate tables with junction tables
pub struct NormalizedStrategy;

impl NormalizedStrategy {
    /// Create a new normalized strategy
    pub fn new() -> Self {
        Self
    }
}

impl Default for NormalizedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NormalizationStrategy for NormalizedStrategy {
    async fn generate_schemas(&self, entity_def: &EntityDefinition) -> Result<Vec<TableSchema>> {
        info!(
            "Generating normalized schemas for entity: {}",
            entity_def.label
        );

        let schemas = create_normalized_schemas(entity_def).await?;

        // Topologically sort to ensure dependencies are created first
        let sorted = topological_sort(schemas)?;

        Ok(sorted)
    }

    fn get_mode(&self) -> NormalizationMode {
        NormalizationMode::Normalized
    }
}

/// Hybrid strategy: mix based on cardinality
pub struct HybridStrategy {
    /// Threshold for relationship complexity (number of related entities)
    complexity_threshold: usize,
}

impl HybridStrategy {
    /// Create a new hybrid strategy
    pub fn new() -> Self {
        Self {
            complexity_threshold: 10,
        }
    }

    /// Create with custom complexity threshold
    pub fn with_threshold(complexity_threshold: usize) -> Self {
        Self {
            complexity_threshold,
        }
    }
}

impl Default for HybridStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NormalizationStrategy for HybridStrategy {
    async fn generate_schemas(&self, entity_def: &EntityDefinition) -> Result<Vec<TableSchema>> {
        info!(
            "Generating hybrid schemas for entity: {} (threshold: {})",
            entity_def.label, self.complexity_threshold
        );

        let schemas = create_hybrid_schemas(entity_def, self.complexity_threshold).await?;

        // Topologically sort for proper creation order
        let sorted = topological_sort(schemas)?;

        Ok(sorted)
    }

    fn get_mode(&self) -> NormalizationMode {
        NormalizationMode::Hybrid
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a denormalized table schema with all properties and relationships as columns
fn create_denormalized_schema(entity: &EntityDefinition) -> Result<TableSchema> {
    let table_name = sanitize_identifier(&entity.label)?;
    let mut schema = TableSchema::new(table_name.clone());

    // Add auto-generated ID column as primary key
    schema.add_column(
        ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
    );
    schema.add_primary_key("id".to_string());

    // Add all property columns
    for prop in &entity.properties {
        let col_name = sanitize_identifier(&prop.label)?;
        let sql_type = map_xsd_to_sql(&prop.range)?;
        let nullable = !prop.required;

        if prop.multi_valued {
            // For multi-valued properties in denormalized mode, use JSON or TEXT
            warn!(
                "Multi-valued property '{}' stored as JSON in denormalized table",
                prop.label
            );
            schema.add_column(ColumnDefinition::new(
                col_name,
                "TEXT".to_string(),
                nullable,
            ));
        } else {
            schema.add_column(ColumnDefinition::new(col_name, sql_type, nullable));
        }
    }

    // Add relationship columns as foreign keys
    for rel in &entity.relationships {
        let col_name = format!("{}_id", sanitize_identifier(&rel.label)?);

        match &rel.cardinality {
            Cardinality::OneToOne | Cardinality::ManyToOne => {
                // Single FK column
                schema.add_column(ColumnDefinition::new(
                    col_name.clone(),
                    "INTEGER".to_string(),
                    true, // Relationships typically nullable
                ));

                let target_table =
                    sanitize_identifier(&extract_entity_label(&rel.target_entity_uri))?;
                schema.add_foreign_key(ForeignKeyDefinition::new(
                    col_name,
                    target_table,
                    "id".to_string(),
                ));
            }
            Cardinality::OneToMany | Cardinality::ManyToMany => {
                // Store as JSON array or comma-separated IDs
                warn!(
                    "Many-relationship '{}' stored as JSON array in denormalized table",
                    rel.label
                );
                schema.add_column(ColumnDefinition::new(col_name, "TEXT".to_string(), true));
            }
        }
    }

    debug!(
        "Created denormalized schema '{}' with {} columns",
        table_name,
        schema.columns.len()
    );

    Ok(schema)
}

/// Create normalized schemas with main table and junction tables
async fn create_normalized_schemas(entity: &EntityDefinition) -> Result<Vec<TableSchema>> {
    let mut schemas = Vec::new();

    // 1. Create main entity table with only direct properties
    let table_name = sanitize_identifier(&entity.label)?;
    let mut main_schema = TableSchema::new(table_name.clone());

    // Primary key
    main_schema.add_column(
        ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
    );
    main_schema.add_primary_key("id".to_string());

    // Add property columns (no relationships)
    for prop in &entity.properties {
        let col_name = sanitize_identifier(&prop.label)?;
        let sql_type = map_xsd_to_sql(&prop.range)?;
        let nullable = !prop.required;

        if prop.multi_valued {
            warn!(
                "Multi-valued property '{}' requires separate table in normalized mode",
                prop.label
            );
            // Create a separate value table for multi-valued properties
            let value_table = create_multi_value_table(&table_name, &col_name, &sql_type)?;
            schemas.push(value_table);
        } else {
            main_schema.add_column(ColumnDefinition::new(col_name, sql_type, nullable));
        }
    }

    schemas.push(main_schema);

    // 2. Create junction tables for all relationships
    for rel in &entity.relationships {
        let junction = create_junction_table(&table_name, rel).await?;
        schemas.push(junction);
    }

    debug!(
        "Created {} normalized schemas for entity '{}'",
        schemas.len(),
        entity.label
    );

    Ok(schemas)
}

/// Create hybrid schemas: FK columns for simple relationships, junction tables for complex
async fn create_hybrid_schemas(
    entity: &EntityDefinition,
    _complexity_threshold: usize,
) -> Result<Vec<TableSchema>> {
    let mut schemas = Vec::new();

    let table_name = sanitize_identifier(&entity.label)?;
    let mut main_schema = TableSchema::new(table_name.clone());

    // Primary key
    main_schema.add_column(
        ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
    );
    main_schema.add_primary_key("id".to_string());

    // Add all properties
    for prop in &entity.properties {
        let col_name = sanitize_identifier(&prop.label)?;
        let sql_type = map_xsd_to_sql(&prop.range)?;
        let nullable = !prop.required;

        if prop.multi_valued {
            // Multi-valued properties get their own table
            let value_table = create_multi_value_table(&table_name, &col_name, &sql_type)?;
            schemas.push(value_table);
        } else {
            main_schema.add_column(ColumnDefinition::new(col_name, sql_type, nullable));
        }
    }

    // Handle relationships based on cardinality
    for rel in &entity.relationships {
        match &rel.cardinality {
            Cardinality::OneToOne | Cardinality::ManyToOne => {
                // Simple relationships: add FK column to main table
                let col_name = format!("{}_id", sanitize_identifier(&rel.label)?);
                main_schema.add_column(ColumnDefinition::new(
                    col_name.clone(),
                    "INTEGER".to_string(),
                    true,
                ));

                let target_table =
                    sanitize_identifier(&extract_entity_label(&rel.target_entity_uri))?;
                main_schema.add_foreign_key(ForeignKeyDefinition::new(
                    col_name,
                    target_table,
                    "id".to_string(),
                ));
            }
            Cardinality::OneToMany | Cardinality::ManyToMany => {
                // Complex relationships: create junction table
                let junction = create_junction_table(&table_name, rel).await?;
                schemas.push(junction);
            }
        }
    }

    schemas.insert(0, main_schema);

    debug!(
        "Created {} hybrid schemas for entity '{}'",
        schemas.len(),
        entity.label
    );

    Ok(schemas)
}

/// Create a junction table for a relationship
async fn create_junction_table(
    entity_table: &str,
    relationship: &RelationshipDefinition,
) -> Result<TableSchema> {
    let rel_name = sanitize_identifier(&relationship.label)?;
    let target_table = sanitize_identifier(&extract_entity_label(&relationship.target_entity_uri))?;

    let junction_name = format!("{}_{}", entity_table, rel_name);
    validate_identifier(&junction_name)?;

    let mut schema = TableSchema::new(junction_name.clone());

    // Composite primary key: source_id + target_id
    schema.add_column(
        ColumnDefinition::new("source_id".to_string(), "INTEGER".to_string(), false)
            .as_primary_key(),
    );
    schema.add_column(
        ColumnDefinition::new("target_id".to_string(), "INTEGER".to_string(), false)
            .as_primary_key(),
    );

    schema.add_primary_key("source_id".to_string());
    schema.add_primary_key("target_id".to_string());

    // Foreign keys
    schema.add_foreign_key(ForeignKeyDefinition::new(
        "source_id".to_string(),
        entity_table.to_string(),
        "id".to_string(),
    ));

    schema.add_foreign_key(ForeignKeyDefinition::new(
        "target_id".to_string(),
        target_table.clone(),
        "id".to_string(),
    ));

    debug!(
        "Created junction table '{}' linking {} to {}",
        junction_name, entity_table, target_table
    );

    Ok(schema)
}

/// Create a table for multi-valued properties
fn create_multi_value_table(
    entity_table: &str,
    property_name: &str,
    sql_type: &str,
) -> Result<TableSchema> {
    let table_name = format!("{}_{}_values", entity_table, property_name);
    validate_identifier(&table_name)?;

    let mut schema = TableSchema::new(table_name.clone());

    // Foreign key to parent entity
    schema.add_column(ColumnDefinition::new(
        "entity_id".to_string(),
        "INTEGER".to_string(),
        false,
    ));

    // Value column
    schema.add_column(ColumnDefinition::new(
        "value".to_string(),
        sql_type.to_string(),
        false,
    ));

    // Sequence number for ordering
    schema.add_column(ColumnDefinition::new(
        "seq".to_string(),
        "INTEGER".to_string(),
        false,
    ));

    // Composite primary key
    schema.add_primary_key("entity_id".to_string());
    schema.add_primary_key("seq".to_string());

    // Foreign key constraint
    schema.add_foreign_key(ForeignKeyDefinition::new(
        "entity_id".to_string(),
        entity_table.to_string(),
        "id".to_string(),
    ));

    Ok(schema)
}

/// Topologically sort schemas based on foreign key dependencies
fn topological_sort(mut schemas: Vec<TableSchema>) -> Result<Vec<TableSchema>> {
    // Build dependency graph
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();

    // Initialize
    for schema in &schemas {
        graph.insert(schema.table_name.clone(), Vec::new());
        in_degree.insert(schema.table_name.clone(), 0);
    }

    // Build edges: if table A has FK to table B, then B -> A (B must be created first)
    for schema in &schemas {
        for fk in &schema.foreign_keys {
            // Don't add edge if it's a self-reference
            if fk.referenced_table != schema.table_name {
                // Only add edge if referenced table is in our schema set
                if graph.contains_key(&fk.referenced_table) {
                    graph
                        .get_mut(&fk.referenced_table)
                        .unwrap()
                        .push(schema.table_name.clone());
                    *in_degree.get_mut(&schema.table_name).unwrap() += 1;
                }
            }
        }
    }

    // Kahn's algorithm for topological sort
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut result = Vec::new();

    // Start with tables that have no dependencies
    for (table, &degree) in &in_degree {
        if degree == 0 {
            queue.push_back(table.clone());
        }
    }

    while let Some(table) = queue.pop_front() {
        result.push(table.clone());

        if let Some(neighbors) = graph.get(&table) {
            for neighbor in neighbors {
                let degree = in_degree.get_mut(neighbor).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    // Check for cycles
    if result.len() != schemas.len() {
        let unsorted: Vec<_> = schemas
            .iter()
            .filter(|s| !result.contains(&s.table_name))
            .map(|s| s.table_name.clone())
            .collect();

        warn!(
            "Circular dependencies detected in tables: {:?}. Using original order.",
            unsorted
        );
        return Ok(schemas);
    }

    // Reorder schemas based on topological order
    let mut sorted_schemas = Vec::new();
    for table_name in result {
        if let Some(pos) = schemas.iter().position(|s| s.table_name == table_name) {
            sorted_schemas.push(schemas.remove(pos));
        }
    }

    debug!("Topologically sorted {} schemas", sorted_schemas.len());

    Ok(sorted_schemas)
}

/// Sanitize identifier to be SQL-safe
fn sanitize_identifier(label: &str) -> Result<String> {
    // Convert to uppercase and replace invalid characters
    let sanitized: String = label
        .to_uppercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Ensure starts with letter or underscore
    let sanitized = if sanitized
        .chars()
        .next()
        .map_or(false, |c| c.is_ascii_digit())
    {
        format!("T_{}", sanitized)
    } else {
        sanitized
    };

    validate_identifier(&sanitized)?;

    Ok(sanitized)
}

/// Validate identifier meets SQL naming requirements
fn validate_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty() {
        return Err(anyhow!("Identifier cannot be empty"));
    }

    if identifier.len() > MAX_IDENTIFIER_LENGTH {
        return Err(anyhow!(
            "Identifier '{}' exceeds maximum length of {} characters",
            identifier,
            MAX_IDENTIFIER_LENGTH
        ));
    }

    let first_char = identifier.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(anyhow!(
            "Identifier '{}' must start with letter or underscore",
            identifier
        ));
    }

    // Check reserved words
    if RESERVED_WORDS.contains(&identifier) {
        return Err(anyhow!(
            "Identifier '{}' is a reserved SQL keyword",
            identifier
        ));
    }

    Ok(())
}

/// Map XSD type to SQL type
fn map_xsd_to_sql(xsd_type: &str) -> Result<String> {
    let sql_type = match xsd_type {
        "http://www.w3.org/2001/XMLSchema#string" => "VARCHAR(255)",
        "http://www.w3.org/2001/XMLSchema#int" => "INTEGER",
        "http://www.w3.org/2001/XMLSchema#integer" => "INTEGER",
        "http://www.w3.org/2001/XMLSchema#long" => "BIGINT",
        "http://www.w3.org/2001/XMLSchema#decimal" => "DECIMAL(19,4)",
        "http://www.w3.org/2001/XMLSchema#double" => "DOUBLE",
        "http://www.w3.org/2001/XMLSchema#float" => "FLOAT",
        "http://www.w3.org/2001/XMLSchema#boolean" => "BOOLEAN",
        "http://www.w3.org/2001/XMLSchema#date" => "DATE",
        "http://www.w3.org/2001/XMLSchema#dateTime" => "TIMESTAMP",
        "http://www.w3.org/2001/XMLSchema#time" => "TIME",
        _ => {
            // Try to extract simple type name
            if let Some(simple_name) = xsd_type.split('#').last() {
                match simple_name.to_lowercase().as_str() {
                    "string" => "VARCHAR(255)",
                    "int" | "integer" => "INTEGER",
                    "long" => "BIGINT",
                    "decimal" => "DECIMAL(19,4)",
                    "double" => "DOUBLE",
                    "float" => "FLOAT",
                    "boolean" => "BOOLEAN",
                    "date" => "DATE",
                    "datetime" => "TIMESTAMP",
                    "time" => "TIME",
                    _ => {
                        warn!(
                            "Unknown XSD type '{}', defaulting to VARCHAR(255)",
                            xsd_type
                        );
                        "VARCHAR(255)"
                    }
                }
            } else {
                "VARCHAR(255)"
            }
        }
    };

    Ok(sql_type.to_string())
}

/// Extract entity label from URI (last segment)
fn extract_entity_label(uri: &str) -> String {
    uri.split(&['/', '#'][..])
        .last()
        .unwrap_or("Entity")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Mock Schema Provider for Testing
    // ========================================================================

    struct MockSchemaProvider {
        entities: HashMap<String, EntityDefinition>,
    }

    impl MockSchemaProvider {
        fn new() -> Self {
            Self {
                entities: HashMap::new(),
            }
        }

        fn add_entity(&mut self, entity: EntityDefinition) {
            self.entities.insert(entity.entity_uri.clone(), entity);
        }
    }

    #[async_trait]
    impl OntologySchemaProvider for MockSchemaProvider {
        async fn get_entity_definition(&self, entity_uri: &str) -> Result<EntityDefinition> {
            self.entities
                .get(entity_uri)
                .cloned()
                .ok_or_else(|| anyhow!("Entity not found: {}", entity_uri))
        }

        async fn get_all_entities(&self) -> Result<Vec<String>> {
            Ok(self.entities.keys().cloned().collect())
        }

        async fn resolve_relationships(
            &self,
            entity_uri: &str,
        ) -> Result<Vec<RelationshipDefinition>> {
            Ok(self
                .entities
                .get(entity_uri)
                .map(|e| e.relationships.clone())
                .unwrap_or_default())
        }

        async fn entity_exists(&self, entity_uri: &str) -> Result<bool> {
            Ok(self.entities.contains_key(entity_uri))
        }
    }

    // ========================================================================
    // Test Helpers
    // ========================================================================

    fn create_test_entity() -> EntityDefinition {
        EntityDefinition {
            entity_uri: "http://example.org/Patient".to_string(),
            label: "Patient".to_string(),
            properties: vec![
                PropertyDefinition {
                    property_uri: "http://example.org/name".to_string(),
                    label: "name".to_string(),
                    range: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                    required: true,
                    multi_valued: false,
                },
                PropertyDefinition {
                    property_uri: "http://example.org/age".to_string(),
                    label: "age".to_string(),
                    range: "http://www.w3.org/2001/XMLSchema#int".to_string(),
                    required: false,
                    multi_valued: false,
                },
            ],
            relationships: vec![RelationshipDefinition {
                relationship_uri: "http://example.org/primaryDoctor".to_string(),
                label: "primaryDoctor".to_string(),
                target_entity_uri: "http://example.org/Doctor".to_string(),
                cardinality: Cardinality::ManyToOne,
            }],
        }
    }

    fn create_test_entity_with_many_to_many() -> EntityDefinition {
        EntityDefinition {
            entity_uri: "http://example.org/Patient".to_string(),
            label: "Patient".to_string(),
            properties: vec![PropertyDefinition {
                property_uri: "http://example.org/name".to_string(),
                label: "name".to_string(),
                range: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                required: true,
                multi_valued: false,
            }],
            relationships: vec![RelationshipDefinition {
                relationship_uri: "http://example.org/diagnoses".to_string(),
                label: "diagnoses".to_string(),
                target_entity_uri: "http://example.org/Diagnosis".to_string(),
                cardinality: Cardinality::ManyToMany,
            }],
        }
    }

    // ========================================================================
    // Validation Tests
    // ========================================================================

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("PATIENT").is_ok());
        assert!(validate_identifier("_PATIENT").is_ok());
        assert!(validate_identifier("PATIENT_123").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("123PATIENT").is_err());
        assert!(validate_identifier("SELECT").is_err());
        assert!(validate_identifier("TABLE").is_err());
    }

    #[test]
    fn test_validate_identifier_too_long() {
        let long_name = "A".repeat(MAX_IDENTIFIER_LENGTH + 1);
        assert!(validate_identifier(&long_name).is_err());
    }

    #[test]
    fn test_sanitize_identifier() {
        assert_eq!(sanitize_identifier("patient").unwrap(), "PATIENT");
        assert_eq!(sanitize_identifier("Patient Name").unwrap(), "PATIENT_NAME");
        assert_eq!(sanitize_identifier("123Patient").unwrap(), "T_123PATIENT");
        assert_eq!(
            sanitize_identifier("has-diagnosis").unwrap(),
            "HAS_DIAGNOSIS"
        );
    }

    // ========================================================================
    // XSD Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_xsd_to_sql() {
        assert_eq!(
            map_xsd_to_sql("http://www.w3.org/2001/XMLSchema#string").unwrap(),
            "VARCHAR(255)"
        );
        assert_eq!(
            map_xsd_to_sql("http://www.w3.org/2001/XMLSchema#int").unwrap(),
            "INTEGER"
        );
        assert_eq!(
            map_xsd_to_sql("http://www.w3.org/2001/XMLSchema#boolean").unwrap(),
            "BOOLEAN"
        );
        assert_eq!(
            map_xsd_to_sql("http://www.w3.org/2001/XMLSchema#dateTime").unwrap(),
            "TIMESTAMP"
        );
    }

    #[test]
    fn test_map_xsd_to_sql_simple_name() {
        assert_eq!(map_xsd_to_sql("string").unwrap(), "VARCHAR(255)");
        assert_eq!(map_xsd_to_sql("int").unwrap(), "INTEGER");
        assert_eq!(map_xsd_to_sql("custom#string").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_map_xsd_to_sql_unknown() {
        assert_eq!(
            map_xsd_to_sql("http://example.org/CustomType").unwrap(),
            "VARCHAR(255)"
        );
    }

    // ========================================================================
    // Denormalized Strategy Tests
    // ========================================================================

    #[tokio::test]
    async fn test_denormalized_strategy_basic() {
        let entity = create_test_entity();
        let strategy = DenormalizedStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        assert_eq!(schemas.len(), 1);
        let schema = &schemas[0];

        assert_eq!(schema.table_name, "PATIENT");
        assert!(schema.has_column("id"));
        assert!(schema.has_column("NAME"));
        assert!(schema.has_column("AGE"));
        assert!(schema.has_column("PRIMARYDOCTOR_id"));

        assert_eq!(schema.primary_key.len(), 1);
        assert_eq!(schema.primary_key[0], "id");

        assert_eq!(schema.foreign_keys.len(), 1);
        assert_eq!(schema.foreign_keys[0].column, "PRIMARYDOCTOR_id");
    }

    #[tokio::test]
    async fn test_denormalized_strategy_mode() {
        let strategy = DenormalizedStrategy::new();
        assert_eq!(strategy.get_mode(), NormalizationMode::Denormalized);
    }

    #[tokio::test]
    async fn test_denormalized_many_to_many() {
        let entity = create_test_entity_with_many_to_many();
        let strategy = DenormalizedStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        assert_eq!(schemas.len(), 1);
        let schema = &schemas[0];

        // Many-to-many stored as TEXT in denormalized mode
        assert!(schema.has_column("DIAGNOSES_id"));
        let col = schema.get_column("DIAGNOSES_id").unwrap();
        assert_eq!(col.sql_type, "TEXT");
    }

    // ========================================================================
    // Normalized Strategy Tests
    // ========================================================================

    #[tokio::test]
    async fn test_normalized_strategy_basic() {
        let entity = create_test_entity();
        let strategy = NormalizedStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        // Should have: main table + junction table for relationship
        assert_eq!(schemas.len(), 2);

        // Main table should only have properties
        let main_table = schemas.iter().find(|s| s.table_name == "PATIENT").unwrap();
        assert!(main_table.has_column("id"));
        assert!(main_table.has_column("NAME"));
        assert!(main_table.has_column("AGE"));
        assert!(!main_table.has_column("PRIMARYDOCTOR_ID")); // No FK in normalized

        // Junction table
        let junction = schemas
            .iter()
            .find(|s| s.table_name.contains("PRIMARYDOCTOR"))
            .unwrap();
        assert!(junction.has_column("source_id"));
        assert!(junction.has_column("target_id"));
        assert_eq!(junction.foreign_keys.len(), 2);
    }

    #[tokio::test]
    async fn test_normalized_strategy_mode() {
        let strategy = NormalizedStrategy::new();
        assert_eq!(strategy.get_mode(), NormalizationMode::Normalized);
    }

    #[tokio::test]
    async fn test_normalized_many_to_many() {
        let entity = create_test_entity_with_many_to_many();
        let strategy = NormalizedStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        // Main table + junction table
        assert_eq!(schemas.len(), 2);

        let junction = schemas
            .iter()
            .find(|s| s.table_name == "PATIENT_DIAGNOSES")
            .unwrap();

        assert_eq!(junction.primary_key.len(), 2);
        assert!(junction.primary_key.contains(&"source_id".to_string()));
        assert!(junction.primary_key.contains(&"target_id".to_string()));
    }

    // ========================================================================
    // Hybrid Strategy Tests
    // ========================================================================

    #[tokio::test]
    async fn test_hybrid_strategy_simple_relationship() {
        let entity = create_test_entity();
        let strategy = HybridStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        // ManyToOne should be FK column in main table
        assert_eq!(schemas.len(), 1);

        let main_table = &schemas[0];
        assert!(main_table.has_column("PRIMARYDOCTOR_id"));
        assert_eq!(main_table.foreign_keys.len(), 1);
    }

    #[tokio::test]
    async fn test_hybrid_strategy_complex_relationship() {
        let entity = create_test_entity_with_many_to_many();
        let strategy = HybridStrategy::new();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();

        // ManyToMany should create junction table
        assert_eq!(schemas.len(), 2);

        let junction = schemas
            .iter()
            .find(|s| s.table_name.contains("DIAGNOSES"))
            .unwrap();

        assert!(junction.has_column("source_id"));
        assert!(junction.has_column("target_id"));
    }

    #[tokio::test]
    async fn test_hybrid_strategy_mode() {
        let strategy = HybridStrategy::new();
        assert_eq!(strategy.get_mode(), NormalizationMode::Hybrid);
    }

    #[tokio::test]
    async fn test_hybrid_strategy_custom_threshold() {
        let strategy = HybridStrategy::with_threshold(5);
        let entity = create_test_entity();

        let schemas = strategy.generate_schemas(&entity).await.unwrap();
        assert!(!schemas.is_empty());
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_create_denormalized_schema() {
        let entity = create_test_entity();
        let schema = create_denormalized_schema(&entity).unwrap();

        assert_eq!(schema.table_name, "PATIENT");
        assert!(schema.has_column("id"));
        assert_eq!(schema.columns.len(), 4); // id, name, age, primaryDoctor_id
    }

    #[tokio::test]
    async fn test_create_junction_table() {
        let rel = RelationshipDefinition {
            relationship_uri: "http://example.org/treats".to_string(),
            label: "treats".to_string(),
            target_entity_uri: "http://example.org/Patient".to_string(),
            cardinality: Cardinality::ManyToMany,
        };

        let junction = create_junction_table("DOCTOR", &rel).await.unwrap();

        assert_eq!(junction.table_name, "DOCTOR_TREATS");
        assert!(junction.has_column("source_id"));
        assert!(junction.has_column("target_id"));
        assert_eq!(junction.foreign_keys.len(), 2);
    }

    #[test]
    fn test_extract_entity_label() {
        assert_eq!(
            extract_entity_label("http://example.org/Patient"),
            "Patient"
        );
        assert_eq!(extract_entity_label("http://example.org#Doctor"), "Doctor");
        assert_eq!(extract_entity_label("Diagnosis"), "Diagnosis");
    }

    // ========================================================================
    // Topological Sort Tests
    // ========================================================================

    #[test]
    fn test_topological_sort_simple() {
        let mut schemas = vec![
            TableSchema {
                table_name: "PATIENT".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![ForeignKeyDefinition::new(
                    "doctor_id".to_string(),
                    "DOCTOR".to_string(),
                    "id".to_string(),
                )],
            },
            TableSchema {
                table_name: "DOCTOR".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![],
            },
        ];

        let sorted = topological_sort(schemas).unwrap();

        // DOCTOR should come before PATIENT
        assert_eq!(sorted[0].table_name, "DOCTOR");
        assert_eq!(sorted[1].table_name, "PATIENT");
    }

    #[test]
    fn test_topological_sort_complex() {
        let schemas = vec![
            TableSchema {
                table_name: "C".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![ForeignKeyDefinition::new(
                    "b_id".to_string(),
                    "B".to_string(),
                    "id".to_string(),
                )],
            },
            TableSchema {
                table_name: "B".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![ForeignKeyDefinition::new(
                    "a_id".to_string(),
                    "A".to_string(),
                    "id".to_string(),
                )],
            },
            TableSchema {
                table_name: "A".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![],
            },
        ];

        let sorted = topological_sort(schemas).unwrap();

        // Order should be: A, B, C
        assert_eq!(sorted[0].table_name, "A");
        assert_eq!(sorted[1].table_name, "B");
        assert_eq!(sorted[2].table_name, "C");
    }

    #[test]
    fn test_topological_sort_circular_dependency() {
        let schemas = vec![
            TableSchema {
                table_name: "A".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![ForeignKeyDefinition::new(
                    "b_id".to_string(),
                    "B".to_string(),
                    "id".to_string(),
                )],
            },
            TableSchema {
                table_name: "B".to_string(),
                columns: vec![],
                primary_key: vec![],
                foreign_keys: vec![ForeignKeyDefinition::new(
                    "a_id".to_string(),
                    "A".to_string(),
                    "id".to_string(),
                )],
            },
        ];

        // Should return original order with warning
        let sorted = topological_sort(schemas).unwrap();
        assert_eq!(sorted.len(), 2);
    }

    #[test]
    fn test_topological_sort_self_reference() {
        let schemas = vec![TableSchema {
            table_name: "EMPLOYEE".to_string(),
            columns: vec![],
            primary_key: vec![],
            foreign_keys: vec![ForeignKeyDefinition::new(
                "manager_id".to_string(),
                "EMPLOYEE".to_string(),
                "id".to_string(),
            )],
        }];

        let sorted = topological_sort(schemas).unwrap();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].table_name, "EMPLOYEE");
    }

    #[test]
    fn test_topological_sort_external_reference() {
        // References to tables not in the schema set should be ignored
        let schemas = vec![TableSchema {
            table_name: "PATIENT".to_string(),
            columns: vec![],
            primary_key: vec![],
            foreign_keys: vec![ForeignKeyDefinition::new(
                "doctor_id".to_string(),
                "EXTERNAL_DOCTOR".to_string(), // Not in schema set
                "id".to_string(),
            )],
        }];

        let sorted = topological_sort(schemas).unwrap();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].table_name, "PATIENT");
    }
}
