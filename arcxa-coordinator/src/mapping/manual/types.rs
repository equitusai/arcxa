// Manual Field Mapping Types - Core Domain Model
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Manual field mapping stored as RDF triples with fast RocksDB indexes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ManualFieldMapping {
    /// Unique ID for this mapping (gph:mapping/{uuid})
    pub id: String,

    /// Source context
    pub source_context: SourceContext,

    /// Target ontology field URI (e.g., "retail:customerFirstName")
    pub target_field_uri: String,

    /// Confidence (always 1.0 for manual mappings)
    pub confidence: f64,

    /// Metadata
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Optional rationale/notes from user
    pub notes: Option<String>,

    /// Usage statistics
    pub usage_stats: UsageStats,
}

/// Source context for field mapping
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, ToSchema)]
pub struct SourceContext {
    /// Optional source system ID (e.g., "sap_prod", "salesforce")
    pub source_id: Option<String>,

    /// Table/entity name (e.g., "customers", "orders")
    pub table_name: String,

    /// Field name as it appears in source (e.g., "fname1", "JSHHDD1")
    pub field_name: String,

    /// Optional field characteristics for better matching
    pub field_metadata: Option<FieldCharacteristics>,
}

/// Field characteristics to improve auto-suggestion
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, ToSchema)]
pub struct FieldCharacteristics {
    /// Detected data type from samples
    pub data_type: Option<String>,

    /// Sample values (first 5 distinct, anonymized)
    pub sample_values: Vec<String>,

    /// Pattern detected (e.g., "email", "phone", "date")
    pub detected_pattern: Option<String>,

    /// Statistical profile hash (for similarity matching)
    pub profile_hash: Option<String>,
}

/// Usage statistics for learning and suggestions
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct UsageStats {
    /// Number of times this mapping was applied
    pub apply_count: u64,

    /// Number of times suggested and accepted
    pub accept_count: u64,

    /// Number of times suggested and rejected
    pub reject_count: u64,

    /// Last used timestamp
    pub last_used: Option<DateTime<Utc>>,
}

/// Mapping suggestion with provenance
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingSuggestion {
    /// The suggested mapping
    pub mapping: ManualFieldMapping,

    /// Why this was suggested
    pub suggestion_reason: SuggestionReason,

    /// Relevance score (0.0 to 1.0)
    pub relevance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum SuggestionReason {
    /// Exact match on field name in same table
    ExactFieldMatch { previous_source: String },

    /// Similar field name pattern
    SimilarFieldName { similarity: f64 },

    /// Similar data characteristics
    SimilarDataProfile { profile_match: f64 },

    /// Frequently used mapping for this pattern
    FrequentPattern { usage_count: u64 },

    /// ML model suggestion (existing)
    MLModel { model_name: String, confidence: f64 },
}

/// Bulk import/export format
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingImportExport {
    pub version: String,
    pub exported_at: DateTime<Utc>,
    pub mappings: Vec<ManualFieldMapping>,
    pub statistics: ImportExportStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportExportStats {
    pub total_mappings: usize,
    pub unique_sources: usize,
    pub unique_tables: usize,
    pub unique_fields: usize,
}

/// Conflict resolution strategy for bulk import
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub enum ConflictResolution {
    /// Skip conflicting mappings, keep existing
    Skip,
    /// Overwrite existing mappings with imported ones
    Overwrite,
    /// Fail the entire import if any conflicts detected
    Fail,
    /// Merge usage stats, keep newer timestamp
    Merge,
}

impl Default for ConflictResolution {
    fn default() -> Self {
        ConflictResolution::Skip
    }
}

/// Import options for validation and conflict handling
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportOptions {
    /// How to handle conflicts (duplicate IDs or source contexts)
    pub conflict_resolution: ConflictResolution,

    /// Validate mapping structure before import
    pub validate_structure: bool,

    /// Check for duplicate source contexts
    pub check_duplicates: bool,

    /// Preserve original created_by user or override with importer
    pub preserve_creator: bool,

    /// Dry run - validate only, don't actually import
    pub dry_run: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            conflict_resolution: ConflictResolution::Skip,
            validate_structure: true,
            check_duplicates: true,
            preserve_creator: true,
            dry_run: false,
        }
    }
}

