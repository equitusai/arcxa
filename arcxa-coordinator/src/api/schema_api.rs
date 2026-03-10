//! # Schema API Module
//!
//! Phase 2: Cross-source mapping and type conversion API endpoints.
//!
//! This module provides REST endpoints for:
//! - Cross-source field mapping (PostgreSQL ↔ CSV ↔ DB2 ↔ Oracle)
//! - Type conversion validation and SQL generation
//! - Unified schema profiling using V2 connectors

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::api::ApiState;

// ============================================================================
// DTOs - Request/Response Types
// ============================================================================

/// Request to map fields between two datasources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSourceMappingRequest {
    /// Source datasource ID (from catalog)
    pub source_datasource_id: String,

    /// Source table name
    pub source_table: String,

    /// Target datasource ID (from catalog)
    pub target_datasource_id: String,

    /// Target table name
    pub target_table: String,

    /// Minimum confidence threshold (default: 0.5)
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,

    /// Auto-map threshold (default: 0.9)
    #[serde(default = "default_auto_map_threshold")]
    pub auto_map_threshold: f64,

    /// Recommend threshold (default: 0.7)
    #[serde(default = "default_recommend_threshold")]
    pub recommend_threshold: f64,
}

fn default_min_confidence() -> f64 {
    0.5
}

fn default_auto_map_threshold() -> f64 {
    0.9
}

fn default_recommend_threshold() -> f64 {
    0.7
}

/// Field mapping candidate with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingCandidate {
    /// Source field name
    pub source_field: String,

    /// Source data type
    pub source_type: String,

    /// Target field name
    pub target_field: String,

    /// Target data type
    pub target_type: String,

    /// Mapping confidence score
    pub confidence: f64,

    /// Whether types match exactly
    pub types_match: bool,

    /// Type conversion info (if types don't match)
    pub conversion: Option<TypeConversionInfo>,
}

/// Type conversion information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeConversionInfo {
    /// Whether conversion is safe (no data loss)
    pub is_safe: bool,

    /// Whether conversion is lossy (may lose data)
    pub is_lossy: bool,

    /// Warnings about the conversion
    pub warnings: Vec<String>,

    /// Suggested conversion function (generic SQL)
    pub conversion_function: Option<String>,
}

/// Response from cross-source mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSourceMappingResponse {
    /// Source schema information
    pub source_schema: SchemaInfo,

    /// Target schema information
    pub target_schema: SchemaInfo,

    /// Auto-mapped fields (confidence >= 0.9)
    pub auto_mapped: Vec<FieldMappingCandidate>,

    /// Recommended mappings (0.7 <= confidence < 0.9)
    pub recommended: Vec<FieldMappingCandidate>,

    /// Possible mappings (0.5 <= confidence < 0.7)
    pub possible: Vec<FieldMappingCandidate>,

    /// Summary statistics
    pub summary: MappingSummary,
}

/// Schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    /// Datasource ID
    pub datasource_id: String,

    /// Table name
    pub table_name: String,

    /// Source type (PostgreSQL, CsvFile, DB2, etc.)
    pub source_type: String,

    /// Number of fields
    pub field_count: usize,

    /// List of field names and types
    pub fields: Vec<FieldInfo>,
}

/// Field information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    /// Field name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Whether field is nullable
    pub nullable: bool,

    /// Whether field is primary key
    pub is_primary_key: bool,
}

/// Mapping summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingSummary {
    /// Total fields in source
    pub total_source_fields: usize,

    /// Total fields in target
    pub total_target_fields: usize,

    /// Number of auto-mapped fields
    pub auto_mapped_count: usize,

    /// Number of recommended mappings
    pub recommended_count: usize,

    /// Number of possible mappings
    pub possible_count: usize,

    /// Number of unmapped fields
    pub unmapped_count: usize,

    /// Number of conversions required
    pub conversions_required: usize,

    /// Number of lossy conversions
    pub lossy_conversions: usize,
}

/// Request to validate a type conversion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateConversionRequest {
    /// Source data type
    pub source_type: String,

    /// Target data type
    pub target_type: String,
}

/// Response from conversion validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateConversionResponse {
    /// Source type (normalized)
    pub source_type: String,

    /// Target type (normalized)
    pub target_type: String,

    /// Whether conversion is safe
    pub is_safe: bool,

    /// Whether conversion is lossy
    pub is_lossy: bool,

    /// Whether conversion is invalid
    pub is_invalid: bool,

    /// Validation warnings
    pub warnings: Vec<String>,

    /// Recommended SQL function (generic)
    pub recommended_sql: Option<String>,
}

