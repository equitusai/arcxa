//! Field Lineage API Handlers
//!
//! HTTP handlers for field lineage endpoints with RDF persistence.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use utoipa;

use crate::governance::rdf_store::{NamedGraph, RdfStore};
use graphica_core::orchestration::field_lineage::{
    ConflictSeverity, FieldConflict, FieldLineageStore, FieldResolution, FieldResolver, FieldValue,
    SourceValue, StrategyType, VotingStrategy,
};

use super::types::*;
use crate::api::ApiState;

// SPARQL result parsing helpers

/// Extract string value from SPARQL binding
fn get_sparql_string(binding: &serde_json::Value, var: &str) -> Option<String> {
    binding.get(var)?.get("value")?.as_str().map(String::from)
}

/// Extract f64 value from SPARQL binding
fn get_sparql_f64(binding: &serde_json::Value, var: &str) -> Option<f64> {
    binding.get(var)?.get("value")?.as_str()?.parse().ok()
}

/// Extract DateTime from SPARQL binding
fn get_sparql_datetime(binding: &serde_json::Value, var: &str) -> Option<chrono::DateTime<Utc>> {
    let s = binding.get(var)?.get("value")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Extract JSON value from SPARQL binding
fn get_sparql_json(binding: &serde_json::Value, var: &str) -> Option<serde_json::Value> {
    let s = binding.get(var)?.get("value")?.as_str()?;
    serde_json::from_str(s).ok()
}

/// Get field lineage for a specific field
#[utoipa::path(
    get,
    path = "/api/v1/entities/{entity_id}/fields/{field_name}/lineage",
    tag = "Field Lineage",
    params(
        ("entity_id" = String, Path, description = "Entity ID"),
        ("field_name" = String, Path, description = "Field name")
    ),
    responses(
        (status = 200, description = "Field lineage retrieved successfully", body = FieldLineageResponse),
        (status = 404, description = "Entity or field not found"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "RDF store not available")
    )
)]
pub async fn get_field_lineage(
    State(state): State<Arc<ApiState>>,
    Path((entity_id, field_name)): Path<(String, String)>,
) -> Result<Json<FieldLineageResponse>, (StatusCode, String)> {
    tracing::info!(
        "Getting field lineage for entity {}, field {}",
        entity_id,
        field_name
    );

    let rdf_store = state.rdf_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "RDF store not available".to_string(),
        )
    })?;

    let storage = FieldLineageStore::new();
    let sparql = storage.query_field_lineage(&entity_id, &field_name);

    // Execute SPARQL query
    let results = rdf_store.query(&sparql).map_err(|e| {
        tracing::error!("SPARQL query failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query failed: {}", e),
        )
    })?;

    if results.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            format!(
                "No lineage found for entity {}, field {}",
                entity_id, field_name
            ),
        ));
    }

    // Parse SPARQL JSON results
    tracing::debug!(
        "Retrieved {} lineage records for {}.{}",
        results.len(),
        entity_id,
        field_name
    );

    // Extract bindings from SPARQL JSON result
    let bindings = results
        .get(0)
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid SPARQL result format".to_string(),
            )
        })?;

    if bindings.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            format!(
                "No lineage found for entity {}, field {}",
                entity_id, field_name
            ),
        ));
    }

    // Parse first binding for main field info
    let first = &bindings[0];
    let value = get_sparql_json(first, "value").ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing value".to_string(),
    ))?;
    let confidence = get_sparql_f64(first, "confidence").ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing confidence".to_string(),
    ))?;
    let resolved_at = get_sparql_datetime(first, "resolvedAt").ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing resolvedAt".to_string(),
    ))?;
    let strategy = get_sparql_string(first, "strategy").ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing strategy".to_string(),
    ))?;
    let explanation = get_sparql_string(first, "explanation").ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing explanation".to_string(),
    ))?;

    // Collect all source values from bindings
    let mut source_values = Vec::new();
    for (idx, binding) in bindings.iter().enumerate() {
        if let Some(source_value) = get_sparql_json(binding, "sourceValue") {
            let source_system = get_sparql_string(binding, "sourceSystem").unwrap_or_default();
            let source_authority = get_sparql_f64(binding, "sourceAuthority").unwrap_or(0.0);
            let vote_weight = get_sparql_f64(binding, "voteWeight").unwrap_or(0.0);

            source_values.push(SourceValueResponse {
                id: format!("source_{}", idx),
                value: source_value,
                source_system,
                source_timestamp: resolved_at,
                source_authority,
                confidence: None,
                vote_count: 0,
                vote_weight,
                metadata: HashMap::new(),
            });
        }
    }

    // Build response
    let current_value = FieldValueResponse {
        field_name: field_name.clone(),
        value: value.clone(),
        value_type: infer_json_type(&value),
        confidence,
        resolved_at,
        valid_from: resolved_at,
        valid_to: None,
        explanation: Some(explanation.clone()),
    };

    let resolution = FieldResolutionResponse {
        id: format!("resolution_{}", resolved_at.timestamp()),
        resolved_at,
        resolved_by: "system".to_string(),
        strategy: VotingStrategyResponse {
            strategy_type: parse_strategy_type(&strategy),
            config: serde_json::json!({}),
        },
        source_values: source_values.clone(),
        selected_value: source_values
            .first()
            .cloned()
            .unwrap_or_else(|| SourceValueResponse {
                id: "selected".to_string(),
                value: value.clone(),
                source_system: "unknown".to_string(),
                source_timestamp: resolved_at,
                source_authority: 1.0,
                confidence: Some(confidence),
                vote_count: 0,
                vote_weight: confidence,
                metadata: HashMap::new(),
            }),
        rejected_values: Vec::new(),
        explanation,
        conflict: None,
    };

    let response = FieldLineageResponse {
        entity_id: entity_id.clone(),
        field_name,
        current_value,
        resolution,
    };

    Ok(Json(response))
}

