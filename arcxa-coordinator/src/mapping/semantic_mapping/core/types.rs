//! # Unified Semantic Mapping Types
//!
//! Core type definitions shared across RDF and SQL output paths.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified semantic mapping set that can generate multiple output formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMappingSet {
    /// Source descriptor (CSV file, database table, etc.)
    pub source: SourceDescriptor,

    /// Field-level semantic mappings
    pub field_mappings: Vec<FieldSemanticMapping>,

    /// Entity type (ontology class)
    pub entity_type: OntologyClass,

    /// SHACL constraints derived from mappings
    pub constraints: Vec<ShaclConstraint>,

    /// Mapping metadata
    pub metadata: MappingMetadata,
}

impl SemanticMappingSet {
    /// Create a new semantic mapping set
    pub fn new(
        source: SourceDescriptor,
        entity_type: OntologyClass,
        metadata: MappingMetadata,
    ) -> Self {
        Self {
            source,
            field_mappings: Vec::new(),
            entity_type,
            constraints: Vec::new(),
            metadata,
        }
    }

    /// Add a field mapping
    pub fn add_field_mapping(&mut self, mapping: FieldSemanticMapping) {
        self.field_mappings.push(mapping);
    }

    /// Add a constraint
    pub fn add_constraint(&mut self, constraint: ShaclConstraint) {
        self.constraints.push(constraint);
    }
}

/// Source descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDescriptor {
    /// Source type (csv, parquet, database, etc.)
    pub source_type: String,

    /// Source location (file path, table name, etc.)
    pub location: String,

    /// Schema name (if database)
    pub schema: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Field-level semantic mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSemanticMapping {
    /// Source field name
    pub source_field: String,

    /// Source field index
    pub field_index: usize,

    /// Source data type
    pub source_type: String,

    /// Mapped ontology property
    pub ontology_property: OntologyProperty,

    /// Mapping confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Mapping strategy used (pattern, semantic, statistical, etc.)
    pub strategy: String,

    /// Derived constraints for this field
    pub constraints: Vec<FieldConstraint>,
}

/// Ontology class (entity type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyClass {
    /// Full URI (e.g., http://schema.org/Person)
    pub uri: String,

    /// Local name (e.g., "Person")
    pub local_name: String,

    /// Namespace prefix (e.g., "schema")
    pub prefix: String,

    /// Human-readable label
    pub label: Option<String>,

    /// Description
    pub description: Option<String>,
}

/// Ontology property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyProperty {
    /// Full URI (e.g., http://schema.org/email)
    pub uri: String,

    /// Local name (e.g., "email")
    pub local_name: String,

    /// Namespace prefix (e.g., "schema")
    pub prefix: String,

    /// Expected range/datatype
    pub range: String,

    /// Human-readable label
    pub label: Option<String>,

    /// Description
    pub description: Option<String>,
}

/// SHACL constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaclConstraint {
    /// Constraint type (minCount, maxCount, pattern, datatype, etc.)
    pub constraint_type: String,

    /// Target property path
    pub property_path: String,

    /// Constraint value (depends on type)
    pub value: ConstraintValue,

    /// Severity (Violation, Warning, Info)
    pub severity: String,
}

/// Field-level constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldConstraint {
    Required,
    Unique,
    MinLength(usize),
    MaxLength(usize),
    Pattern(String),
    MinValue(String),
    MaxValue(String),
    Datatype(String),
}

/// Constraint value (polymorphic)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstraintValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

/// Mapping metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingMetadata {
    /// Mapping identifier
    pub mapping_id: String,

    /// Base URI for generated entities
    pub base_uri: String,

    /// Target output format(s)
    pub output_formats: Vec<OutputFormat>,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Creator (user or system)
    pub created_by: Option<String>,

    /// Mapping version
    pub version: String,
}

/// Output format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    /// RDF triples (R2RML)
    Rdf,

    /// SQL DDL
    Sql,

    /// Both RDF and SQL (hybrid)
    Hybrid,
}