/// Request to generate conversion SQL for a specific dialect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateConversionSqlRequest {
    /// Source data type
    pub source_type: String,

    /// Target data type
    pub target_type: String,

    /// SQL dialect (PostgreSQL, MySQL, Oracle, DB2, SQLServer, Snowflake)
    pub dialect: String,
}

/// Response with dialect-specific conversion SQL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateConversionSqlResponse {
    /// SQL dialect
    pub dialect: String,

    /// Conversion SQL function
    pub sql: String,

    /// Whether conversion is safe
    pub is_safe: bool,

    /// Whether conversion is lossy
    pub is_lossy: bool,

    /// Warnings
    pub warnings: Vec<String>,
}

/// Request to profile a datasource and return unified schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSchemaRequest {
    /// Datasource ID (from catalog)
    pub datasource_id: String,

    /// Table name
    pub table_name: String,

    /// Sample size for profiling (default: 1000)
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,
}

fn default_sample_size() -> usize {
    1000
}

/// Response with unified schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSchemaResponse {
    /// Datasource ID
    pub datasource_id: String,

    /// Table name
    pub table_name: String,

    /// Source type
    pub source_type: String,

    /// Number of fields
    pub field_count: usize,

    /// Fields with profiling information
    pub fields: Vec<ProfiledField>,

    /// Row count (if available)
    pub row_count: Option<u64>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Profiled field with statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfiledField {
    /// Field name
    pub name: String,

    /// Universal data type
    pub data_type: String,

    /// Whether field is nullable
    pub nullable: bool,

    /// Field position
    pub position: usize,

    /// Profiling statistics (if available)
    pub profile: Option<FieldProfileStats>,
}

/// Field profiling statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldProfileStats {
    /// Number of distinct values
    pub distinct_count: u64,

    /// Total rows profiled
    pub total_rows: u64,

    /// Null percentage (0.0 - 1.0)
    pub null_percentage: f64,

    /// Sample values
    pub samples: Vec<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /api/v1/schema/cross-source/map