/// Get field history for a specific field
#[utoipa::path(
    get,
    path = "/api/v1/entities/{entity_id}/fields/{field_name}/history",
    tag = "Field Lineage",
    params(
        ("entity_id" = String, Path, description = "Entity ID"),
        ("field_name" = String, Path, description = "Field name")
    ),
    responses(
        (status = 200, description = "Field history retrieved successfully", body = FieldHistoryResponse),
        (status = 404, description = "Entity or field not found"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "RDF store not available")
    )
)]
pub async fn get_field_history(
    State(state): State<Arc<ApiState>>,
    Path((entity_id, field_name)): Path<(String, String)>,
) -> Result<Json<FieldHistoryResponse>, (StatusCode, String)> {
    tracing::info!(
        "Getting field history for entity {}, field {}",
        entity_id,
        field_name
    );

    let rdf_store = state.rdf_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "RDF store not available".to_string(),
        )
    })?;

    let storage = FieldLineageStore::new();
    let sparql = storage.query_field_history(&entity_id, &field_name);

    // Execute SPARQL query for high-performance history retrieval
    let results = rdf_store.query(&sparql).map_err(|e| {
        tracing::error!("History query failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query failed: {}", e),
        )
    })?;

    tracing::debug!(
        "Retrieved {} historical values for {}.{}",
        results.len(),
        entity_id,
        field_name
    );

    // Extract bindings from SPARQL JSON result
    let bindings = results
        .get(0)
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid SPARQL result format".to_string(),
            )
        })?;

    // Parse each historical value
    let mut history = Vec::new();
    for binding in bindings {
        if let (Some(value), Some(confidence), Some(valid_from)) = (
            get_sparql_json(binding, "value"),
            get_sparql_f64(binding, "confidence"),
            get_sparql_datetime(binding, "validFrom"),
        ) {
            history.push(FieldValueResponse {
                field_name: field_name.clone(),
                value: value.clone(),
                value_type: infer_json_type(&value),
                confidence,
                resolved_at: valid_from,
                valid_from,
                valid_to: get_sparql_datetime(binding, "validTo"),
                explanation: get_sparql_string(binding, "explanation"),
            });
        }
    }

    let response = FieldHistoryResponse {
        entity_id: entity_id.clone(),
        field_name,
        history,
    };

    Ok(Json(response))
}

