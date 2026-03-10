//! Ontology Mapping Type Definitions
//!
//! Core types for XSD-to-SQL mapping in workflow-based ontology processing,
//! entity definitions from ontologies, and relationship mappings.

use serde::{Deserialize, Serialize};

/// Cardinality of entity relationship
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

/// Normalization mode for schema generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NormalizationMode {
    /// Single table with all properties, no FK constraints
    Denormalized,
    /// Multiple tables with foreign key constraints
    Normalized,
    /// Mix based on cardinality (OneToOne/ManyToOne = denormalized, others = normalized)
    Hybrid,
}

/// Entity definition extracted from ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDefinition {
    /// URI of the entity class in the ontology
    pub entity_uri: String,

    /// Human-readable label
    pub label: String,

    /// Properties (datatype properties) of this entity
    pub properties: Vec<PropertyDefinition>,

    /// Relationships (object properties) to other entities
    pub relationships: Vec<RelationshipDefinition>,
}

/// Property definition (datatype property)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDefinition {
    /// URI of the property
    pub property_uri: String,

    /// Human-readable label
    pub label: String,

    /// Expected data type (XSD type or custom)
    pub range: String,

    /// Whether this property is required (from SHACL constraints)
    pub required: bool,

    /// Whether this property is multi-valued
    pub multi_valued: bool,
}

/// Relationship definition (object property)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipDefinition {
    /// URI of the relationship property
    pub relationship_uri: String,

    /// Human-readable label
    pub label: String,

    /// Target entity URI (the entity this relationship points to)
    pub target_entity_uri: String,

    /// Cardinality of the relationship
    pub cardinality: Cardinality,
}

/// Table schema definition for workflow-generated tables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Table name
    pub table_name: String,

    /// Column definitions
    pub columns: Vec<ColumnDefinition>,

    /// Primary key column names
    pub primary_key: Vec<String>,

    /// Foreign key constraints
    pub foreign_keys: Vec<ForeignKeyDefinition>,
}

impl TableSchema {
    /// Create a new table schema
    pub fn new(table_name: String) -> Self {
        Self {
            table_name,
            columns: Vec::new(),
            primary_key: Vec::new(),
            foreign_keys: Vec::new(),
        }
    }

    /// Add a column to the schema
    pub fn add_column(&mut self, column: ColumnDefinition) {
        self.columns.push(column);
    }

    /// Add a primary key column
    pub fn add_primary_key(&mut self, column_name: String) {
        if !self.primary_key.contains(&column_name) {
            self.primary_key.push(column_name);
        }
    }

    /// Add a foreign key constraint
    pub fn add_foreign_key(&mut self, fk: ForeignKeyDefinition) {
        self.foreign_keys.push(fk);
    }

    /// Get column by name
    pub fn get_column(&self, name: &str) -> Option<&ColumnDefinition> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Check if column exists
    pub fn has_column(&self, name: &str) -> bool {
        self.columns.iter().any(|c| c.name == name)
    }
}

/// Column definition for a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Column name
    pub name: String,

    /// SQL type (database-specific)
    pub sql_type: String,

    /// Whether the column is nullable
    pub nullable: bool,

    /// Whether this is a primary key column
    pub is_primary_key: bool,
}

impl ColumnDefinition {
    /// Create a new column definition
    pub fn new(name: String, sql_type: String, nullable: bool) -> Self {
        Self {
            name,
            sql_type,
            nullable,
            is_primary_key: false,
        }
    }

    /// Mark this column as a primary key
    pub fn as_primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self.nullable = false; // Primary keys cannot be null
        self
    }

    /// Set nullable flag
    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }
}

/// Foreign key constraint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyDefinition {
    /// Column in this table
    pub column: String,

    /// Referenced table name
    pub referenced_table: String,

    /// Referenced column name
    pub referenced_column: String,
}

impl ForeignKeyDefinition {
    /// Create a new foreign key definition
    pub fn new(column: String, referenced_table: String, referenced_column: String) -> Self {
        Self {
            column,
            referenced_table,
            referenced_column,
        }
    }
}