///
/// Map fields between two datasources using cross-source mapper.
pub async fn map_cross_source(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CrossSourceMappingRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use super::schema_api_impl::{parse_type_string, source_type_to_string, type_to_string};
    use graphica_core::schema::{CrossSourceMapper, UnifiedField, UnifiedSchema};

    info!(
        "Cross-source mapping: {} -> {}",
        request.source_datasource_id, request.target_datasource_id
    );

    // Get datasource catalog
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Datasource catalog not available".to_string(),
        )
    })?;

    // Get source datasource
    let source_ds = catalog
        .get_source(&request.source_datasource_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("Source datasource not found: {}", e),
            )
        })?;

    // Get target datasource
    let target_ds = catalog
        .get_source(&request.target_datasource_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("Target datasource not found: {}", e),
            )
        })?;

    // Infer schemas for both datasources
    let source_schema_def = catalog
        .infer_schema(
            &request.source_datasource_id,
            Some(&request.source_table),
            1000,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to infer source schema: {}", e),
            )
        })?;

    let target_schema_def = catalog
        .infer_schema(
            &request.target_datasource_id,
            Some(&request.target_table),
            1000,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to infer target schema: {}", e),
            )
        })?;

    // Find the requested tables
    let source_table_def = source_schema_def
        .tables
        .iter()
        .find(|t| t.name == request.source_table)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Source table '{}' not found", request.source_table),
            )
        })?;

    let target_table_def = target_schema_def
        .tables
        .iter()
        .find(|t| t.name == request.target_table)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Target table '{}' not found", request.target_table),
            )
        })?;

    // Convert catalog schema to UnifiedSchema for cross-source mapper
    // Parse source_type string to SourceType enum
    let source_type = match source_ds.source.source_type.as_str() {
        "PostgreSQL" => graphica_core::schema::SourceType::PostgreSQL,
        "MySQL" => graphica_core::schema::SourceType::MySQL,
        "Oracle" => graphica_core::schema::SourceType::Oracle,
        "DB2" => graphica_core::schema::SourceType::DB2,
        "CsvFile" => graphica_core::schema::SourceType::CsvFile,
        _ => graphica_core::schema::SourceType::CsvFile, // Default fallback
    };

    let source_schema = UnifiedSchema {
        id: format!("schema_{}", uuid::Uuid::new_v4()),
        name: request.source_table.clone(),
        source_type,
        source_ref: request.source_datasource_id.clone(),
        fields: source_table_def
            .columns
            .iter()
            .enumerate()
            .map(|(pos, col)| {
                let data_type = parse_type_string(&col.data_type)
                    .unwrap_or(graphica_core::schema::UniversalDataType::Unknown);
                UnifiedField::new(col.name.clone(), data_type)
            })
            .collect(),
        row_count: source_table_def.estimated_rows,
        size_bytes: None,
        last_profiled: Some(chrono::Utc::now()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        metadata: std::collections::HashMap::new(),
    };

    let target_type = match target_ds.source.source_type.as_str() {
        "PostgreSQL" => graphica_core::schema::SourceType::PostgreSQL,
        "MySQL" => graphica_core::schema::SourceType::MySQL,
        "Oracle" => graphica_core::schema::SourceType::Oracle,
        "DB2" => graphica_core::schema::SourceType::DB2,
        "CsvFile" => graphica_core::schema::SourceType::CsvFile,
        _ => graphica_core::schema::SourceType::CsvFile, // Default fallback
    };

    let target_schema = UnifiedSchema {
        id: format!("schema_{}", uuid::Uuid::new_v4()),
        name: request.target_table.clone(),
        source_type: target_type,
        source_ref: request.target_datasource_id.clone(),
        fields: target_table_def
            .columns
            .iter()
            .enumerate()
            .map(|(pos, col)| {
                let data_type = parse_type_string(&col.data_type)
                    .unwrap_or(graphica_core::schema::UniversalDataType::Unknown);
                UnifiedField::new(col.name.clone(), data_type)
            })
            .collect(),
        row_count: target_table_def.estimated_rows,
        size_bytes: None,
        last_profiled: Some(chrono::Utc::now()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        metadata: std::collections::HashMap::new(),
    };

    // Create CrossSourceMapper with custom config
    use graphica_core::inference::mapping::MapperConfig;
    let config = MapperConfig {
        min_confidence: request.min_confidence,
        auto_map_threshold: request.auto_map_threshold,
        recommend_threshold: request.recommend_threshold,
        ..Default::default()
    };

    let mapper = CrossSourceMapper::with_config(config);

    // Execute cross-source mapping
    let mapping_result = mapper
        .map_unified_schemas(&source_schema, &target_schema)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Mapping failed: {}", e),
            )
        })?;

    // Helper to convert inference DataType to string
    let inference_type_to_string =
        |data_type: &graphica_core::inference::mapping::types::DataType| -> String {
            use graphica_core::inference::mapping::types::DataType;
            match data_type {
                DataType::Integer => "Integer".to_string(),
                DataType::Float => "Float".to_string(),
                DataType::String => "String".to_string(),
                DataType::Boolean => "Boolean".to_string(),
                DataType::Date => "Date".to_string(),
                DataType::DateTime => "DateTime".to_string(),
                DataType::Time => "Time".to_string(),
                DataType::Decimal { precision, scale } => {
                    format!("Decimal({}, {})", precision, scale)
                }
                DataType::Binary => "Binary".to_string(),
                DataType::Json => "JSON".to_string(),
                DataType::Unknown => "Unknown".to_string(),
            }
        };

    // Helper to convert FieldSimilarity to FieldMappingCandidate
    let convert_to_candidate = |similarity: &graphica_core::inference::mapping::types::FieldSimilarity| -> FieldMappingCandidate {
        use graphica_core::schema::ConversionRulesEngine;

        let source_type_str = inference_type_to_string(&similarity.source.data_type);
        let target_type_str = inference_type_to_string(&similarity.target.data_type);
        let types_match = similarity.source.data_type == similarity.target.data_type;

        // Generate conversion info if types don't match
        let conversion = if !types_match {
            // Convert inference DataType to UniversalDataType for conversion engine
            let source_universal = parse_type_string(&source_type_str).unwrap_or(graphica_core::schema::UniversalDataType::Unknown);
            let target_universal = parse_type_string(&target_type_str).unwrap_or(graphica_core::schema::UniversalDataType::Unknown);

            let engine = ConversionRulesEngine::new();
            let is_safe = engine.is_safe_conversion(&source_universal, &target_universal);
            let is_lossy = engine.is_lossy_conversion(&source_universal, &target_universal);

            let warnings = if is_lossy {
                engine.validate_conversion(&source_universal, &target_universal).unwrap_or_default()
            } else {
                vec![]
            };

            let conversion_function = engine.get_conversion_sql(
                &source_universal,
                &target_universal,
                graphica_core::schema::SqlDialect::Generic
            ).ok();

            Some(TypeConversionInfo {
                is_safe,
                is_lossy,
                warnings,
                conversion_function,
            })
        } else {
            None
        };

        FieldMappingCandidate {
            source_field: similarity.source.column_name.clone(),
            source_type: source_type_str,
            target_field: similarity.target.column_name.clone(),
            target_type: target_type_str,
            confidence: similarity.confidence,
            types_match,
            conversion,
        }
    };

    // Convert MappingSuggestions to API response format
    let auto_mapped: Vec<FieldMappingCandidate> = mapping_result
        .suggestions
        .auto_mapped
        .iter()
        .map(convert_to_candidate)
        .collect();

    let recommended: Vec<FieldMappingCandidate> = mapping_result
        .suggestions
        .recommended
        .iter()
        .map(convert_to_candidate)
        .collect();

    let possible: Vec<FieldMappingCandidate> = mapping_result
        .suggestions
        .possible
        .iter()
        .map(convert_to_candidate)
        .collect();

    // Count conversions across all mapping tiers
    let mut all_mappings = Vec::new();
    all_mappings.extend(auto_mapped.iter());
    all_mappings.extend(recommended.iter());
    all_mappings.extend(possible.iter());

    let conversions_required = all_mappings.iter().filter(|m| !m.types_match).count();
    let lossy_conversions = all_mappings
        .iter()
        .filter(|m| m.conversion.as_ref().map_or(false, |c| c.is_lossy))
        .count();

    // Build SchemaInfo for source and target
    let source_schema_info = SchemaInfo {
        datasource_id: request.source_datasource_id.clone(),
        table_name: request.source_table.clone(),
        source_type: source_ds.source.source_type.clone(),
        field_count: source_schema.fields.len(),
        fields: source_schema
            .fields
            .iter()
            .map(|f| FieldInfo {
                name: f.name.clone(),
                data_type: type_to_string(&f.data_type),
                nullable: f.nullable,
                is_primary_key: f.constraints.primary_key,
            })
            .collect(),
    };

    let target_schema_info = SchemaInfo {
        datasource_id: request.target_datasource_id.clone(),
        table_name: request.target_table.clone(),
        source_type: target_ds.source.source_type.clone(),
        field_count: target_schema.fields.len(),
        fields: target_schema
            .fields
            .iter()
            .map(|f| FieldInfo {
                name: f.name.clone(),
                data_type: type_to_string(&f.data_type),
                nullable: f.nullable,
                is_primary_key: f.constraints.primary_key,
            })
            .collect(),
    };

    // Calculate unmapped fields
    let mapped_source_fields: std::collections::HashSet<_> = all_mappings
        .iter()
        .map(|m| m.source_field.as_str())
        .collect();
    let unmapped_count = source_schema.fields.len() - mapped_source_fields.len();

    // Build summary
    let summary = MappingSummary {
        total_source_fields: source_schema.fields.len(),
        total_target_fields: target_schema.fields.len(),
        auto_mapped_count: auto_mapped.len(),
        recommended_count: recommended.len(),
        possible_count: possible.len(),
        unmapped_count,
        conversions_required,
        lossy_conversions,
    };

    // Build response
    let response = CrossSourceMappingResponse {
        source_schema: source_schema_info,
        target_schema: target_schema_info,
        auto_mapped,
        recommended,
        possible,
        summary,
    };

    info!(
        "Cross-source mapping complete: {} auto-mapped, {} recommended, {} possible",
        response.summary.auto_mapped_count,
        response.summary.recommended_count,
        response.summary.possible_count
    );

    Ok((StatusCode::OK, Json(response)))
}