/// Create a golden record from multiple source values
#[utoipa::path(
    post,
    path = "/api/v1/entities/{entity_id}/resolved-entity",
    tag = "Field Lineage",
    params(
        ("entity_id" = String, Path, description = "Entity ID")
    ),
    request_body = CreateResolvedEntityRequest,
    responses(
        (status = 200, description = "Golden record created successfully", body = ResolvedEntityResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_resolved_entity(
    State(_state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
    Json(request): Json<CreateResolvedEntityRequest>,
) -> Result<Json<ResolvedEntityResponse>, (StatusCode, String)> {
    tracing::info!("Creating golden record for entity {}", entity_id);

    // Create field resolver
    let resolver = if let Some(strategy_input) = &request.voting_strategy {
        FieldResolver::with_strategy(strategy_input.strategy_type)
    } else {
        FieldResolver::new()
    };

    let resolver = if let Some(min_conf) = request.min_confidence {
        resolver.with_min_confidence(min_conf)
    } else {
        resolver
    };

    // Convert input source values to core types
    let mut fields_to_resolve: HashMap<String, Vec<SourceValue>> = HashMap::new();

    for (field_name, source_inputs) in request.fields {
        let source_values: Vec<SourceValue> = source_inputs
            .into_iter()
            .enumerate()
            .map(|(idx, input)| SourceValue {
                id: format!("src_{}_{}", field_name, idx),
                value: input.value,
                source_system: input.source_system,
                source_timestamp: input.source_timestamp,
                source_authority: input.source_authority,
                confidence: input.confidence,
                vote_count: 0,
                vote_weight: 1.0,
                metadata: input.metadata.unwrap_or_default(),
            })
            .collect();

        fields_to_resolve.insert(field_name, source_values);
    }

    // Convert voting strategy input if provided
    let voting_strategy = request.voting_strategy.as_ref().map(|vs| {
        let mut params = serde_json::Map::new();
        if let Some(decay_rate) = vs.decay_rate {
            params.insert("decay_rate".to_string(), serde_json::json!(decay_rate));
        }
        if let Some(ref_time) = vs.reference_time {
            params.insert("reference_time".to_string(), serde_json::json!(ref_time));
        }

        VotingStrategy {
            strategy_type: vs.strategy_type,
            parameters: serde_json::Value::Object(params),
            description: format!("{:?} voting strategy", vs.strategy_type),
        }
    });

    // Resolve all fields
    let resolutions = resolver
        .resolve_fields(&entity_id, fields_to_resolve, voting_strategy)
        .map_err(|e| {
            tracing::error!("Failed to resolve fields: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Resolution failed: {}", e),
            )
        })?;

    // Create golden record
    let golden_record = resolver
        .create_resolved_entity(&entity_id, resolutions)
        .map_err(|e| {
            tracing::error!("Failed to create golden record: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Golden record creation failed: {}", e),
            )
        })?;

    // Convert to response type
    let mut fields_response = HashMap::new();
    for (field_name, field_value) in &golden_record.fields {
        fields_response.insert(field_name.clone(), field_value_to_response(field_value));
    }

    let low_confidence_fields = golden_record.low_confidence_fields(0.70);
    let conflicting_fields = golden_record.conflicting_fields();

    let response = ResolvedEntityResponse {
        entity_id: golden_record.entity_id.clone(),
        fields: fields_response,
        overall_confidence: golden_record.overall_confidence,
        conflict_count: golden_record.conflict_count,
        requires_review: golden_record.requires_review,
        created_at: golden_record.created_at,
        low_confidence_fields: low_confidence_fields
            .into_iter()
            .map(String::from)
            .collect(),
        conflicting_fields: conflicting_fields.into_iter().map(String::from).collect(),
    };

    tracing::info!(
        "Created golden record for entity {} with {} fields, {} conflicts",
        entity_id,
        response.fields.len(),
        response.conflict_count
    );

    // Persist to RDF store for durability and queryability
    if let Some(rdf_store) = &_state.rdf_store {
        let storage = FieldLineageStore::new();
        let sparql_queries = resolver.resolved_entity_to_sparql(&golden_record);

        // Execute batch SPARQL UPDATE for high performance
        let graph = NamedGraph::new("http://graphica.io/graph/field-lineage");

        match rdf_store.update(&sparql_queries) {
            Ok(_) => {
                tracing::debug!(
                    "Persisted golden record {} to RDF store ({} fields)",
                    entity_id,
                    golden_record.fields.len()
                );
            }
            Err(e) => {
                tracing::error!("Failed to persist golden record to RDF: {}", e);
                // Don't fail the request - golden record is already computed
                // RDF persistence is for lineage/audit purposes
            }
        }
    } else {
        tracing::warn!("RDF store not available - golden record not persisted");
    }

    Ok(Json(response))
}

/// Get existing golden record from cache or RDF store
///
/// Flow:
/// 1. Check streaming cache (if available) - O(1) lookup
/// 2. If cache miss, query RDF store - SPARQL query
/// 3. Return result
#[utoipa::path(
    get,
    path = "/api/v1/entities/{entity_id}/resolved-entity",
    tag = "Field Lineage",
    params(
        ("entity_id" = String, Path, description = "Entity ID")
    ),
    responses(
        (status = 200, description = "Golden record retrieved successfully", body = ResolvedEntityResponse),
        (status = 404, description = "Golden record not found"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "RDF store not available")
    )
)]
pub async fn get_resolved_entity(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<ResolvedEntityResponse>, (StatusCode, String)> {
    tracing::info!("Getting golden record for entity {}", entity_id);

    // Step 1: Check cache first (if available)
    if let Some(cache) = &state.resolved_entity_cache {
        if let Some(cached_record) = cache.get(&entity_id) {
            tracing::debug!("Cache HIT for entity {}", entity_id);

            // Convert cached record to API response
            let mut fields_response = HashMap::new();
            for (field_name, cached_field) in &cached_record.fields {
                // Infer value type
                let value_type = match &cached_field.value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                }
                .to_string();

                fields_response.insert(
                    field_name.clone(),
                    FieldValueResponse {
                        field_name: field_name.clone(),
                        value: cached_field.value.clone(),
                        value_type,
                        confidence: cached_field.confidence,
                        resolved_at: cached_field.resolved_at,
                        valid_from: cached_field.resolved_at,
                        valid_to: None,
                        explanation: None,
                    },
                );
            }

            let response = ResolvedEntityResponse {
                entity_id: cached_record.entity_id.clone(),
                fields: fields_response,
                overall_confidence: cached_record.overall_confidence,
                conflict_count: cached_record.conflict_count,
                requires_review: cached_record.requires_review,
                created_at: cached_record.created_at,
                low_confidence_fields: cached_record
                    .fields
                    .iter()
                    .filter(|(_, f)| f.confidence < 0.70)
                    .map(|(name, _)| name.clone())
                    .collect(),
                conflicting_fields: vec![], // Not tracked in cache
            };

            return Ok(Json(response));
        } else {
            tracing::debug!("Cache MISS for entity {}", entity_id);
        }
    }

    // Step 2: Cache miss or cache not available - query RDF store
    let rdf_store = state.rdf_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "RDF store not available".to_string(),
        )
    })?;

    // Build SPARQL query
    let storage = FieldLineageStore::new();
    let sparql = storage.query_golden_record(&entity_id);

    // Execute query
    let results = rdf_store.query(&sparql).map_err(|e| {
        tracing::error!("SPARQL query failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query failed: {}", e),
        )
    })?;

    if results.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("No golden record found for entity {}", entity_id),
        ));
    }

    // Parse SPARQL JSON results
    let bindings = results
        .get(0)
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid SPARQL result format".to_string(),
            )
        })?;

    if bindings.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("No golden record found for entity {}", entity_id),
        ));
    }

    // Parse fields from bindings
    let mut fields: HashMap<String, FieldValueResponse> = HashMap::new();
    let mut conflict_count = 0;
    let mut requires_review = false;
    let mut latest_resolved_at = None::<chrono::DateTime<Utc>>;
    let mut total_confidence = 0.0;
    let mut conflicting_fields = Vec::new();
    let mut low_confidence_fields = Vec::new();

    for binding in bindings {
        let field_name = get_sparql_string(binding, "fieldName").ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Missing fieldName".to_string(),
        ))?;

        let value = get_sparql_json(binding, "value").ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Missing value".to_string(),
        ))?;

        let confidence = get_sparql_f64(binding, "confidence").ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Missing confidence".to_string(),
        ))?;

        let resolved_at = get_sparql_datetime(binding, "resolvedAt").ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Missing resolvedAt".to_string(),
        ))?;

        // Track latest resolution time
        if latest_resolved_at.is_none() || resolved_at > latest_resolved_at.unwrap() {
            latest_resolved_at = Some(resolved_at);
        }

        // Check for conflicts
        let has_conflict = binding.get("conflictSeverity").is_some();
        if has_conflict {
            conflict_count += 1;
            conflicting_fields.push(field_name.clone());

            let review_required = get_sparql_string(binding, "requiresReview")
                .map(|s| s == "true")
                .unwrap_or(false);
            if review_required {
                requires_review = true;
            }
        }

        // Track low confidence fields
        if confidence < 0.70 {
            low_confidence_fields.push(field_name.clone());
        }

        total_confidence += confidence;

        // Infer value type
        let value_type = match &value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
        .to_string();

        // Create field value response
        fields.insert(
            field_name.clone(),
            FieldValueResponse {
                field_name: field_name.clone(),
                value: value.clone(),
                value_type,
                confidence,
                resolved_at,
                valid_from: resolved_at, // Use resolved_at as valid_from for current values
                valid_to: None,          // Current values don't have valid_to
                explanation: None,       // Could be added to SPARQL query if needed
            },
        );
    }

    let overall_confidence = if !fields.is_empty() {
        total_confidence / fields.len() as f64
    } else {
        0.0
    };

    let response = ResolvedEntityResponse {
        entity_id: entity_id.clone(),
        fields,
        overall_confidence,
        conflict_count,
        requires_review,
        created_at: latest_resolved_at.unwrap_or_else(Utc::now),
        low_confidence_fields,
        conflicting_fields,
    };

    tracing::info!(
        "Retrieved golden record for entity {} with {} fields, {} conflicts",
        entity_id,
        response.fields.len(),
        response.conflict_count
    );

    Ok(Json(response))
}

