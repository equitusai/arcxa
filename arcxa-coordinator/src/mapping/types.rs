//! # Field Mapping Core Types
//!
//! Domain types for the Advanced Field Mapping Engine.
//! These types represent schema fields, extracted features, mapping candidates,
//! and ontology terms.

use serde::{Deserialize, Serialize};

/// Represents a field from a source schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    /// Unique identifier for this field
    pub id: String,

    /// Original field name from the source
    pub name: String,

    /// Normalized field name (lowercase, no special chars)
    pub normalized_name: String,

    /// Data type (e.g., "VARCHAR", "INTEGER", "TIMESTAMP")
    pub data_type: String,

    /// Whether the field allows null values
    pub nullable: bool,

    /// Sample values from the field (for profiling)
    pub sample_values: Vec<String>,

    /// Source system identifier
    pub source_id: String,

    /// Table/collection name
    pub table_name: String,

    /// Optional description from source metadata
    pub description: Option<String>,

    /// Extracted features (computed by Schema Intelligence)
    pub features: Option<FieldFeatures>,
}

/// Extracted features from a schema field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldFeatures {
    /// TF-IDF tokens from field name
    pub name_tokens: Vec<String>,

    /// N-grams (2-grams and 3-grams) from field name
    pub name_ngrams: Vec<String>,

    /// Detected semantic patterns (email, phone, date, etc.)
    pub semantic_patterns: Vec<SemanticPattern>,

    /// Statistical properties from sample values
    pub statistics: FieldStatistics,

    /// Inferred semantic type
    pub inferred_type: Option<String>,

    /// Contextual information from table/schema
    pub context: FieldContext,
}

/// Detected semantic pattern in field values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticPattern {
    /// Pattern type (e.g., "email", "phone", "ssn", "date")
    pub pattern_type: String,

    /// Percentage of sample values matching this pattern (0.0 - 1.0)
    pub match_rate: f64,

    /// Confidence in this pattern detection (0.0 - 1.0)
    pub confidence: f64,
}

/// Statistical properties of a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldStatistics {
    /// Estimated distinct count (from HyperLogLog)
    pub distinct_count: usize,

    /// Total sample count
    pub sample_count: usize,

    /// Null rate (0.0 - 1.0)
    pub null_rate: f64,

    /// Average value length (for strings)
    pub avg_length: Option<f64>,

    /// Min/max values (for numeric types)
    pub min_value: Option<String>,
    pub max_value: Option<String>,

    /// Most common values with frequency
    pub top_values: Vec<(String, usize)>,
}

/// Contextual information about a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldContext {
    /// Table name this field belongs to
    pub table_name: String,

    /// Schema/database name
    pub schema_name: Option<String>,

    /// Related fields in the same table
    pub related_fields: Vec<String>,

    /// Whether this is a primary key
    pub is_primary_key: bool,

    /// Whether this is a foreign key
    pub is_foreign_key: bool,

    /// Foreign key reference (table.column)
    pub foreign_key_ref: Option<String>,
}

/// A candidate mapping from a source field to an ontology term
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingCandidate {
    /// Source field ID
    pub source_field_id: String,

    /// Target ontology term URI
    pub ontology_term_uri: String,

    /// Overall confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Breakdown of confidence by matcher type
    pub confidence_breakdown: ConfidenceBreakdown,

    /// Explanation of why this mapping was suggested
    pub explanation: String,

    /// Similar mappings from history (for context)
    pub similar_mappings: Vec<HistoricalMapping>,

    /// Suggested transformation (if any)
    pub transformation: Option<String>,
}

/// Confidence scores from each matcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    /// Statistical matcher score (TF-IDF, n-grams)
    pub statistical: f64,

    /// Semantic matcher score (embedding similarity) - Phase 2
    pub semantic: Option<f64>,

    /// GNN matcher score (graph structure) - Phase 3
    pub graph: Option<f64>,

    /// Symbolic matcher score (SPARQL reasoning) - Phase 4
    pub symbolic: Option<f64>,
}

/// Historical mapping for reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalMapping {
    /// Source field name from history
    pub source_field_name: String,

    /// Ontology term that was mapped
    pub ontology_term_uri: String,

    /// User who approved this mapping
    pub approved_by: String,

    /// Timestamp of approval
    pub approved_at: i64,

    /// Similarity score to current field (0.0 - 1.0)
    pub similarity: f64,
}

/// Ontology term representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyTerm {
    /// URI of the term (e.g., "http://schema.org/Person")
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Description of this term
    pub description: Option<String>,

    /// Parent class URIs
    pub parent_classes: Vec<String>,

    /// Alternative labels (synonyms)
    pub aliases: Vec<String>,

    /// Example values for this term
    pub examples: Vec<String>,

    /// Data type constraint (if any)
    pub data_type: Option<String>,

    /// Expected patterns (regex) for values
    pub value_patterns: Vec<String>,
}