/// POST /api/v1/schema/conversion/validate
///
/// Validate a type conversion.
pub async fn validate_conversion(
    State(_state): State<Arc<ApiState>>,
    Json(request): Json<ValidateConversionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use super::schema_api_impl::{parse_type_string, type_to_string};
    use graphica_core::schema::ConversionRulesEngine;

    info!(
        "Validating conversion: {} -> {}",
        request.source_type, request.target_type
    );

    // Parse type strings to UniversalDataType
    let source = parse_type_string(&request.source_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid source type: {}", e),
        )
    })?;
    let target = parse_type_string(&request.target_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid target type: {}", e),
        )
    })?;

    // Use ConversionRulesEngine to validate
    let engine = ConversionRulesEngine::new();

    let is_safe = engine.is_safe_conversion(&source, &target);
    let is_lossy = engine.is_lossy_conversion(&source, &target);
    let is_invalid = !is_safe && !is_lossy;

    // Get warnings from validation
    let warnings = if is_lossy || is_invalid {
        engine
            .validate_conversion(&source, &target)
            .unwrap_or_default()
    } else {
        vec![]
    };

    // Get recommended SQL (generic dialect)
    let recommended_sql = if !is_invalid {
        engine
            .get_conversion_sql(&source, &target, graphica_core::schema::SqlDialect::Generic)
            .ok()
    } else {
        None
    };

    // Return validation results
    Ok((
        StatusCode::OK,
        Json(ValidateConversionResponse {
            source_type: type_to_string(&source),
            target_type: type_to_string(&target),
            is_safe,
            is_lossy,
            is_invalid,
            warnings,
            recommended_sql,
        }),
    ))
}