/// List conflicts requiring human review
#[utoipa::path(
    get,
    path = "/api/v1/conflicts/requiring-review",
    tag = "Field Lineage",
    responses(
        (status = 200, description = "Conflicts list retrieved successfully", body = ConflictsListResponse),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "RDF store not available")
    )
)]
pub async fn list_conflicts_requiring_review(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ConflictsListResponse>, (StatusCode, String)> {
    tracing::info!("Listing conflicts requiring review");

    let rdf_store = state.rdf_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "RDF store not available".to_string(),
        )
    })?;

    let storage = FieldLineageStore::new();
    let sparql = storage.query_conflicts_requiring_review();

    // Execute SPARQL query for conflict retrieval
    let results = rdf_store.query(&sparql).map_err(|e| {
        tracing::error!("Conflicts query failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query failed: {}", e),
        )
    })?;

    tracing::debug!("Retrieved {} conflicts requiring review", results.len());

    // Extract bindings from SPARQL JSON result
    let bindings = results
        .get(0)
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid SPARQL result format".to_string(),
            )
        })?;

    // Parse each conflict
    let mut conflicts = Vec::new();
    for binding in bindings {
        if let (
            Some(entity_id),
            Some(field_name),
            Some(severity),
            Some(reason),
            Some(resolved_at),
        ) = (
            get_sparql_string(binding, "entityId"),
            get_sparql_string(binding, "fieldName"),
            get_sparql_string(binding, "severity"),
            get_sparql_string(binding, "reason"),
            get_sparql_datetime(binding, "resolvedAt"),
        ) {
            conflicts.push(ConflictListItem {
                entity_id,
                field_name,
                severity: parse_conflict_severity(&severity),
                reason,
                resolved_at,
            });
        }
    }

    let response = ConflictsListResponse {
        total: conflicts.len(),
        conflicts,
    };

    Ok(Json(response))
}

