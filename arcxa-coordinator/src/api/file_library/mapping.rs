//! CSV Field Mapping API
//!
//! Endpoints for intelligent field mapping suggestions between CSV files.

use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Import graphica-core field mapping types
use graphica_core::inference::{
    mapping::DataType as CoreDataType, profile_csv_file, CsvProfilerConfig, FieldMapper,
};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request for field mapping suggestions
#[derive(Debug, Deserialize)]
pub struct SuggestMappingsRequest {
    /// Configuration for CSV profiling
    pub profiler_config: Option<ProfilerConfigDto>,

    /// Minimum confidence threshold (0.0 - 1.0)
    pub min_confidence: Option<f64>,
}

/// Configuration for CSV profiling
#[derive(Debug, Deserialize)]
pub struct ProfilerConfigDto {
    /// Maximum rows to profile (None = all rows)
    pub max_rows: Option<usize>,

    /// CSV delimiter
    pub delimiter: Option<String>,

    /// Whether file has header
    pub has_header: Option<bool>,
}

/// Response with field mapping suggestions
#[derive(Debug, Serialize)]
pub struct SuggestMappingsResponse {
    /// Source file information
    pub source: FileInfoDto,

    /// Target file information
    pub target: FileInfoDto,

    /// All field mappings with confidence scores
    pub mappings: Vec<FieldMappingDto>,

    /// Auto-mapped fields (confidence ≥ 90%)
    pub auto_mapped: Vec<FieldMappingDto>,

    /// Recommended mappings (70-89% confidence)
    pub recommended: Vec<FieldMappingDto>,

    /// Possible mappings (50-69% confidence)
    pub possible: Vec<FieldMappingDto>,

    /// Timestamp of analysis
    pub analyzed_at: DateTime<Utc>,
}

/// File information summary
#[derive(Debug, Serialize)]
pub struct FileInfoDto {
    pub file_id: String,
    pub file_name: String,
    pub field_count: usize,
    pub total_rows: u64,
}

/// Field mapping with confidence scores
#[derive(Debug, Serialize)]
pub struct FieldMappingDto {
    /// Source field name
    pub source_field: String,

    /// Target field name
    pub target_field: String,

    /// Overall confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Breakdown of confidence scores
    pub scores: SimilarityScoresDto,

    /// Detected relationship type
    pub relationship: Option<RelationshipDto>,

    /// Source field metadata
    pub source_metadata: FieldMetadataDto,

    /// Target field metadata
    pub target_metadata: FieldMetadataDto,
}

/// Similarity scores breakdown
#[derive(Debug, Serialize)]
pub struct SimilarityScoresDto {
    /// Lexical similarity (name matching)
    pub lexical: f64,

    /// Statistical similarity (cardinality, distribution)
    pub statistical: f64,

    /// Schema context similarity (position, neighbors)
    pub schema_context: f64,

    /// Semantic similarity (if available)
    pub semantic: Option<f64>,
}

/// Relationship information
#[derive(Debug, Serialize)]
pub struct RelationshipDto {
    /// Type of relationship
    #[serde(rename = "type")]
    pub relationship_type: String,

    /// Direction of relationship
    pub direction: Option<String>,

    /// Cardinality (OneToOne, OneToMany, etc.)
    pub cardinality: Option<String>,
}

/// Field metadata summary
#[derive(Debug, Serialize)]
pub struct FieldMetadataDto {
    /// Field name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Total rows profiled
    pub total_rows: u64,

    /// Distinct value count
    pub distinct_count: u64,

    /// Null percentage (0.0 - 1.0)
    pub null_percentage: f64,

    /// Value distribution
    pub distribution: Option<ValueDistributionDto>,

    /// Sample values
    pub samples: Vec<String>,
}

/// Value distribution
#[derive(Debug, Serialize)]
pub struct ValueDistributionDto {
    pub min: Option<String>,
    pub max: Option<String>,
    pub median: Option<String>,
}

// ============================================================================
// Handler
// ============================================================================