/// POST /api/v1/schema/conversion/sql
///
/// Generate conversion SQL for a specific dialect.
pub async fn generate_conversion_sql(
    State(_state): State<Arc<ApiState>>,
    Json(request): Json<GenerateConversionSqlRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use super::schema_api_impl::{parse_dialect_string, parse_type_string};
    use graphica_core::schema::ConversionRulesEngine;

    info!(
        "Generating conversion SQL: {} -> {} (dialect: {})",
        request.source_type, request.target_type, request.dialect
    );

    // Parse type strings to UniversalDataType
    let source = parse_type_string(&request.source_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid source type: {}", e),
        )
    })?;
    let target = parse_type_string(&request.target_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid target type: {}", e),
        )
    })?;

    // Parse dialect string
    let dialect = parse_dialect_string(&request.dialect)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid dialect: {}", e)))?;

    // Use ConversionRulesEngine to generate SQL
    let engine = ConversionRulesEngine::new();

    let is_safe = engine.is_safe_conversion(&source, &target);
    let is_lossy = engine.is_lossy_conversion(&source, &target);

    // Generate dialect-specific SQL
    let sql = engine
        .get_conversion_sql(&source, &target, dialect)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Cannot generate SQL: {}", e),
            )
        })?;

    // Get warnings
    let warnings = if is_lossy {
        engine
            .validate_conversion(&source, &target)
            .unwrap_or_default()
    } else {
        vec![]
    };

    // Return SQL response
    Ok((
        StatusCode::OK,
        Json(GenerateConversionSqlResponse {
            dialect: request.dialect,
            sql,
            is_safe,
            is_lossy,
            warnings,
        }),
    ))
}

/// POST /api/v1/schema/profile
///
/// Profile a datasource and return unified schema.
pub async fn profile_schema(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ProfileSchemaRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use super::schema_api_impl::{source_type_to_string, type_to_string};
    use std::time::Instant;

    info!(
        "Profiling schema: {}.{}",
        request.datasource_id, request.table_name
    );

    let start = Instant::now();

    // Get datasource catalog
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Datasource catalog not available".to_string(),
        )
    })?;

    // Get datasource from catalog
    let datasource_response = catalog
        .get_source(&request.datasource_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("Datasource not found: {}", e),
            )
        })?;

    // Use catalog's infer_schema method to profile the datasource
    let schema_def = catalog
        .infer_schema(
            &request.datasource_id,
            Some(&request.table_name),
            request.sample_size,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Schema profiling failed: {}", e),
            )
        })?;

    // Find the requested table in the schema definition
    let table_def = schema_def
        .tables
        .iter()
        .find(|t| t.name == request.table_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Table '{}' not found in datasource", request.table_name),
            )
        })?;

    // Convert columns to profiled fields
    let fields: Vec<ProfiledField> = table_def
        .columns
        .iter()
        .enumerate()
        .map(|(position, col)| {
            // Extract statistics if available
            let profile = col.statistics.as_ref().map(|stats| {
                // Extract sample values from most_common_values if available
                let samples = stats
                    .most_common_values
                    .as_ref()
                    .map(|mcv| mcv.iter().take(5).map(|vf| vf.value.clone()).collect())
                    .unwrap_or_default();

                // Calculate total rows from sample_size if available, otherwise estimate
                let total_rows = stats.sample_size.unwrap_or_else(|| {
                    // Estimate from null_count and null_percentage if available
                    if stats.null_percentage > 0.0 {
                        (stats.null_count as f64 / stats.null_percentage.max(0.001)) as u64
                    } else {
                        stats.null_count + stats.distinct_count.unwrap_or(0)
                    }
                });

                FieldProfileStats {
                    distinct_count: stats.distinct_count.unwrap_or(0),
                    total_rows,
                    null_percentage: stats.null_percentage,
                    samples,
                }
            });

            ProfiledField {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                nullable: col.nullable,
                position,
                profile,
            }
        })
        .collect();

    let processing_time_ms = start.elapsed().as_millis() as u64;

    // Return profiled schema response
    Ok((
        StatusCode::OK,
        Json(ProfileSchemaResponse {
            datasource_id: request.datasource_id.clone(),
            table_name: request.table_name.clone(),
            source_type: datasource_response.source.source_type.clone(),
            field_count: fields.len(),
            fields,
            row_count: table_def.estimated_rows,
            processing_time_ms,
        }),
    ))
}