/// Resolve a field conflict with explicit strategy
#[utoipa::path(
    post,
    path = "/api/v1/entities/{entity_id}/fields/{field_name}/resolve",
    tag = "Field Lineage",
    params(
        ("entity_id" = String, Path, description = "Entity ID"),
        ("field_name" = String, Path, description = "Field name")
    ),
    request_body = ResolveFieldConflictRequest,
    responses(
        (status = 200, description = "Field conflict resolved successfully", body = FieldResolutionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn resolve_field_conflict(
    State(state): State<Arc<ApiState>>,
    Path((entity_id, field_name)): Path<(String, String)>,
    Json(request): Json<ResolveFieldConflictRequest>,
) -> Result<Json<FieldResolutionResponse>, (StatusCode, String)> {
    tracing::info!(
        "Resolving field conflict for entity {}, field {}",
        entity_id,
        field_name
    );

    // Create field resolver
    let resolver = FieldResolver::new();

    // Convert input source values
    let source_values: Vec<SourceValue> = request
        .source_values
        .into_iter()
        .enumerate()
        .map(|(idx, input)| SourceValue {
            id: format!("src_{}_{}", field_name, idx),
            value: input.value,
            source_system: input.source_system,
            source_timestamp: input.source_timestamp,
            source_authority: input.source_authority,
            confidence: input.confidence,
            vote_count: 0,
            vote_weight: 1.0,
            metadata: input.metadata.unwrap_or_default(),
        })
        .collect();

    // Convert voting strategy
    let voting_strategy = request.voting_strategy.as_ref().map(|vs| {
        let mut params = serde_json::Map::new();
        if let Some(decay_rate) = vs.decay_rate {
            params.insert("decay_rate".to_string(), serde_json::json!(decay_rate));
        }
        if let Some(ref_time) = vs.reference_time {
            params.insert("reference_time".to_string(), serde_json::json!(ref_time));
        }

        VotingStrategy {
            strategy_type: vs.strategy_type,
            parameters: serde_json::Value::Object(params),
            description: format!("{:?} voting strategy", vs.strategy_type),
        }
    });

    // Resolve field
    let resolution = resolver
        .resolve_field(&entity_id, &field_name, source_values, voting_strategy)
        .map_err(|e| {
            tracing::error!("Failed to resolve field: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Resolution failed: {}", e),
            )
        })?;

    // Convert to response
    let response = resolution_to_response(&resolution);

    tracing::info!(
        "Resolved field {} for entity {} with confidence {:.2}",
        field_name,
        entity_id,
        response.selected_value.vote_weight
    );

    // Persist resolution to RDF store for lineage tracking
    if let Some(rdf_store) = &state.rdf_store {
        let storage = FieldLineageStore::new();
        let sparql = storage.insert_field_resolution_query(&resolution);

        match rdf_store.update(&sparql) {
            Ok(_) => {
                tracing::debug!("Persisted field resolution to RDF store");
            }
            Err(e) => {
                tracing::error!("Failed to persist field resolution to RDF: {}", e);
                // Don't fail the request - resolution is already computed
            }
        }
    }

    Ok(Json(response))
}

// Helper functions to convert core types to response types

/// Infer JSON value type as string
fn infer_json_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

/// Parse strategy type from URI string
fn parse_strategy_type(uri: &str) -> StrategyType {
    if uri.contains("frequency") {
        StrategyType::Frequency
    } else if uri.contains("time-decay") || uri.contains("timedecay") {
        StrategyType::TimeDecay
    } else if uri.contains("authority") {
        StrategyType::Authority
    } else if uri.contains("ensemble") {
        StrategyType::Ensemble
    } else if uri.contains("ml") || uri.contains("prediction") {
        StrategyType::MlPrediction
    } else {
        StrategyType::Custom
    }
}

/// Parse conflict severity from string
fn parse_conflict_severity(s: &str) -> ConflictSeverity {
    match s.to_lowercase().as_str() {
        "low" => ConflictSeverity::Low,
        "medium" => ConflictSeverity::Medium,
        "high" => ConflictSeverity::High,
        "critical" => ConflictSeverity::Critical,
        _ => ConflictSeverity::Medium, // Default
    }
}

fn field_value_to_response(fv: &FieldValue) -> FieldValueResponse {
    FieldValueResponse {
        field_name: fv.field_name.clone(),
        value: fv.value.clone(),
        value_type: fv.value_type.clone(),
        confidence: fv.confidence,
        resolved_at: fv.resolved_at,
        valid_from: fv.valid_from,
        valid_to: fv.valid_to,
        explanation: fv.explanation.clone(),
    }
}

fn resolution_to_response(resolution: &FieldResolution) -> FieldResolutionResponse {
    FieldResolutionResponse {
        id: resolution.id.clone(),
        resolved_at: resolution.resolved_at,
        resolved_by: resolution.resolved_by.clone(),
        strategy: VotingStrategyResponse {
            strategy_type: resolution.strategy.strategy_type,
            config: resolution.strategy.parameters.clone(),
        },
        source_values: resolution
            .source_values
            .iter()
            .map(source_value_to_response)
            .collect(),
        selected_value: source_value_to_response(&resolution.selected_value),
        rejected_values: resolution
            .rejected_values
            .iter()
            .map(source_value_to_response)
            .collect(),
        explanation: resolution.explanation.clone(),
        conflict: resolution.conflict.as_ref().map(conflict_to_response),
    }
}

fn source_value_to_response(sv: &SourceValue) -> SourceValueResponse {
    SourceValueResponse {
        id: sv.id.clone(),
        value: sv.value.clone(),
        source_system: sv.source_system.clone(),
        source_timestamp: sv.source_timestamp,
        source_authority: sv.source_authority,
        confidence: sv.confidence,
        vote_count: sv.vote_count,
        vote_weight: sv.vote_weight,
        metadata: sv.metadata.clone(),
    }
}

fn conflict_to_response(conflict: &FieldConflict) -> FieldConflictResponse {
    FieldConflictResponse {
        id: conflict.id.clone(),
        severity: conflict.severity,
        reason: conflict.reason.clone(),
        requires_review: conflict.requires_review,
        conflicting_values: conflict
            .conflicting_values
            .iter()
            .map(source_value_to_response)
            .collect(),
    }
}

/// Get cache metrics
///
/// Returns cache hit/miss rates, size, evictions, and other performance metrics.
/// Useful for monitoring and tuning the streaming golden record cache.
#[utoipa::path(
    get,
    path = "/api/v1/resolved-entities/cache/metrics",
    tag = "Field Lineage",
    responses(
        (status = 200, description = "Cache metrics retrieved successfully", body = CacheMetricsResponse),
        (status = 503, description = "Cache not available")
    )
)]
pub async fn get_cache_metrics(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<CacheMetricsResponse>, (StatusCode, String)> {
    tracing::debug!("Getting cache metrics");

    // Get cache from state
    let cache = state.resolved_entity_cache.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Cache not available".to_string(),
        )
    })?;

    // Get metrics
    let metrics = cache.metrics();

    let response = CacheMetricsResponse {
        hits: metrics.hits.load(std::sync::atomic::Ordering::Relaxed),
        misses: metrics.misses.load(std::sync::atomic::Ordering::Relaxed),
        hit_rate: metrics.hit_rate(),
        insertions: metrics
            .insertions
            .load(std::sync::atomic::Ordering::Relaxed),
        ttl_evictions: metrics
            .ttl_evictions
            .load(std::sync::atomic::Ordering::Relaxed),
        size_evictions: metrics
            .size_evictions
            .load(std::sync::atomic::Ordering::Relaxed),
        current_size: metrics.size(),
        max_size: 10_000, // From default config
    };

    tracing::debug!(
        "Cache metrics: hit_rate={:.2}%, size={}, hits={}, misses={}",
        response.hit_rate * 100.0,
        response.current_size,
        response.hits,
        response.misses
    );

    Ok(Json(response))
}

/// Cache metrics response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct CacheMetricsResponse {
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
    /// Total insertions
    pub insertions: u64,
    /// TTL-based evictions
    pub ttl_evictions: u64,
    /// Size-based evictions (LRU)
    pub size_evictions: u64,
    /// Current cache size (number of entries)
    pub current_size: usize,
    /// Maximum cache size
    pub max_size: usize,
}