/// XSD type information extracted from ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XsdTypeInfo {
    /// XSD type URI (e.g., "http://www.w3.org/2001/XMLSchema#string")
    pub type_uri: String,

    /// Optional max length for string types
    pub max_length: Option<u32>,

    /// Optional precision for numeric types
    pub precision: Option<u32>,

    /// Optional scale for decimal types
    pub scale: Option<u32>,
}

impl XsdTypeInfo {
    /// Create a simple XSD type (no constraints)
    pub fn simple(type_uri: String) -> Self {
        Self {
            type_uri,
            max_length: None,
            precision: None,
            scale: None,
        }
    }

    /// Create XSD string type with max length
    pub fn string_with_length(max_length: u32) -> Self {
        Self {
            type_uri: "http://www.w3.org/2001/XMLSchema#string".to_string(),
            max_length: Some(max_length),
            precision: None,
            scale: None,
        }
    }

    /// Create XSD decimal type with precision and scale
    pub fn decimal(precision: u32, scale: u32) -> Self {
        Self {
            type_uri: "http://www.w3.org/2001/XMLSchema#decimal".to_string(),
            max_length: None,
            precision: Some(precision),
            scale: Some(scale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_schema_creation() {
        let mut schema = TableSchema::new("PATIENTS".to_string());

        schema.add_column(
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "name".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        schema.add_primary_key("id".to_string());

        assert_eq!(schema.table_name, "PATIENTS");
        assert_eq!(schema.columns.len(), 2);
        assert_eq!(schema.primary_key.len(), 1);
        assert_eq!(schema.primary_key[0], "id");
    }

    #[test]
    fn test_column_definition_primary_key() {
        let col =
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), true).as_primary_key();

        assert!(col.is_primary_key);
        assert!(!col.nullable); // Primary keys must not be null
    }

    #[test]
    fn test_foreign_key_definition() {
        let fk = ForeignKeyDefinition::new(
            "department_id".to_string(),
            "DEPARTMENTS".to_string(),
            "id".to_string(),
        );

        assert_eq!(fk.column, "department_id");
        assert_eq!(fk.referenced_table, "DEPARTMENTS");
        assert_eq!(fk.referenced_column, "id");
    }

    #[test]
    fn test_xsd_type_info_simple() {
        let xsd_type = XsdTypeInfo::simple("http://www.w3.org/2001/XMLSchema#string".to_string());

        assert_eq!(xsd_type.type_uri, "http://www.w3.org/2001/XMLSchema#string");
        assert!(xsd_type.max_length.is_none());
    }

    #[test]
    fn test_xsd_type_info_string_with_length() {
        let xsd_type = XsdTypeInfo::string_with_length(100);

        assert_eq!(xsd_type.type_uri, "http://www.w3.org/2001/XMLSchema#string");
        assert_eq!(xsd_type.max_length, Some(100));
    }

    #[test]
    fn test_xsd_type_info_decimal() {
        let xsd_type = XsdTypeInfo::decimal(19, 4);

        assert_eq!(
            xsd_type.type_uri,
            "http://www.w3.org/2001/XMLSchema#decimal"
        );
        assert_eq!(xsd_type.precision, Some(19));
        assert_eq!(xsd_type.scale, Some(4));
    }

    #[test]
    fn test_table_schema_get_column() {
        let mut schema = TableSchema::new("TEST".to_string());

        schema.add_column(ColumnDefinition::new(
            "id".to_string(),
            "INTEGER".to_string(),
            false,
        ));

        schema.add_column(ColumnDefinition::new(
            "name".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        assert!(schema.has_column("id"));
        assert!(schema.has_column("name"));
        assert!(!schema.has_column("email"));

        let id_col = schema.get_column("id").unwrap();
        assert_eq!(id_col.name, "id");
        assert_eq!(id_col.sql_type, "INTEGER");
    }
}
