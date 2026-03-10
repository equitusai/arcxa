//! Core types for ontology-driven DDL generation

use serde::{Deserialize, Serialize};

/// Configuration for ontology-aware DDL generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyDdlConfig {
    /// Skip ontology mapping (for quick prototyping)
    pub skip_ontology_mapping: bool,

    /// Minimum confidence threshold for auto-mapping
    pub min_mapping_confidence: f64,

    /// Use strict ontology constraints
    pub strict_constraints: bool,

    /// Record RDF lineage triples
    pub record_lineage: bool,

    /// Maximum candidates to consider per field
    pub max_candidates: usize,
}

impl Default for OntologyDdlConfig {
    fn default() -> Self {
        Self {
            skip_ontology_mapping: false,
            min_mapping_confidence: 0.7,
            strict_constraints: true,
            record_lineage: true,
            max_candidates: 5,
        }
    }
}

/// Mapping from source field to ontology term
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOntologyMapping {
    /// Unique field identifier
    pub field_id: String,

    /// Source field name
    pub field_name: String,

    /// Source table name
    pub table_name: String,

    /// Target ontology URI (e.g., http://schema.org/email)
    pub ontology_uri: String,

    /// Mapping confidence score (0.0 to 1.0)
    pub confidence: f64,

    /// Method used for mapping
    pub mapping_method: MappingMethod,

    /// Timestamp of mapping
    pub mapped_at: i64,
}

/// Method used to determine field→ontology mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MappingMethod {
    /// Statistical TF-IDF matching
    Statistical,
    /// Semantic transformer-based matching
    Semantic,
    /// Combined statistical + semantic
    Hybrid,
    /// User-provided explicit mapping
    Manual,
    /// Inferred from sample data patterns
    PatternInference,
    /// Matched against registry ontologies (custom + default)
    RegistryMatching,
}

/// SHACL constraint template derived from ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaclConstraintTemplate {
    /// Ontology URI this template is for
    pub ontology_uri: String,

    /// XSD datatype
    pub datatype: String,

    /// Maximum length for strings
    pub max_length: Option<u32>,

    /// Regex pattern for validation
    pub pattern: Option<String>,

    /// Minimum cardinality (0 = nullable, 1 = NOT NULL)
    pub min_count: Option<u32>,

    /// Maximum cardinality (1 = unique)
    pub max_count: Option<u32>,

    /// Minimum numeric value
    pub min_inclusive: Option<f64>,

    /// Maximum numeric value
    pub max_inclusive: Option<f64>,

    /// Enumeration values
    pub in_values: Option<Vec<String>>,

    /// Default value
    pub default_value: Option<String>,

    /// Whether to recommend an index
    pub recommended_index: bool,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Result of ontology-driven DDL generation
#[derive(Debug, Clone)]
pub struct OntologyDdlResult {
    /// Generated DDL statements
    pub ddl_statements: Vec<String>,

    /// Table definition (for versioning)
    pub table_definition: crate::mapping::ddl::TableDefinition,

    /// Field→ontology mappings applied
    pub ontology_mappings: Vec<FieldOntologyMapping>,

    /// Generated SHACL shape (for validation)
    pub shacl_shape: crate::mapping::ddl::shacl::NodeShape,

    /// RDF triples generated (if lineage enabled)
    pub rdf_triples: Option<Vec<(String, String, String)>>,

    /// Transformations derived from ontology mappings (GAP-003)
    pub transformations: Vec<super::transformation_rules::FieldTransformation>,
}