/// Suggest field mappings between two CSV files
///
/// POST /api/v1/file-library/files/:source_id/suggest-mappings/:target_id
pub async fn suggest_csv_mappings(
    State(state): State<Arc<ApiState>>,
    Path((source_id, target_id)): Path<(String, String)>,
    Json(request): Json<SuggestMappingsRequest>,
) -> Result<Json<SuggestMappingsResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Suggesting field mappings: {} -> {}", source_id, target_id);

    // Get source and target files
    let source_file = storage
        .get_file(&source_id)
        .map_err(|e| ApiError::internal(format!("Failed to get source file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Source file not found: {}", source_id)))?;

    let target_file = storage
        .get_file(&target_id)
        .map_err(|e| ApiError::internal(format!("Failed to get target file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Target file not found: {}", target_id)))?;

    // Build profiler config
    let profiler_config = request
        .profiler_config
        .as_ref()
        .map(|cfg| CsvProfilerConfig {
            max_rows: cfg.max_rows,
            delimiter: cfg
                .delimiter
                .as_ref()
                .and_then(|d| d.chars().next())
                .unwrap_or(',') as u8,
            has_header: cfg.has_header.unwrap_or(true),
        })
        .unwrap_or_else(|| CsvProfilerConfig::default());

    // Profile source CSV
    tracing::debug!("Profiling source file: {}", source_file.file_path);
    let source_schema = profile_csv_file(
        std::path::Path::new(&source_file.file_path),
        profiler_config.clone(),
    )
    .map_err(|e| ApiError::internal(format!("Failed to profile source file: {}", e)))?;

    // Profile target CSV
    tracing::debug!("Profiling target file: {}", target_file.file_path);
    let target_schema = profile_csv_file(
        std::path::Path::new(&target_file.file_path),
        profiler_config,
    )
    .map_err(|e| ApiError::internal(format!("Failed to profile target file: {}", e)))?;

    // Run field mapper
    tracing::debug!("Running field mapper");
    let mapper = FieldMapper::new();
    let mapping_results = mapper
        .find_mappings(&source_schema, &target_schema)
        .map_err(|e| ApiError::internal(format!("Failed to find mappings: {}", e)))?;

    // Collect all similarities
    let all_similarities: Vec<_> = mapping_results
        .into_iter()
        .flat_map(|m| m.candidates)
        .collect();

    // Categorize mappings
    let suggestions = mapper.categorize_mappings(all_similarities.clone());

    // Apply minimum confidence filter if provided
    let min_confidence = request.min_confidence.unwrap_or(0.5);
    let filtered_mappings: Vec<_> = all_similarities
        .into_iter()
        .filter(|sim| sim.confidence >= min_confidence)
        .collect();

    // Convert to DTOs
    let mappings: Vec<FieldMappingDto> = filtered_mappings
        .iter()
        .map(|sim| similarity_to_dto(sim))
        .collect();

    let auto_mapped: Vec<FieldMappingDto> = suggestions
        .auto_mapped
        .iter()
        .map(|sim| similarity_to_dto(sim))
        .collect();

    let recommended: Vec<FieldMappingDto> = suggestions
        .recommended
        .iter()
        .map(|sim| similarity_to_dto(sim))
        .collect();

    let possible: Vec<FieldMappingDto> = suggestions
        .possible
        .iter()
        .map(|sim| similarity_to_dto(sim))
        .collect();

    tracing::info!(
        "Field mapping complete: {} total mappings ({} auto, {} recommended, {} possible)",
        mappings.len(),
        auto_mapped.len(),
        recommended.len(),
        possible.len()
    );

    Ok(Json(SuggestMappingsResponse {
        source: FileInfoDto {
            file_id: source_id,
            file_name: source_file.name,
            field_count: source_schema.fields.len(),
            total_rows: source_schema
                .fields
                .first()
                .map(|f| f.profile.total_rows)
                .unwrap_or(0),
        },
        target: FileInfoDto {
            file_id: target_id,
            file_name: target_file.name,
            field_count: target_schema.fields.len(),
            total_rows: target_schema
                .fields
                .first()
                .map(|f| f.profile.total_rows)
                .unwrap_or(0),
        },
        mappings,
        auto_mapped,
        recommended,
        possible,
        analyzed_at: Utc::now(),
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert FieldSimilarity from graphica-core to DTO
fn similarity_to_dto(sim: &graphica_core::inference::FieldSimilarity) -> FieldMappingDto {
    use graphica_core::inference::mapping::RelationshipType;

    let (relationship_type, direction, cardinality) = match &sim.relationship_type {
        RelationshipType::PrimaryForeignKey {
            direction,
            cardinality,
        } => (
            "PrimaryForeignKey".to_string(),
            Some(format!("{:?}", direction)),
            Some(format!("{:?}", cardinality)),
        ),
        RelationshipType::Duplicate => ("Duplicate".to_string(), None, None),
        RelationshipType::Derived { formula } => (
            "Derived".to_string(),
            None,
            formula.clone().map(|_| "Computed".to_string()),
        ),
        RelationshipType::Correlated {
            correlation_coefficient,
        } => (
            "Correlated".to_string(),
            None,
            Some(format!("r={:.2}", correlation_coefficient)),
        ),
        RelationshipType::Unrelated => ("Unrelated".to_string(), None, None),
    };

    FieldMappingDto {
        source_field: sim.source.column_name.clone(),
        target_field: sim.target.column_name.clone(),
        confidence: sim.confidence,
        scores: SimilarityScoresDto {
            lexical: sim.scores.lexical,
            statistical: sim.scores.statistical,
            schema_context: sim.scores.schema_context,
            semantic: sim.scores.semantic,
        },
        relationship: Some(RelationshipDto {
            relationship_type,
            direction,
            cardinality,
        }),
        source_metadata: field_metadata_to_dto(&sim.source),
        target_metadata: field_metadata_to_dto(&sim.target),
    }
}

/// Convert FieldMetadata from graphica-core to DTO
fn field_metadata_to_dto(
    field: &graphica_core::inference::mapping::FieldMetadata,
) -> FieldMetadataDto {
    FieldMetadataDto {
        name: field.column_name.clone(),
        data_type: data_type_to_string(&field.data_type),
        total_rows: field.profile.total_rows,
        distinct_count: field.profile.distinct_count,
        null_percentage: field.profile.null_percentage,
        distribution: Some(ValueDistributionDto {
            min: field.profile.distribution.min.clone(),
            max: field.profile.distribution.max.clone(),
            median: field.profile.distribution.median.clone(),
        }),
        samples: field.profile.samples.clone(),
    }
}

/// Convert DataType enum to string
fn data_type_to_string(data_type: &CoreDataType) -> String {
    match data_type {
        CoreDataType::Integer => "INTEGER".to_string(),
        CoreDataType::Float => "FLOAT".to_string(),
        CoreDataType::String => "STRING".to_string(),
        CoreDataType::Boolean => "BOOLEAN".to_string(),
        CoreDataType::Date => "DATE".to_string(),
        CoreDataType::DateTime => "DATETIME".to_string(),
        CoreDataType::Time => "TIME".to_string(),
        CoreDataType::Decimal { precision, scale } => {
            format!("DECIMAL({},{})", precision, scale)
        }
        CoreDataType::Binary => "BINARY".to_string(),
        CoreDataType::Json => "JSON".to_string(),
        CoreDataType::Unknown => "UNKNOWN".to_string(),
    }
}
