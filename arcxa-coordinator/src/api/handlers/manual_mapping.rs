//! Manual Field Mapping REST API Handlers
//!
//! User-defined field-to-ontology mappings with 100% confidence priority.

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;

use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::mapping::manual::types::FieldCharacteristics;
use crate::mapping::manual::{ManualFieldMapping, SourceContext, UsageStats};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to create a manual field mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingRequest {
    /// Source identifier (e.g., database name)
    pub source_id: Option<String>,
    /// Table name
    pub table_name: String,
    /// Field name
    pub field_name: String,
    /// Target ontology field URI
    pub target_field_uri: String,
    /// Optional field characteristics for context matching
    pub field_metadata: Option<FieldCharacteristics>,
    /// Optional notes explaining the mapping
    pub notes: Option<String>,
    /// User creating the mapping
    pub created_by: String,
}

/// Response after creating a mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingResponse {
    pub id: String,
    pub message: String,
}

/// Request to update a manual field mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateMappingRequest {
    /// New target ontology field URI (optional)
    pub target_field_uri: Option<String>,
    /// Updated notes (optional)
    pub notes: Option<String>,
}

/// Response for get mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetMappingResponse {
    pub id: String,
    pub source_context: SourceContext,
    pub target_field_uri: String,
    pub confidence: f64,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub notes: Option<String>,
    pub usage_stats: UsageStatsDto,
}

/// Usage statistics DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UsageStatsDto {
    pub apply_count: u64,
    pub accept_count: u64,
    pub reject_count: u64,
    pub last_used: Option<String>,
}

/// Request for auto-suggestions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingsRequest {
    /// Optional table name to filter suggestions
    pub table_name: Option<String>,
    /// Optional field pattern to match
    pub field_pattern: Option<String>,
    /// Maximum number of suggestions
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Auto-suggestion response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingsResponse {
    pub suggestions: Vec<MappingSuggestionDto>,
}

/// Individual mapping suggestion DTO for API response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingSuggestionDto {
    pub table_name: String,
    pub field_name: String,
    pub target_field_uri: String,
    pub confidence: f64,
    pub reasoning: String,
    pub similar_mappings_count: u64,
}

/// Request for bulk import
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkImportRequest {
    pub mappings: Vec<CreateMappingRequest>,
    /// Strategy for handling conflicts: "skip", "overwrite", or "error"
    #[serde(default = "default_conflict_strategy")]
    pub conflict_strategy: String,
}

fn default_conflict_strategy() -> String {
    "skip".to_string()
}

/// Response for bulk import
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkImportResponse {
    pub total: usize,
    pub created: usize,
    pub skipped: usize,
    pub errors: Vec<BulkImportError>,
}

/// Error during bulk import
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkImportError {
    pub index: usize,
    pub table_name: String,
    pub field_name: String,
    pub error: String,
}

/// Response for bulk export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkExportResponse {
    pub mappings: Vec<GetMappingResponse>,
    pub total: usize,
    pub exported_at: String,
}

// ============================================================================
// Helper Functions for ML-based Suggestions
// ============================================================================