/// Request to analyze a source schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeSchemaRequest {
    /// Source identifier
    pub source_id: String,

    /// Table/collection name
    pub table_name: String,

    /// Fields to analyze
    pub fields: Vec<SchemaFieldInput>,

    /// Number of sample values per field
    pub sample_size: Option<usize>,
}

/// Input field for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaFieldInput {
    /// Field name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Nullable
    pub nullable: bool,

    /// Sample values (optional, will be auto-sampled if not provided)
    pub sample_values: Option<Vec<String>>,

    /// Description (optional)
    pub description: Option<String>,
}

/// Response from schema analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeSchemaResponse {
    /// Analyzed fields with extracted features
    pub fields: Vec<SchemaField>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Request to get mapping candidates for a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCandidatesRequest {
    /// Field ID from analysis
    pub field_id: String,

    /// Maximum number of candidates to return
    pub top_k: Option<usize>,

    /// Minimum confidence threshold (0.0 - 1.0)
    pub min_confidence: Option<f64>,

    /// Filter to specific ontology namespaces
    pub ontology_filters: Option<Vec<String>>,
}

/// Response with mapping candidates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCandidatesResponse {
    /// Field ID
    pub field_id: String,

    /// Mapping candidates sorted by confidence (descending)
    pub candidates: Vec<MappingCandidate>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// User feedback on a mapping suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingFeedback {
    /// Field ID
    pub field_id: String,

    /// Ontology term URI that was selected
    pub selected_term_uri: Option<String>,

    /// Whether the user accepted the top suggestion
    pub accepted_top_suggestion: bool,

    /// User who provided feedback
    pub user_id: String,

    /// Optional notes
    pub notes: Option<String>,

    /// Timestamp
    pub timestamp: i64,
}

// ============================================================================
// Mapping Session Types - Phase 1 Implementation
// ============================================================================

/// Status of a mapping session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingSessionStatus {
    /// Initial state - mappings being generated
    Draft,
    /// Awaiting user review
    PendingReview,
    /// User has reviewed and approved
    Approved,
    /// Mappings stored in RDF
    Applied,
    /// Active and being used for imports
    Active,
    /// Session cancelled
    Cancelled,
}

impl MappingSessionStatus {
    /// Check if transition to new status is valid
    pub fn can_transition_to(&self, new_status: MappingSessionStatus) -> bool {
        use MappingSessionStatus::*;
        matches!(
            (self, new_status),
            (Draft, PendingReview)
                | (Draft, Cancelled)
                | (PendingReview, Approved)
                | (PendingReview, Draft)
                | (PendingReview, Cancelled)
                | (Approved, Applied)
                | (Applied, Active)
        )
    }
}

/// Approval status of a field mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldApprovalStatus {
    /// Not yet reviewed
    Pending,
    /// Auto-approved due to high confidence
    AutoApproved,
    /// User reviewed and approved
    Approved,
    /// User rejected this mapping
    Rejected,
    /// User selected a different mapping
    Modified,
}

/// A mapping session tracking schema-to-ontology mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingSession {
    /// Unique session identifier
    pub session_id: String,

    /// Data source ID this session is for
    pub source_id: String,

    /// Current status
    pub status: MappingSessionStatus,

    /// Tables being mapped
    pub tables: Vec<TableMapping>,

    /// User who initiated this session
    pub created_by: String,

    /// Creation timestamp
    pub created_at: i64,

    /// User who reviewed/approved
    pub reviewed_by: Option<String>,

    /// Review timestamp
    pub reviewed_at: Option<i64>,

    /// When mappings were applied to RDF
    pub applied_at: Option<i64>,

    /// Configuration used for analysis
    pub config: MappingSessionConfig,

    /// Summary statistics
    pub summary: MappingSessionSummary,
}

/// Configuration for a mapping session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingSessionConfig {
    /// Sample size used for analysis
    pub sample_size: usize,

    /// Confidence threshold for auto-approval
    pub auto_approve_threshold: f64,

    /// Minimum confidence to show as candidate
    pub min_confidence: f64,

    /// Maximum candidates per field
    pub max_candidates: usize,

    /// Ontology namespaces to use for mapping (if None, uses all active ontologies)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_namespaces: Option<Vec<String>>,
}

impl Default for MappingSessionConfig {
    fn default() -> Self {
        Self {
            sample_size: 1000,
            auto_approve_threshold: 0.95,
            min_confidence: 0.5,
            max_candidates: 10,
            ontology_namespaces: None, // Use all active ontologies by default
        }
    }
}