/// GET /api/v1/schema/health
///
/// Health check for schema API.
pub async fn health_check(
    State(state): State<Arc<ApiState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let catalog_available = state.datasource_catalog.is_some();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "operational",
            "phase": "Phase 2 - API Integration Complete",
            "catalog_connected": catalog_available,
            "endpoints": {
                "POST /api/v1/schema/conversion/validate": {
                    "status": "operational",
                    "description": "Validate type conversions between UniversalDataTypes"
                },
                "POST /api/v1/schema/conversion/sql": {
                    "status": "operational",
                    "description": "Generate dialect-specific conversion SQL",
                    "supported_dialects": ["PostgreSQL", "MySQL", "Oracle", "DB2", "SQLServer", "Snowflake", "Generic"]
                },
                "POST /api/v1/schema/profile": {
                    "status": if catalog_available { "operational" } else { "unavailable" },
                    "description": "Profile datasource schema with field-level statistics",
                    "requires": "datasource_catalog"
                },
                "POST /api/v1/schema/cross-source/map": {
                    "status": if catalog_available { "operational" } else { "unavailable" },
                    "description": "Map fields between two datasources using multi-dimensional similarity analysis",
                    "requires": "datasource_catalog",
                    "features": ["lexical_matching", "statistical_analysis", "type_conversion_hints", "confidence_scoring"]
                }
            },
            "core_features": {
                "conversion_rules_engine": "available",
                "cross_source_mapper": "available",
                "unified_schema": "available",
                "multi_dialect_sql": "available"
            }
        })),
    ))
}

// ============================================================================
// Router Creation
// ============================================================================

/// Create schema API router
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Health check (no auth required for this endpoint)
        .route("/schema/health", get(health_check))
        // Cross-source mapping
        .route("/schema/cross-source/map", post(map_cross_source))
        // Type conversion
        .route("/schema/conversion/validate", post(validate_conversion))
        .route("/schema/conversion/sql", post(generate_conversion_sql))
        // Schema profiling
        .route("/schema/profile", post(profile_schema))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_min_confidence(), 0.5);
        assert_eq!(default_auto_map_threshold(), 0.9);
        assert_eq!(default_recommend_threshold(), 0.7);
        assert_eq!(default_sample_size(), 1000);
    }

    #[test]
    fn test_cross_source_mapping_request_deserialization() {
        let json = r#"{
            "source_datasource_id": "pg_prod",
            "source_table": "customers",
            "target_datasource_id": "csv_export",
            "target_table": "customer_data"
        }"#;

        let request: CrossSourceMappingRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.source_datasource_id, "pg_prod");
        assert_eq!(request.source_table, "customers");
        assert_eq!(request.min_confidence, 0.5);
        assert_eq!(request.auto_map_threshold, 0.9);
    }

    #[test]
    fn test_validate_conversion_request() {
        let json = r#"{
            "source_type": "Integer",
            "target_type": "String"
        }"#;

        let request: ValidateConversionRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.source_type, "Integer");
        assert_eq!(request.target_type, "String");
    }

    #[test]
    fn test_generate_conversion_sql_request() {
        let json = r#"{
            "source_type": "Integer",
            "target_type": "String",
            "dialect": "PostgreSQL"
        }"#;

        let request: GenerateConversionSqlRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.dialect, "PostgreSQL");
    }

    #[test]
    fn test_profile_schema_request() {
        let json = r#"{
            "datasource_id": "pg_prod",
            "table_name": "customers",
            "sample_size": 5000
        }"#;

        let request: ProfileSchemaRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.sample_size, 5000);
    }

    #[test]
    fn test_profile_schema_request_defaults() {
        let json = r#"{
            "datasource_id": "pg_prod",
            "table_name": "customers"
        }"#;

        let request: ProfileSchemaRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.sample_size, 1000);
    }
}
