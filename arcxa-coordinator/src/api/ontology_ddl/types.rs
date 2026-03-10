//! API Types for Ontology-Driven DDL Generation

use crate::mapping::ontology_ddl::types::{FieldOntologyMapping, MappingMethod};
use serde::{Deserialize, Serialize};

/// Request to generate ontology-driven DDL from discovered schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOntologyDdlRequest {
    /// Table name
    pub table_name: String,

    /// Column definitions
    pub columns: Vec<ColumnDiscoveryInput>,

    /// SQL dialect (postgresql, db2, oracle)
    #[serde(default = "default_dialect")]
    pub dialect: String,

    /// Configuration options
    #[serde(default)]
    pub config: OntologyDdlConfigInput,
}

/// Column discovery input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDiscoveryInput {
    /// Column name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Is primary key?
    #[serde(default)]
    pub primary_key: bool,

    /// Sample values (for pattern inference)
    #[serde(default)]
    pub sample_values: Vec<String>,

    /// Min value (for numeric types)
    pub min_value: Option<String>,

    /// Max value (for numeric types)
    pub max_value: Option<String>,

    /// Average length (for string types)
    pub avg_length: Option<f64>,

    /// Distinct count
    pub distinct_count: Option<i64>,
}

/// Configuration for ontology-driven DDL generation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OntologyDdlConfigInput {
    /// Skip ontology mapping (generate basic DDL)
    #[serde(default)]
    pub skip_ontology_mapping: bool,

    /// Minimum confidence threshold for auto-mapping
    #[serde(default = "default_min_confidence")]
    pub min_mapping_confidence: f64,

    /// Use strict ontology constraints
    #[serde(default = "default_true")]
    pub strict_constraints: bool,

    /// Record RDF lineage triples
    #[serde(default = "default_true")]
    pub record_lineage: bool,

    /// Maximum candidates to consider per field
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,
}

/// Response with generated DDL and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOntologyDdlResponse {
    /// Generated DDL statements
    pub ddl_statements: Vec<String>,

    /// Table name
    pub table_name: String,

    /// Number of columns
    pub column_count: usize,

    /// Ontology mappings that were applied
    pub ontology_mappings: Vec<OntologyMappingInfo>,

    /// SHACL shape summary
    pub shacl_shape_summary: ShaclShapeSummary,

    /// RDF lineage summary (if enabled)
    pub lineage_summary: Option<LineageSummaryInfo>,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Ontology mapping information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyMappingInfo {
    /// Field name
    pub field_name: String,

    /// Ontology URI (e.g., http://schema.org/email)
    pub ontology_uri: String,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Mapping method used
    pub mapping_method: String,
}

/// SHACL shape summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaclShapeSummary {
    /// Shape URI
    pub shape_uri: String,

    /// Target class
    pub target_class: String,

    /// Number of property shapes
    pub property_count: usize,

    /// Is closed shape?
    pub closed: bool,
}

/// Lineage summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageSummaryInfo {
    /// Total RDF triples generated
    pub total_triples: usize,

    /// Number of entities tracked
    pub entity_count: usize,

    /// Number of activities tracked
    pub activity_count: usize,

    /// Number of derivation relationships
    pub derivation_count: usize,
}

// Default functions
fn default_dialect() -> String {
    "postgresql".to_string()
}

fn default_min_confidence() -> f64 {
    0.7
}

fn default_true() -> bool {
    true
}

fn default_max_candidates() -> usize {
    5
}

impl Default for OntologyDdlConfigInput {
    fn default() -> Self {
        Self {
            skip_ontology_mapping: false,
            min_mapping_confidence: default_min_confidence(),
            strict_constraints: default_true(),
            record_lineage: default_true(),
            max_candidates: default_max_candidates(),
        }
    }
}

impl From<&FieldOntologyMapping> for OntologyMappingInfo {
    fn from(mapping: &FieldOntologyMapping) -> Self {
        Self {
            field_name: mapping.field_name.clone(),
            ontology_uri: mapping.ontology_uri.clone(),
            confidence: mapping.confidence,
            mapping_method: format!("{:?}", mapping.mapping_method),
        }
    }
}