/// Summary statistics for a mapping session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingSessionSummary {
    /// Total fields analyzed
    pub total_fields: usize,

    /// Fields with at least one candidate
    pub fields_with_candidates: usize,

    /// Fields auto-approved
    pub auto_approved: usize,

    /// Fields pending review
    pub pending_review: usize,

    /// Fields approved by user
    pub user_approved: usize,

    /// Fields rejected
    pub rejected: usize,

    /// Fields modified
    pub modified: usize,

    /// Number of times this session was used in transformations
    #[serde(default)]
    pub transformations_executed: usize,

    /// Number of fields from this session used in transformations
    #[serde(default)]
    pub fields_used_in_transformations: usize,

    /// Number of successful transformations
    #[serde(default)]
    pub successful_transformations: usize,

    /// Number of failed transformations
    #[serde(default)]
    pub failed_transformations: usize,
}

impl Default for MappingSessionSummary {
    fn default() -> Self {
        Self {
            total_fields: 0,
            fields_with_candidates: 0,
            auto_approved: 0,
            pending_review: 0,
            user_approved: 0,
            rejected: 0,
            modified: 0,
            transformations_executed: 0,
            fields_used_in_transformations: 0,
            successful_transformations: 0,
            failed_transformations: 0,
        }
    }
}

/// Mapping for a single table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMapping {
    /// Table name
    pub table_name: String,

    /// Field mappings for this table
    pub field_mappings: Vec<FieldMappingState>,

    /// Table-level metadata
    pub metadata: Option<TableMetadata>,
}

/// Metadata about a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Table description
    pub description: Option<String>,

    /// Row count
    pub row_count: Option<u64>,

    /// Primary key columns
    pub primary_keys: Vec<String>,

    /// Foreign key relationships
    pub foreign_keys: Vec<ForeignKeyRef>,
}

/// Foreign key reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyRef {
    /// Source column
    pub column: String,

    /// Referenced table
    pub referenced_table: String,

    /// Referenced column
    pub referenced_column: String,
}

/// State of a single field mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingState {
    /// Field ID (unique identifier)
    pub field_id: String,

    /// Original field name
    pub field_name: String,

    /// Data type
    pub data_type: String,

    /// Sample values (for display)
    pub sample_values: Vec<String>,

    /// Generated mapping candidates
    pub candidates: Vec<MappingCandidate>,

    /// Selected mapping (if any)
    pub selected_mapping: Option<SelectedMapping>,

    /// Approval status
    pub approval_status: FieldApprovalStatus,

    /// User who approved/rejected
    pub reviewed_by: Option<String>,

    /// Review timestamp
    pub reviewed_at: Option<i64>,

    /// Review notes
    pub notes: Option<String>,
}

/// A selected mapping choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedMapping {
    /// Ontology term URI
    pub ontology_term_uri: String,

    /// Confidence score
    pub confidence: f64,

    /// Whether this was the top candidate
    pub was_top_candidate: bool,

    /// Optional transformation to apply
    pub transformation: Option<String>,
}

// ============================================================================
// API Request/Response Types for Mapping Session Workflow
// ============================================================================

/// Request to analyze a data source for mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeForMappingRequest {
    /// Tables to analyze (if None, analyze all tables)
    pub tables: Option<Vec<String>>,

    /// Sample size for field analysis
    pub sample_size: Option<usize>,

    /// Confidence threshold for auto-approval (0.0 - 1.0)
    pub auto_approve_threshold: Option<f64>,

    /// Minimum confidence to show as candidate
    pub min_confidence: Option<f64>,

    /// Maximum candidates per field
    pub max_candidates: Option<usize>,

    /// Ontology namespaces to use for mapping (if None, uses all active ontologies)
    /// Example: ["http://schema.org/", "http://example.com/retail#"]
    pub ontology_namespaces: Option<Vec<String>>,

    /// User initiating this session
    pub user_id: String,
}

/// Response from analyze-for-mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeForMappingResponse {
    /// Created session ID
    pub session_id: String,

    /// Summary statistics
    pub summary: MappingSessionSummary,

    /// Session status
    pub status: MappingSessionStatus,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Request to review and update field mappings in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewMappingsRequest {
    /// Field mapping updates
    pub field_mappings: Vec<FieldMappingUpdate>,

    /// User performing the review
    pub reviewed_by: String,

    /// If true, finalize review and move to Approved status
    pub finalize: bool,
}