/// Generate ML-based field mapping suggestions using the statistical matcher
///
/// This function creates a synthetic SchemaField from the request parameters,
/// extracts minimal features (tokens and n-grams), and uses the MappingEngine's
/// statistical matcher to find ontology term candidates.
///
/// # Arguments
/// * `mapping_engine` - The mapping engine with access to statistical matcher
/// * `request` - The suggestion request with field pattern and table name
/// * `pattern` - The field pattern to match against
/// * `suggestions` - Mutable vector to append ML-based suggestions to
///
/// # Returns
/// Number of ML-based suggestions added
async fn get_ml_based_suggestions(
    mapping_engine: &Arc<crate::mapping::MappingEngine>,
    request: &SuggestMappingsRequest,
    pattern: &str,
    suggestions: &mut Vec<MappingSuggestionDto>,
) -> Result<usize> {
    use crate::mapping::types::{FieldContext, FieldFeatures, FieldStatistics, SchemaField};

    // Create a synthetic SchemaField from the request parameters
    // This is a lightweight field with minimal data - just enough for statistical matching
    let synthetic_field = SchemaField {
        id: format!("suggest_{}", uuid::Uuid::new_v4()),
        name: pattern.to_string(),
        normalized_name: normalize_field_name(pattern),
        data_type: "VARCHAR".to_string(), // Default type for pattern matching
        nullable: true,
        sample_values: vec![],
        source_id: format!("manual_suggestion_{}", chrono::Utc::now().timestamp()),
        table_name: request
            .table_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        description: None,
        features: Some(extract_minimal_features(pattern, &request.table_name)),
    };

    // Use the mapping engine's public API to find candidates for the synthetic field
    // This uses the statistical matcher (TF-IDF + N-grams) internally
    let candidates = match mapping_engine
        .find_candidates_for_field(&synthetic_field, request.limit)
        .await
    {
        Ok(candidates) => candidates,
        Err(e) => {
            debug!("Failed to generate ML-based suggestions: {}", e);
            return Ok(0); // Graceful degradation - return 0 suggestions
        }
    };

    if candidates.is_empty() {
        debug!("No ML-based suggestions found for pattern '{}'", pattern);
        return Ok(0);
    }

    debug!(
        "Found {} ML-based candidates for pattern '{}'",
        candidates.len(),
        pattern
    );

    let initial_count = suggestions.len();

    // Convert MappingCandidate results to MappingSuggestionDto
    for candidate in candidates {
        // Skip low-confidence suggestions (< 0.3)
        if candidate.confidence < 0.3 {
            continue;
        }

        suggestions.push(MappingSuggestionDto {
            table_name: synthetic_field.table_name.clone(),
            field_name: pattern.to_string(),
            target_field_uri: candidate.ontology_term_uri.clone(),
            confidence: candidate.confidence,
            reasoning: format!(
                "ML-based match (statistical): {} (confidence: {:.2})",
                candidate.explanation, candidate.confidence
            ),
            similar_mappings_count: candidate.similar_mappings.len() as u64,
        });
    }

    let added_count = suggestions.len() - initial_count;
    Ok(added_count)
}

/// Normalize a field name for matching (lowercase, remove special chars)
fn normalize_field_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Extract minimal features from a field name for statistical matching
///
/// This creates basic tokens and n-grams without requiring full schema profiling.
/// Sufficient for statistical matcher's TF-IDF and n-gram matching.
fn extract_minimal_features(
    field_name: &str,
    table_name: &Option<String>,
) -> crate::mapping::types::FieldFeatures {
    use crate::mapping::types::{FieldContext, FieldFeatures, FieldStatistics};

    // Tokenize field name (split on common delimiters + camelCase)
    let tokens = tokenize_field_name(field_name);

    // Generate 2-grams and 3-grams for fuzzy matching
    let ngrams = generate_ngrams(field_name, 2, 3);

    FieldFeatures {
        name_tokens: tokens,
        name_ngrams: ngrams,
        semantic_patterns: vec![], // No pattern detection for lightweight suggestions
        statistics: FieldStatistics {
            distinct_count: 0,
            sample_count: 0,
            null_rate: 0.0,
            avg_length: None,
            min_value: None,
            max_value: None,
            top_values: vec![],
        },
        inferred_type: None,
        context: FieldContext {
            table_name: table_name.clone().unwrap_or_default(),
            schema_name: None,
            related_fields: vec![],
            is_primary_key: false,
            is_foreign_key: false,
            foreign_key_ref: None,
        },
    }
}

/// Tokenize a field name into words
///
/// Splits on:
/// - Underscores: "customer_email" → ["customer", "email"]
/// - Hyphens: "customer-email" → ["customer", "email"]
/// - CamelCase: "customerEmail" → ["customer", "Email"]
fn tokenize_field_name(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();

    for (i, ch) in name.chars().enumerate() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current_token.is_empty() {
                tokens.push(current_token.to_lowercase());
                current_token.clear();
            }
        } else if ch.is_uppercase() && i > 0 && !current_token.is_empty() {
            // CamelCase boundary: save current token and start new one
            tokens.push(current_token.to_lowercase());
            current_token = ch.to_string();
        } else {
            current_token.push(ch);
        }
    }

    if !current_token.is_empty() {
        tokens.push(current_token.to_lowercase());
    }

    tokens
}