/// Validation error for a single mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    /// Mapping ID that failed
    pub mapping_id: String,

    /// Type of validation error
    pub error_type: ValidationErrorType,

    /// Human-readable error message
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub enum ValidationErrorType {
    /// Mapping ID is empty or invalid
    InvalidId,

    /// Source context is incomplete
    InvalidSourceContext,

    /// Target field URI is empty or malformed
    InvalidTargetUri,

    /// Confidence is not 1.0 for manual mapping
    InvalidConfidence,

    /// Duplicate mapping ID exists
    DuplicateId,

    /// Duplicate source context exists
    DuplicateSourceContext,

    /// Other validation failure
    Other,
}

/// Enhanced import result with detailed per-mapping status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportResult {
    /// Total mappings processed
    pub total: usize,

    /// Successfully imported
    pub successful: usize,

    /// Skipped due to conflicts
    pub skipped: usize,

    /// Failed validation
    pub failed: usize,

    /// Detailed errors per mapping
    pub errors: Vec<ValidationError>,

    /// IDs of successfully imported mappings
    pub imported_ids: Vec<String>,

    /// IDs of skipped mappings
    pub skipped_ids: Vec<String>,
}

impl Default for ImportResult {
    fn default() -> Self {
        ImportResult {
            total: 0,
            successful: 0,
            skipped: 0,
            failed: 0,
            errors: Vec::new(),
            imported_ids: Vec::new(),
            skipped_ids: Vec::new(),
        }
    }
}

/// RDF Triple representation for storage
impl ManualFieldMapping {
    /// Convert to RDF triples for storage in Oxigraph (3-tuple format)
    pub fn to_rdf_triples(&self) -> Vec<(String, String, String)> {
        let mapping_uri = format!("gph:mapping/{}", self.id);
        let mut triples = vec![];

        // Core mapping triple
        triples.push((
            mapping_uri.clone(),
            "rdf:type".to_string(),
            "gph:ManualFieldMapping".to_string(),
        ));

        // Source context
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}", self.source_context).as_bytes());
        let source_uri = format!("gph:source/{:x}", hasher.finalize());
        triples.push((
            mapping_uri.clone(),
            "gph:hasSource".to_string(),
            source_uri.clone(),
        ));

        triples.push((
            source_uri.clone(),
            "gph:tableName".to_string(),
            format!("\"{}\"", self.source_context.table_name),
        ));

        triples.push((
            source_uri.clone(),
            "gph:fieldName".to_string(),
            format!("\"{}\"", self.source_context.field_name),
        ));

        if let Some(ref source_id) = self.source_context.source_id {
            triples.push((
                source_uri,
                "gph:sourceId".to_string(),
                format!("\"{}\"", source_id),
            ));
        }

        // Target mapping
        triples.push((
            mapping_uri.clone(),
            "gph:mapsTo".to_string(),
            self.target_field_uri.clone(),
        ));

        // Confidence
        triples.push((
            mapping_uri.clone(),
            "gph:confidence".to_string(),
            format!("\"{}\"^^xsd:double", self.confidence),
        ));

        // Provenance
        triples.push((
            mapping_uri.clone(),
            "prov:wasAttributedTo".to_string(),
            format!("gph:user/{}", self.created_by),
        ));

        triples.push((
            mapping_uri.clone(),
            "prov:generatedAtTime".to_string(),
            format!("\"{}\"^^xsd:dateTime", self.created_at.to_rfc3339()),
        ));

        // Usage stats
        triples.push((
            mapping_uri.clone(),
            "gph:applyCount".to_string(),
            format!("\"{}\"^^xsd:integer", self.usage_stats.apply_count),
        ));

        triples
    }
}

/// Index keys for RocksDB column families
pub struct MappingIndexKeys;

impl MappingIndexKeys {
    /// Primary key: source_context -> mapping_id
    pub fn source_to_mapping(ctx: &SourceContext) -> Vec<u8> {
        format!(
            "{}:{}:{}",
            ctx.source_id.as_ref().unwrap_or(&"*".to_string()),
            ctx.table_name,
            ctx.field_name
        )
        .into_bytes()
    }

    /// Reverse index: target_uri -> [mapping_ids]
    pub fn target_to_mappings(target_uri: &str) -> Vec<u8> {
        format!("target:{}", target_uri).into_bytes()
    }

    /// Pattern index: field_pattern -> [mapping_ids]
    pub fn pattern_to_mappings(pattern: &str) -> Vec<u8> {
        format!("pattern:{}", pattern.to_lowercase()).into_bytes()
    }

    /// User index: user -> [mapping_ids]
    pub fn user_to_mappings(user: &str) -> Vec<u8> {
        format!("user:{}", user).into_bytes()
    }
}