/// Update for a single field mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingUpdate {
    /// Field ID to update
    pub field_id: String,

    /// Action to take
    pub action: ReviewAction,

    /// Selected mapping (if action is Approve or Modify)
    pub selected_mapping: Option<String>,

    /// Optional notes
    pub notes: Option<String>,
}

/// Review action for a field mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAction {
    /// Approve the top candidate
    Approve,
    /// Reject all candidates
    Reject,
    /// Select a different candidate
    Modify,
}

/// Response from review-mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewMappingsResponse {
    /// Updated session status
    pub status: MappingSessionStatus,

    /// Updated summary
    pub summary: MappingSessionSummary,

    /// Number of mappings approved (auto + user)
    pub approved_mappings: usize,

    /// Whether session is ready to apply
    pub ready_to_apply: bool,
}

/// Request to apply approved mappings to RDF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyMappingsRequest {
    /// Create a default dataset import configuration
    pub create_default_import: bool,
}

/// Response from apply-mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyMappingsResponse {
    /// Session status (should be Applied)
    pub status: MappingSessionStatus,

    /// Number of RDF triples stored
    pub rdf_triples_stored: usize,

    /// Whether session is ready for imports
    pub ready_for_import: bool,

    /// Default import configuration (if created)
    pub default_import_config: Option<serde_json::Value>,
}

// ============================================================================
// Data Import Types - Phase 2 Implementation
// ============================================================================

/// Request to import data using approved mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDataRequest {
    /// Batch size for import (rows per batch)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Target graph URI for entities (if None, uses default)
    pub target_graph: Option<String>,

    /// Table filter (if None, imports all tables)
    pub tables: Option<Vec<String>>,

    /// Row limit (for testing, if None imports all)
    pub limit: Option<usize>,

    /// User initiating import
    pub user_id: String,
}

fn default_batch_size() -> usize {
    1000
}

/// Response from data import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDataResponse {
    /// Import ID for tracking
    pub import_id: String,

    /// Session ID
    pub session_id: String,

    /// Import status
    pub status: ImportStatus,

    /// Statistics
    pub stats: ImportStatistics,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,

    /// Target graph URI where entities were stored
    pub target_graph: String,
}

/// Import execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportStatus {
    /// Import pending
    Pending,
    /// Import in progress
    InProgress,
    /// Import completed successfully
    Completed,
    /// Import failed
    Failed,
}

/// Statistics from data import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportStatistics {
    /// Total rows processed
    pub rows_processed: usize,

    /// Entities created
    pub entities_created: usize,

    /// RDF triples stored
    pub triples_stored: usize,

    /// Tables imported
    pub tables_imported: usize,

    /// Fields mapped
    pub fields_mapped: usize,

    /// Errors encountered
    pub errors: Vec<ImportError>,
}

/// Error during import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    /// Table name where error occurred
    pub table: String,

    /// Row number (if applicable)
    pub row: Option<usize>,

    /// Field name (if applicable)
    pub field: Option<String>,

    /// Error message
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_field_creation() {
        let field = SchemaField {
            id: "field_001".to_string(),
            name: "customer_email".to_string(),
            normalized_name: "customeremail".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: vec![
                "john@example.com".to_string(),
                "jane@example.com".to_string(),
            ],
            source_id: "pg_source".to_string(),
            table_name: "customers".to_string(),
            description: Some("Customer email address".to_string()),
            features: None,
        };

        assert_eq!(field.name, "customer_email");
        assert_eq!(field.sample_values.len(), 2);
    }

    #[test]
    fn test_mapping_candidate_confidence() {
        let candidate = MappingCandidate {
            source_field_id: "field_001".to_string(),
            ontology_term_uri: "http://schema.org/email".to_string(),
            confidence: 0.85,
            confidence_breakdown: ConfidenceBreakdown {
                statistical: 0.85,
                semantic: None,
                graph: None,
                symbolic: None,
            },
            explanation: "High lexical similarity to 'email'".to_string(),
            similar_mappings: vec![],
            transformation: None,
        };

        assert!(candidate.confidence > 0.8);
        assert_eq!(candidate.confidence_breakdown.statistical, 0.85);
    }

    #[test]
    fn test_ontology_term_serialization() {
        let term = OntologyTerm {
            uri: "http://schema.org/Person".to_string(),
            label: "Person".to_string(),
            description: Some("A person entity".to_string()),
            parent_classes: vec!["http://schema.org/Thing".to_string()],
            aliases: vec!["Human".to_string(), "Individual".to_string()],
            examples: vec!["John Doe".to_string()],
            data_type: None,
            value_patterns: vec![],
        };

        let json = serde_json::to_string(&term).unwrap();
        let deserialized: OntologyTerm = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.label, "Person");
        assert_eq!(deserialized.aliases.len(), 2);
    }
}