/// Generate n-grams from a field name
///
/// # Arguments
/// * `text` - The text to generate n-grams from
/// * `min_n` - Minimum n-gram size (e.g., 2 for bigrams)
/// * `max_n` - Maximum n-gram size (e.g., 3 for trigrams)
fn generate_ngrams(text: &str, min_n: usize, max_n: usize) -> Vec<String> {
    let normalized = text.to_lowercase();
    let chars: Vec<char> = normalized.chars().collect();
    let mut ngrams = Vec::new();

    for n in min_n..=max_n {
        if chars.len() < n {
            continue;
        }

        for i in 0..=(chars.len() - n) {
            let ngram: String = chars[i..i + n].iter().collect();
            ngrams.push(ngram);
        }
    }

    ngrams
}

// ============================================================================
// Handler Functions
// ============================================================================

/// POST /api/v1/mapping/manual - Create a new manual mapping
#[utoipa::path(
    post,
    path = "/api/v1/mapping/manual",
    request_body = CreateMappingRequest,
    responses(
        (status = 200, description = "Manual mapping created successfully", body = CreateMappingResponse),
        (status = 409, description = "Mapping already exists for this source context", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn create_mapping(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateMappingRequest>,
) -> Result<Json<CreateMappingResponse>, ApiError> {
    info!(
        "Creating manual mapping: {}.{} -> {}",
        request.table_name, request.field_name, request.target_field_uri
    );

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    // Build source context
    let source_context = SourceContext {
        source_id: request.source_id.clone(),
        table_name: request.table_name.clone(),
        field_name: request.field_name.clone(),
        field_metadata: request.field_metadata.clone(),
    };

    // Check if mapping already exists
    if let Ok(Some(existing)) = store.find_by_source(&source_context).await {
        return Err(ApiError::conflict(format!(
            "Mapping already exists for {}.{} (ID: {}). Use PUT to update.",
            request.table_name, request.field_name, existing.id
        )));
    }

    // Generate ID
    let id = format!(
        "manual_{}_{}_{}",
        request.table_name,
        request.field_name,
        chrono::Utc::now().timestamp()
    );

    // Create mapping
    let mapping = ManualFieldMapping {
        id: id.clone(),
        source_context,
        target_field_uri: request.target_field_uri,
        confidence: 1.0,
        created_by: request.created_by,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        notes: request.notes,
        usage_stats: UsageStats::default(),
    };

    // Store mapping
    store
        .store_mapping(mapping)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store mapping: {}", e)))?;

    info!("Manual mapping created: {}", id);

    Ok(Json(CreateMappingResponse {
        id: id.clone(),
        message: format!("Manual mapping created successfully: {}", id),
    }))
}

/// GET /api/v1/mapping/manual/:id - Get a specific manual mapping
#[utoipa::path(
    get,
    path = "/api/v1/mapping/manual/{id}",
    params(
        ("id" = String, Path, description = "Mapping ID to retrieve")
    ),
    responses(
        (status = 200, description = "Manual mapping retrieved successfully", body = GetMappingResponse),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn get_mapping(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<GetMappingResponse>, ApiError> {
    debug!("Getting manual mapping: {}", id);

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    let mapping = store
        .get_mapping(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to retrieve mapping: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Mapping not found: {}", id)))?;

    Ok(Json(GetMappingResponse {
        id: mapping.id,
        source_context: mapping.source_context,
        target_field_uri: mapping.target_field_uri,
        confidence: mapping.confidence,
        created_by: mapping.created_by,
        created_at: mapping.created_at.to_rfc3339(),
        updated_at: mapping.updated_at.to_rfc3339(),
        notes: mapping.notes,
        usage_stats: UsageStatsDto {
            apply_count: mapping.usage_stats.apply_count,
            accept_count: mapping.usage_stats.accept_count,
            reject_count: mapping.usage_stats.reject_count,
            last_used: mapping.usage_stats.last_used.map(|dt| dt.to_rfc3339()),
        },
    }))
}

/// PUT /api/v1/mapping/manual/:id - Update a manual mapping
#[utoipa::path(
    put,
    path = "/api/v1/mapping/manual/{id}",
    params(
        ("id" = String, Path, description = "Mapping ID to update")
    ),
    request_body = UpdateMappingRequest,
    responses(
        (status = 200, description = "Manual mapping updated successfully", body = GetMappingResponse),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn update_mapping(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateMappingRequest>,
) -> Result<Json<GetMappingResponse>, ApiError> {
    info!("Updating manual mapping: {}", id);

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    // Get existing mapping
    let mut mapping = store
        .get_mapping(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to retrieve mapping: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Mapping not found: {}", id)))?;

    // Update fields
    if let Some(target) = request.target_field_uri {
        mapping.target_field_uri = target;
    }
    if let Some(notes) = request.notes {
        mapping.notes = Some(notes);
    }
    mapping.updated_at = chrono::Utc::now();

    // Store updated mapping
    store
        .store_mapping(mapping.clone())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update mapping: {}", e)))?;

    info!("Manual mapping updated: {}", id);

    Ok(Json(GetMappingResponse {
        id: mapping.id,
        source_context: mapping.source_context,
        target_field_uri: mapping.target_field_uri,
        confidence: mapping.confidence,
        created_by: mapping.created_by,
        created_at: mapping.created_at.to_rfc3339(),
        updated_at: mapping.updated_at.to_rfc3339(),
        notes: mapping.notes,
        usage_stats: UsageStatsDto {
            apply_count: mapping.usage_stats.apply_count,
            accept_count: mapping.usage_stats.accept_count,
            reject_count: mapping.usage_stats.reject_count,
            last_used: mapping.usage_stats.last_used.map(|dt| dt.to_rfc3339()),
        },
    }))
}

/// DELETE /api/v1/mapping/manual/:id - Delete a manual mapping
#[utoipa::path(
    delete,
    path = "/api/v1/mapping/manual/{id}",
    params(
        ("id" = String, Path, description = "Mapping ID to delete")
    ),
    responses(
        (status = 200, description = "Manual mapping deleted successfully", body = serde_json::Value),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn delete_mapping(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Deleting manual mapping: {}", id);

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    // Verify mapping exists first
    store
        .get_mapping(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check mapping: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Mapping not found: {}", id)))?;

    // Delete mapping
    store
        .delete_mapping(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete mapping: {}", e)))?;

    info!("Manual mapping deleted: {}", id);

    Ok(Json(serde_json::json!({
        "id": id,
        "message": "Manual mapping deleted successfully",
        "deleted_at": chrono::Utc::now().to_rfc3339(),
    })))
}

/// POST /api/v1/mapping/manual/suggest - Get auto-suggestions based on existing mappings
#[utoipa::path(
    post,
    path = "/api/v1/mapping/manual/suggest",
    request_body = SuggestMappingsRequest,
    responses(
        (status = 200, description = "Suggestions generated successfully", body = SuggestMappingsResponse),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn suggest_mappings(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SuggestMappingsRequest>,
) -> Result<Json<SuggestMappingsResponse>, ApiError> {
    debug!("Generating mapping suggestions: {:?}", request);

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    let mut suggestions = Vec::new();

    // If field pattern provided, find similar mappings
    if let Some(pattern) = request.field_pattern.as_ref() {
        // Create a dummy source context for similarity search
        let dummy_context = SourceContext {
            source_id: None,
            table_name: request.table_name.clone().unwrap_or_default(),
            field_name: pattern.clone(),
            field_metadata: None,
        };

        let similar_mappings = store
            .find_similar_mappings(&dummy_context, request.limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to find similar mappings: {}", e)))?;

        for suggestion in similar_mappings {
            suggestions.push(MappingSuggestionDto {
                table_name: suggestion.mapping.source_context.table_name.clone(),
                field_name: suggestion.mapping.source_context.field_name.clone(),
                target_field_uri: suggestion.mapping.target_field_uri.clone(),
                confidence: suggestion.relevance_score,
                reasoning: format!(
                    "Similar field name (pattern: {}). Created by {} on {}. Reason: {:?}",
                    pattern,
                    suggestion.mapping.created_by,
                    suggestion.mapping.created_at.format("%Y-%m-%d"),
                    suggestion.suggestion_reason
                ),
                similar_mappings_count: suggestion.mapping.usage_stats.apply_count,
            });
        }
    }

    // ML-based suggestions from statistical matcher (if available)
    // This provides intelligent ontology term matching using TF-IDF + N-grams
    if let Some(pattern) = request.field_pattern.as_ref() {
        if let Some(mapping_engine) = state.mapping_engine.as_ref() {
            match get_ml_based_suggestions(mapping_engine, &request, pattern, &mut suggestions)
                .await
            {
                Ok(ml_count) => {
                    if ml_count > 0 {
                        debug!(
                            "Added {} ML-based suggestions from statistical matcher",
                            ml_count
                        );
                    }
                }
                Err(e) => {
                    // Log error but don't fail the request - graceful degradation
                    warn!("Failed to generate ML-based suggestions: {}", e);
                    debug!("Continuing with pattern-based suggestions only");
                }
            }
        } else {
            debug!("Mapping engine not available, skipping ML-based suggestions");
        }
    }

    // Sort all suggestions by confidence (descending) and limit to requested count
    suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions.truncate(request.limit);

    Ok(Json(SuggestMappingsResponse { suggestions }))
}

/// POST /api/v1/mapping/manual/import - Bulk import manual mappings
#[utoipa::path(
    post,
    path = "/api/v1/mapping/manual/import",
    request_body = BulkImportRequest,
    responses(
        (status = 200, description = "Bulk import completed (check errors for failures)", body = BulkImportResponse),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn bulk_import(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<BulkImportRequest>,
) -> Result<Json<BulkImportResponse>, ApiError> {
    info!("Bulk importing {} manual mappings", request.mappings.len());

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    let mut created = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    for (index, mapping_req) in request.mappings.iter().enumerate() {
        // Build source context
        let source_context = SourceContext {
            source_id: mapping_req.source_id.clone(),
            table_name: mapping_req.table_name.clone(),
            field_name: mapping_req.field_name.clone(),
            field_metadata: mapping_req.field_metadata.clone(),
        };

        // Check if mapping already exists
        let existing = store.find_by_source(&source_context).await;

        match existing {
            Ok(Some(existing_mapping)) => {
                // Mapping exists - apply conflict strategy
                match request.conflict_strategy.as_str() {
                    "skip" => {
                        debug!("Skipping existing mapping: {}", existing_mapping.id);
                        skipped += 1;
                        continue;
                    }
                    "overwrite" => {
                        // Update existing mapping
                        let mut updated = existing_mapping;
                        updated.target_field_uri = mapping_req.target_field_uri.clone();
                        updated.notes = mapping_req.notes.clone();
                        updated.updated_at = chrono::Utc::now();

                        if let Err(e) = store.store_mapping(updated).await {
                            errors.push(BulkImportError {
                                index,
                                table_name: mapping_req.table_name.clone(),
                                field_name: mapping_req.field_name.clone(),
                                error: format!("Failed to overwrite: {}", e),
                            });
                        } else {
                            created += 1;
                        }
                    }
                    "error" => {
                        errors.push(BulkImportError {
                            index,
                            table_name: mapping_req.table_name.clone(),
                            field_name: mapping_req.field_name.clone(),
                            error: format!("Mapping already exists: {}", existing_mapping.id),
                        });
                    }
                    _ => {
                        errors.push(BulkImportError {
                            index,
                            table_name: mapping_req.table_name.clone(),
                            field_name: mapping_req.field_name.clone(),
                            error: format!(
                                "Invalid conflict strategy: {}",
                                request.conflict_strategy
                            ),
                        });
                    }
                }
            }
            Ok(None) => {
                // No existing mapping - create new one
                let id = format!(
                    "manual_{}_{}_{}",
                    mapping_req.table_name,
                    mapping_req.field_name,
                    chrono::Utc::now().timestamp_nanos()
                );

                let mapping = ManualFieldMapping {
                    id,
                    source_context,
                    target_field_uri: mapping_req.target_field_uri.clone(),
                    confidence: 1.0,
                    created_by: mapping_req.created_by.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    notes: mapping_req.notes.clone(),
                    usage_stats: UsageStats::default(),
                };

                if let Err(e) = store.store_mapping(mapping).await {
                    errors.push(BulkImportError {
                        index,
                        table_name: mapping_req.table_name.clone(),
                        field_name: mapping_req.field_name.clone(),
                        error: format!("Failed to create: {}", e),
                    });
                } else {
                    created += 1;
                }
            }
            Err(e) => {
                errors.push(BulkImportError {
                    index,
                    table_name: mapping_req.table_name.clone(),
                    field_name: mapping_req.field_name.clone(),
                    error: format!("Failed to check existing mapping: {}", e),
                });
            }
        }
    }

    info!(
        "Bulk import complete: {} created, {} skipped, {} errors",
        created,
        skipped,
        errors.len()
    );

    Ok(Json(BulkImportResponse {
        total: request.mappings.len(),
        created,
        skipped,
        errors,
    }))
}

/// GET /api/v1/mapping/manual/export - Export all manual mappings
#[utoipa::path(
    get,
    path = "/api/v1/mapping/manual/export",
    responses(
        (status = 200, description = "All manual mappings exported successfully", body = BulkExportResponse),
        (status = 500, description = "Internal server error", body = ApiError),
        (status = 503, description = "Manual mapping store not available", body = ApiError),
    ),
    tag = "Manual Mapping"
)]
pub async fn bulk_export(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<BulkExportResponse>, ApiError> {
    info!("Exporting all manual mappings");

    let store = state.manual_mapping_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Manual mapping store not available".to_string())
    })?;

    let export_result = store
        .bulk_export(None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to export mappings: {}", e)))?;

    let mapped_responses: Vec<GetMappingResponse> = export_result
        .mappings
        .into_iter()
        .map(|mapping| GetMappingResponse {
            id: mapping.id,
            source_context: mapping.source_context,
            target_field_uri: mapping.target_field_uri,
            confidence: mapping.confidence,
            created_by: mapping.created_by,
            created_at: mapping.created_at.to_rfc3339(),
            updated_at: mapping.updated_at.to_rfc3339(),
            notes: mapping.notes,
            usage_stats: UsageStatsDto {
                apply_count: mapping.usage_stats.apply_count,
                accept_count: mapping.usage_stats.accept_count,
                reject_count: mapping.usage_stats.reject_count,
                last_used: mapping.usage_stats.last_used.map(|dt| dt.to_rfc3339()),
            },
        })
        .collect();

    let total = mapped_responses.len();

    info!("Exported {} manual mappings", total);

    Ok(Json(BulkExportResponse {
        total,
        mappings: mapped_responses,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_mapping_request_deserialization() {
        let json = r#"{
            "source_id": "postgres_prod",
            "table_name": "customers",
            "field_name": "email_address",
            "target_field_uri": "http://schema.org/email",
            "notes": "Primary email field",
            "created_by": "analyst@company.com"
        }"#;

        let request: Result<CreateMappingRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize valid request");

        let req = request.unwrap();
        assert_eq!(req.source_id, Some("postgres_prod".to_string()));
        assert_eq!(req.table_name, "customers");
        assert_eq!(req.field_name, "email_address");
        assert_eq!(req.target_field_uri, "http://schema.org/email");
    }

    #[test]
    fn test_update_mapping_request_partial() {
        let json = r#"{ "notes": "Updated notes only" }"#;

        let request: Result<UpdateMappingRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize partial update");

        let req = request.unwrap();
        assert!(req.target_field_uri.is_none());
        assert_eq!(req.notes, Some("Updated notes only".to_string()));
    }

    #[test]
    fn test_suggest_mappings_request_defaults() {
        let json = r#"{ "table_name": "users" }"#;

        let request: Result<SuggestMappingsRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize with defaults");

        let req = request.unwrap();
        assert_eq!(req.table_name, Some("users".to_string()));
        assert_eq!(req.limit, 10); // default
    }

    #[test]
    fn test_bulk_import_request_conflict_strategies() {
        let json = r#"{
            "mappings": [],
            "conflict_strategy": "overwrite"
        }"#;

        let request: Result<BulkImportRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok());

        let req = request.unwrap();
        assert_eq!(req.conflict_strategy, "overwrite");
    }

    #[test]
    fn test_get_mapping_response_serialization() {
        let response = GetMappingResponse {
            id: "manual_test_123".to_string(),
            source_context: SourceContext {
                source_id: Some("db1".to_string()),
                table_name: "users".to_string(),
                field_name: "email".to_string(),
                field_metadata: None,
            },
            target_field_uri: "http://schema.org/email".to_string(),
            confidence: 1.0,
            created_by: "admin".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            notes: Some("Test mapping".to_string()),
            usage_stats: UsageStatsDto {
                apply_count: 42,
                accept_count: 10,
                reject_count: 0,
                last_used: Some("2024-01-02T00:00:00Z".to_string()),
            },
        };

        let json = serde_json::to_string(&response);
        assert!(json.is_ok(), "Should serialize response");

        let json_str = json.unwrap();
        assert!(json_str.contains("manual_test_123"));
        assert!(json_str.contains("http://schema.org/email"));
        assert!(json_str.contains("\"apply_count\":42"));
    }
}
