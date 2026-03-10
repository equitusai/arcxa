//! R2RML API Types
//!
//! Request and response DTOs for R2RML mapping endpoints.

use crate::mapping::r2rml::types::R2rmlMapping;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to create a new R2RML mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingRequest {
    /// Mapping definition (can be JSON or Turtle format)
    #[serde(flatten)]
    pub mapping: R2rmlMapping,

    /// Store mapping in RDF store
    #[serde(default = "default_true")]
    pub store_in_rdf: bool,
}

/// Response from creating a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingResponse {
    /// Generated mapping ID
    pub mapping_id: String,

    /// Mapping URI in RDF store
    pub mapping_uri: String,

    /// RDF graph URI where mapping is stored
    pub graph_uri: Option<String>,

    /// Validation status
    pub is_valid: bool,

    /// Validation errors (if any)
    pub validation_errors: Vec<String>,

    /// Link to retrieve mapping
    pub mapping_link: String,
}

/// Request to update an existing mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateMappingRequest {
    /// Updated mapping definition
    #[serde(flatten)]
    pub mapping: R2rmlMapping,
}

/// Response from updating a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateMappingResponse {
    /// Mapping ID
    pub mapping_id: String,

    /// Updated timestamp
    pub updated_at: DateTime<Utc>,

    /// Validation status
    pub is_valid: bool,

    /// Validation errors (if any)
    pub validation_errors: Vec<String>,
}

/// Response from getting a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetMappingResponse {
    /// Full R2RML mapping
    #[serde(flatten)]
    pub mapping: R2rmlMapping,

    /// R2RML Turtle representation
    pub r2rml_turtle: Option<String>,
}

/// Response from listing mappings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListMappingsResponse {
    /// List of mapping summaries
    pub mappings: Vec<MappingSummary>,

    /// Total count
    pub total_count: usize,
}

/// Mapping summary (for list view)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingSummary {
    /// Mapping ID
    pub mapping_id: String,

    /// Mapping URI
    pub mapping_uri: String,

    /// Source dataset
    pub source_dataset: String,

    /// Number of triples maps
    pub triples_maps_count: usize,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,

    /// Created by user
    pub created_by: Option<String>,
}

impl From<R2rmlMapping> for MappingSummary {
    fn from(mapping: R2rmlMapping) -> Self {
        Self {
            mapping_id: mapping.mapping_id.clone(),
            mapping_uri: mapping.get_mapping_uri(),
            source_dataset: mapping.source_dataset.clone(),
            triples_maps_count: mapping.triples_maps.len(),
            created_at: mapping.created_at,
            updated_at: mapping.updated_at,
            created_by: mapping.created_by.clone(),
        }
    }
}

/// Response from deleting a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeleteMappingResponse {
    /// Mapping ID
    pub mapping_id: String,

    /// Deletion timestamp
    pub deleted_at: DateTime<Utc>,

    /// Success message
    pub message: String,
}

/// Request to execute a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteMappingRequest {
    /// File ID from file library (enforces File Library First architecture)
    pub source_file_id: String,

    /// Target graph URI (where to store triples)
    pub target_graph: Option<String>,

    /// Output format (ntriples, turtle)
    #[serde(default = "default_output_format")]
    pub output_format: String,

    /// Store generated triples in RDF store
    #[serde(default = "default_true")]
    pub store_triples: bool,

    /// Return generated triples in response (may be large)
    #[serde(default)]
    pub include_triples: bool,
}

/// Response from executing a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteMappingResponse {
    /// Mapping ID
    pub mapping_id: String,

    /// Execution ID (for tracking)
    pub execution_id: String,

    /// Number of triples generated
    pub triples_generated: usize,

    /// Number of rows processed
    pub rows_processed: usize,

    /// Execution duration (seconds)
    pub duration_seconds: f64,

    /// Target graph URI
    pub target_graph: Option<String>,

    /// Generated triples (if requested)
    pub triples: Option<Vec<String>>,

    /// Execution timestamp
    pub executed_at: DateTime<Utc>,
}

/// Request to suggest R2RML mapping from profile
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingRequest {
    /// Dataset ID from profiling (optional if profile provided)
    pub dataset_id: Option<String>,

    /// Profile result (optional if dataset_id provided)
    pub profile: Option<crate::mapping::profiling::types::ProfileResult>,

    /// Base URI for generated entities
    pub base_uri: String,

    /// Default namespace for predicates (e.g., "schema", "dbo")
    #[serde(default = "default_namespace")]
    pub default_namespace: String,

    /// Use semantic types for predicate selection
    #[serde(default = "default_true")]
    pub use_semantic_types: bool,
}

/// Response from suggesting mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingResponse {
    /// Suggested R2RML mapping
    #[serde(flatten)]
    pub mapping: R2rmlMapping,

    /// Suggestion confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Suggestions per column
    pub column_suggestions: Vec<ColumnSuggestion>,

    /// R2RML Turtle representation
    pub r2rml_turtle: String,
}

/// Suggestion for a single column
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnSuggestion {
    /// Column name
    pub column_name: String,

    /// Suggested predicate URI
    pub suggested_predicate: String,

    /// Suggested XSD datatype
    pub suggested_datatype: Option<String>,

    /// Confidence score
    pub confidence: f64,

    /// Reasoning for suggestion
    pub reasoning: String,
}

/// Request to validate a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateMappingRequest {
    /// Mapping to validate
    #[serde(flatten)]
    pub mapping: R2rmlMapping,
}

/// Response from validating a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateMappingResponse {
    /// Mapping ID
    pub mapping_id: String,

    /// Is valid
    pub is_valid: bool,

    /// Validation errors
    pub errors: Vec<ValidationError>,

    /// Validation warnings
    pub warnings: Vec<ValidationWarning>,
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    /// Error code
    pub code: String,

    /// Error message
    pub message: String,

    /// Location in mapping (e.g., "TriplesMap.CustomerMap.SubjectMap")
    pub location: Option<String>,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationWarning {
    /// Warning code
    pub code: String,

    /// Warning message
    pub message: String,

    /// Location in mapping
    pub location: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_output_format() -> String {
    "ntriples".to_string()
}

fn default_namespace() -> String {
    "schema".to_string()
}
