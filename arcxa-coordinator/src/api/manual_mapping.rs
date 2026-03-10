// API Layer for Manual Field Mappings
use crate::api::auth::Claims;
use crate::mapping::manual::{store::*, types::*};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use utoipa::ToSchema;
use uuid::Uuid;

/// REST API Router
pub fn manual_mapping_router(store: Arc<ManualMappingStore>) -> Router {
    Router::new()
        // Core CRUD operations
        .route("/api/v1/mappings/manual", post(create_mapping))
        .route("/api/v1/mappings/manual/:id", get(get_mapping))
        .route("/api/v1/mappings/manual/:id", put(update_mapping))
        .route("/api/v1/mappings/manual/:id", delete(delete_mapping))

        // Bulk operations
        .route("/api/v1/mappings/manual/bulk", post(bulk_create_mappings))
        .route("/api/v1/mappings/manual/import", post(import_mappings))
        .route("/api/v1/mappings/manual/export", get(export_mappings))

        // Query operations
        .route("/api/v1/mappings/manual/search", post(search_mappings))
        .route("/api/v1/mappings/manual/suggest", post(suggest_mappings))

        // Stats and learning
        .route("/api/v1/mappings/manual/:id/usage", post(track_usage))
        .route("/api/v1/mappings/manual/stats", get(get_mapping_stats))

        .with_state(store)
}

/// Create a new manual mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingRequest {
    pub source_id: Option<String>,
    pub table_name: String,
    pub field_name: String,
    pub target_field_uri: String,
    pub notes: Option<String>,
    pub field_metadata: Option<FieldCharacteristics>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateMappingResponse {
    pub id: String,
    pub mapping: ManualFieldMapping,
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual",
    request_body = CreateMappingRequest,
    responses(
        (status = 200, description = "Mapping created successfully", body = CreateMappingResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn create_mapping(
    State(store): State<Arc<ManualMappingStore>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<CreateMappingRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Extract user ID from auth claims (falls back to "anonymous" if auth disabled)
    let user_id = claims
        .as_ref()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    let mapping = ManualFieldMapping {
        id: Uuid::new_v4().to_string(),
        source_context: SourceContext {
            source_id: request.source_id,
            table_name: request.table_name,
            field_name: request.field_name,
            field_metadata: request.field_metadata,
        },
        target_field_uri: request.target_field_uri,
        confidence: 1.0, // Always 1.0 for manual mappings
        created_by: user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        notes: request.notes,
        usage_stats: UsageStats::default(),
    };

    store
        .store_mapping(mapping.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CreateMappingResponse {
        id: mapping.id.clone(),
        mapping,
    }))
}

/// Suggest mappings based on context
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingsRequest {
    pub source_id: Option<String>,
    pub table_name: String,
    pub field_name: String,
    pub field_metadata: Option<FieldCharacteristics>,
    pub limit: Option<usize>,
    pub include_ml_suggestions: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SuggestMappingsResponse {
    pub suggestions: Vec<MappingSuggestion>,
    pub has_exact_match: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual/suggest",
    request_body = SuggestMappingsRequest,
    responses(
        (status = 200, description = "Mapping suggestions returned", body = SuggestMappingsResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn suggest_mappings(
    State(store): State<Arc<ManualMappingStore>>,
    Json(request): Json<SuggestMappingsRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let context = SourceContext {
        source_id: request.source_id,
        table_name: request.table_name,
        field_name: request.field_name,
        field_metadata: request.field_metadata,
    };

    let limit = request.limit.unwrap_or(5);

    let mut suggestions = store
        .find_similar_mappings(&context, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Check if we have an exact match
    let has_exact_match = suggestions
        .iter()
        .any(|s| matches!(s.suggestion_reason, SuggestionReason::ExactFieldMatch { .. }));

    // Add ML suggestions if requested and no exact match
    if request.include_ml_suggestions.unwrap_or(false) && !has_exact_match {
        // TODO: Integrate with ML mapping engine when available
        // This would call an ML model service to predict likely target fields
        // based on field name, data type, sample values, and detected patterns
        //
        // Example integration:
        // if let Some(ref ml_engine) = state.ml_mapping_engine {
        //     let ml_suggestions = ml_engine.get_mapping_candidates(&context).await?;
        //     for ml_suggestion in ml_suggestions {
        //         suggestions.push(MappingSuggestion {
        //             mapping: ManualFieldMapping { /* ... */ },
        //             suggestion_reason: SuggestionReason::MLModel {
        //                 model_name: ml_suggestion.model_name,
        //                 confidence: ml_suggestion.confidence,
        //             },
        //             relevance_score: ml_suggestion.confidence,
        //         });
        //     }
        // }
    }

    Ok(Json(SuggestMappingsResponse {
        suggestions,
        has_exact_match,
    }))
}

/// Track usage of a mapping (for learning)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrackUsageRequest {
    pub action: String, // "applied", "accepted", "rejected"
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual/{id}/usage",
    request_body = TrackUsageRequest,
    params(
        ("id" = String, Path, description = "Mapping ID")
    ),
    responses(
        (status = 204, description = "Usage tracked successfully"),
        (status = 400, description = "Invalid action"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn track_usage(
    State(store): State<Arc<ManualMappingStore>>,
    Path(id): Path<String>,
    Json(request): Json<TrackUsageRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let stat_type = match request.action.as_str() {
        "applied" => UsageStatType::Applied,
        "accepted" => UsageStatType::Accepted,
        "rejected" => UsageStatType::Rejected,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    store
        .update_usage_stats(&id, stat_type)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Bulk create mappings
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkCreateRequest {
    pub mappings: Vec<CreateMappingRequest>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkCreateResponse {
    pub created: Vec<String>,
    pub failed: Vec<BulkCreateError>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkCreateError {
    pub index: usize,
    pub error: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual/bulk",
    request_body = BulkCreateRequest,
    responses(
        (status = 200, description = "Bulk create completed", body = BulkCreateResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn bulk_create_mappings(
    State(store): State<Arc<ManualMappingStore>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<BulkCreateRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Extract user ID from auth claims
    let user_id = claims
        .as_ref()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    let mut created = Vec::new();
    let mut failed = Vec::new();

    for (index, req) in request.mappings.into_iter().enumerate() {
        let mapping = ManualFieldMapping {
            id: Uuid::new_v4().to_string(),
            source_context: SourceContext {
                source_id: req.source_id,
                table_name: req.table_name,
                field_name: req.field_name,
                field_metadata: req.field_metadata,
            },
            target_field_uri: req.target_field_uri,
            confidence: 1.0,
            created_by: user_id.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: req.notes,
            usage_stats: UsageStats::default(),
        };

        match store.store_mapping(mapping.clone()).await {
            Ok(_) => created.push(mapping.id),
            Err(e) => failed.push(BulkCreateError {
                index,
                error: e.to_string(),
            }),
        }
    }

    Ok(Json(BulkCreateResponse { created, failed }))
}

/// Search mappings with filters
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SearchMappingsRequest {
    pub source_id: Option<String>,
    pub table_name: Option<String>,
    pub field_name_pattern: Option<String>,
    pub target_field_uri: Option<String>,
    pub created_by: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual/search",
    request_body = SearchMappingsRequest,
    responses(
        (status = 200, description = "Search results returned", body = serde_json::Value),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn search_mappings(
    State(store): State<Arc<ManualMappingStore>>,
    Json(request): Json<SearchMappingsRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // For now, use RocksDB-based search instead of SPARQL
    // SPARQL would require RdfStore::query() which may not be implemented yet

    let limit = request.limit.unwrap_or(100);
    let offset = request.offset.unwrap_or(0);

    // Get all mappings from RocksDB and filter
    let all_mappings = store
        .bulk_export(request.created_by.as_ref().map(|user| ExportFilter::ByUser(user.clone())))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Apply filters
    let mut filtered: Vec<ManualFieldMapping> = all_mappings.mappings.into_iter()
        .filter(|m| {
            // Filter by source_id
            if let Some(ref source_id) = request.source_id {
                if m.source_context.source_id.as_ref() != Some(source_id) {
                    return false;
                }
            }

            // Filter by table_name
            if let Some(ref table_name) = request.table_name {
                if &m.source_context.table_name != table_name {
                    return false;
                }
            }

            // Filter by field_name_pattern (contains match)
            if let Some(ref pattern) = request.field_name_pattern {
                if !m.source_context.field_name.to_lowercase().contains(&pattern.to_lowercase()) {
                    return false;
                }
            }

            // Filter by target_field_uri
            if let Some(ref target_uri) = request.target_field_uri {
                if &m.target_field_uri != target_uri {
                    return false;
                }
            }

            true
        })
        .collect();

    let total = filtered.len();

    // Apply pagination
    filtered = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Ok(Json(serde_json::json!({
        "mappings": filtered,
        "total": total,
        "limit": limit,
        "offset": offset
    })))
}

// gRPC Service Implementation
pub mod grpc {
    use super::*;
    use crate::proto::manual_mapping_service_server::{
        ManualMappingService, ManualMappingServiceServer,
    };
    use crate::proto::{
        CreateMappingGrpcRequest, CreateMappingGrpcResponse, GetMappingRequest,
        GetMappingResponse, SuggestMappingsGrpcRequest, SuggestMappingsGrpcResponse,
        TrackUsageGrpcRequest, Empty,
    };

    pub struct ManualMappingGrpcService {
        store: Arc<ManualMappingStore>,
    }

    impl ManualMappingGrpcService {
        pub fn new(store: Arc<ManualMappingStore>) -> ManualMappingServiceServer<Self> {
            ManualMappingServiceServer::new(Self { store })
        }
    }

    #[tonic::async_trait]
    impl ManualMappingService for ManualMappingGrpcService {
        async fn create_mapping(
            &self,
            request: Request<CreateMappingGrpcRequest>,
        ) -> Result<Response<CreateMappingGrpcResponse>, Status> {
            let req = request.into_inner();

            // TODO: Extract user from gRPC metadata/auth
            // Example: let user_id = request.metadata().get("user-id").and_then(|v| v.to_str().ok());
            let user_id = "grpc_user".to_string();

            let mapping = ManualFieldMapping {
                id: Uuid::new_v4().to_string(),
                source_context: SourceContext {
                    source_id: req.source_id,
                    table_name: req.table_name,
                    field_name: req.field_name,
                    // TODO: Convert FieldCharacteristics from proto when proto is defined
                    // field_metadata: req.field_metadata.map(|fm| FieldCharacteristics {
                    //     data_type: fm.data_type,
                    //     sample_values: fm.sample_values,
                    //     detected_pattern: fm.detected_pattern,
                    //     profile_hash: fm.profile_hash,
                    // }),
                    field_metadata: None,
                },
                target_field_uri: req.target_field_uri,
                confidence: 1.0,
                created_by: user_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                notes: req.notes,
                usage_stats: UsageStats::default(),
            };

            self.store
                .store_mapping(mapping.clone())
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            // TODO: Convert ManualFieldMapping to proto when proto message is defined
            // Example:
            // CreateMappingGrpcResponse {
            //     id: mapping.id.clone(),
            //     mapping: Some(MappingProto {
            //         id: mapping.id,
            //         source_context: Some(SourceContextProto { /* ... */ }),
            //         target_field_uri: mapping.target_field_uri,
            //         confidence: mapping.confidence,
            //         created_by: mapping.created_by,
            //         created_at: Some(prost_types::Timestamp { /* ... */ }),
            //         /* ... */
            //     }),
            // }
            Ok(Response::new(CreateMappingGrpcResponse {
                id: mapping.id.clone(),
            }))
        }

        async fn get_mapping(
            &self,
            request: Request<GetMappingRequest>,
        ) -> Result<Response<GetMappingResponse>, Status> {
            let id = request.into_inner().id;

            let mapping = self
                .store
                .get_mapping(&id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("Mapping not found"))?;

            // TODO: Convert ManualFieldMapping to GetMappingResponse proto
            // Example:
            // GetMappingResponse {
            //     mapping: Some(MappingProto {
            //         id: mapping.id,
            //         source_context: Some(SourceContextProto {
            //             source_id: mapping.source_context.source_id.unwrap_or_default(),
            //             table_name: mapping.source_context.table_name,
            //             field_name: mapping.source_context.field_name,
            //             field_metadata: mapping.source_context.field_metadata.map(|fm| /* ... */),
            //         }),
            //         target_field_uri: mapping.target_field_uri,
            //         confidence: mapping.confidence,
            //         created_by: mapping.created_by,
            //         created_at: Some(timestamp_from_datetime(mapping.created_at)),
            //         updated_at: Some(timestamp_from_datetime(mapping.updated_at)),
            //         notes: mapping.notes.unwrap_or_default(),
            //         usage_stats: Some(UsageStatsProto { /* ... */ }),
            //     }),
            // }
            Ok(Response::new(GetMappingResponse {}))
        }

        async fn suggest_mappings(
            &self,
            request: Request<SuggestMappingsGrpcRequest>,
        ) -> Result<Response<SuggestMappingsGrpcResponse>, Status> {
            let req = request.into_inner();

            let context = SourceContext {
                source_id: req.source_id,
                table_name: req.table_name,
                field_name: req.field_name,
                // TODO: Convert FieldCharacteristics from proto when defined
                // field_metadata: req.field_metadata.map(|fm| FieldCharacteristics { /* ... */ }),
                field_metadata: None,
            };

            let suggestions = self
                .store
                .find_similar_mappings(&context, req.limit as usize)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            // TODO: Convert Vec<MappingSuggestion> to SuggestMappingsGrpcResponse proto
            // Example:
            // SuggestMappingsGrpcResponse {
            //     suggestions: suggestions.into_iter().map(|s| SuggestionProto {
            //         mapping: Some(MappingProto { /* ... from s.mapping */ }),
            //         suggestion_reason: match s.suggestion_reason {
            //             SuggestionReason::ExactFieldMatch { .. } => "exact_match",
            //             SuggestionReason::SimilarFieldName { .. } => "similar_field",
            //             /* ... */
            //         }.to_string(),
            //         relevance_score: s.relevance_score,
            //     }).collect(),
            //     has_exact_match: suggestions.iter().any(|s| matches!(
            //         s.suggestion_reason,
            //         SuggestionReason::ExactFieldMatch { .. }
            //     )),
            // }
            Ok(Response::new(SuggestMappingsGrpcResponse {}))
        }

        async fn track_usage(
            &self,
            request: Request<TrackUsageGrpcRequest>,
        ) -> Result<Response<Empty>, Status> {
            let req = request.into_inner();

            let stat_type = match req.action.as_str() {
                "applied" => UsageStatType::Applied,
                "accepted" => UsageStatType::Accepted,
                "rejected" => UsageStatType::Rejected,
                _ => return Err(Status::invalid_argument("Invalid action")),
            };

            self.store
                .update_usage_stats(&req.mapping_id, stat_type)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            Ok(Response::new(Empty {}))
        }
    }
}

// Helper functions
#[utoipa::path(
    get,
    path = "/api/v1/mappings/manual/{id}",
    params(
        ("id" = String, Path, description = "Mapping ID")
    ),
    responses(
        (status = 200, description = "Mapping found", body = ManualFieldMapping),
        (status = 404, description = "Mapping not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn get_mapping(
    State(store): State<Arc<ManualMappingStore>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let mapping = store
        .get_mapping(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(mapping))
}

#[utoipa::path(
    put,
    path = "/api/v1/mappings/manual/{id}",
    request_body = CreateMappingRequest,
    params(
        ("id" = String, Path, description = "Mapping ID")
    ),
    responses(
        (status = 200, description = "Mapping updated successfully", body = ManualFieldMapping),
        (status = 404, description = "Mapping not found"),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn update_mapping(
    State(store): State<Arc<ManualMappingStore>>,
    Path(id): Path<String>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<CreateMappingRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Check if mapping exists
    let existing = store
        .get_mapping(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Extract user ID from auth claims
    let user_id = claims
        .as_ref()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    // Create updated mapping (preserve ID, created_at, and usage_stats)
    let updated_mapping = ManualFieldMapping {
        id: existing.id.clone(),
        source_context: SourceContext {
            source_id: request.source_id,
            table_name: request.table_name,
            field_name: request.field_name,
            field_metadata: request.field_metadata,
        },
        target_field_uri: request.target_field_uri,
        confidence: 1.0, // Always 1.0 for manual mappings
        created_by: existing.created_by, // Preserve original creator
        created_at: existing.created_at, // Preserve creation time
        updated_at: chrono::Utc::now(),
        notes: request.notes,
        usage_stats: existing.usage_stats, // Preserve usage stats
    };

    // Store updated mapping (will overwrite existing)
    store
        .store_mapping(updated_mapping.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(updated_mapping))
}

#[utoipa::path(
    delete,
    path = "/api/v1/mappings/manual/{id}",
    params(
        ("id" = String, Path, description = "Mapping ID")
    ),
    responses(
        (status = 204, description = "Mapping deleted successfully"),
        (status = 404, description = "Mapping not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn delete_mapping(
    State(store): State<Arc<ManualMappingStore>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // Delete mapping from store
    // Note: ManualMappingStore::delete_mapping already handles:
    // - RocksDB index cleanup
    // - Cache eviction
    // - RDF triple deletion (soft delete via marking)
    let deleted = store
        .delete_mapping(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/mappings/manual/import",
    request_body = MappingImportExport,
    responses(
        (status = 200, description = "Import completed", body = ImportStats),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn import_mappings(
    State(store): State<Arc<ManualMappingStore>>,
    Json(import): Json<MappingImportExport>,
) -> Result<impl IntoResponse, StatusCode> {
    let stats = store
        .bulk_import(import)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(stats))
}

#[utoipa::path(
    get,
    path = "/api/v1/mappings/manual/export",
    params(
        ("user" = Option<String>, Query, description = "Filter by user"),
        ("source" = Option<String>, Query, description = "Filter by source")
    ),
    responses(
        (status = 200, description = "Mappings exported", body = MappingImportExport),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn export_mappings(
    State(store): State<Arc<ManualMappingStore>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, StatusCode> {
    let filter = if let Some(user) = params.get("user") {
        Some(ExportFilter::ByUser(user.clone()))
    } else if let Some(source) = params.get("source") {
        Some(ExportFilter::BySource(source.clone()))
    } else {
        None
    };

    let export = store
        .bulk_export(filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(export))
}

#[utoipa::path(
    get,
    path = "/api/v1/mappings/manual/stats",
    responses(
        (status = 200, description = "Mapping statistics", body = serde_json::Value),
        (status = 500, description = "Internal server error")
    ),
    tag = "Manual Mapping"
)]
async fn get_mapping_stats(
    State(store): State<Arc<ManualMappingStore>>,
) -> Result<impl IntoResponse, StatusCode> {
    // Export all mappings to calculate statistics
    let all_mappings = store
        .bulk_export(None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Aggregate statistics
    let total_mappings = all_mappings.mappings.len();
    let mut users = std::collections::HashSet::new();
    let mut target_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Track most used mappings
    let mut mappings_by_usage: Vec<_> = all_mappings.mappings.iter().collect();
    mappings_by_usage.sort_by(|a, b| {
        b.usage_stats.apply_count.cmp(&a.usage_stats.apply_count)
    });

    for mapping in &all_mappings.mappings {
        users.insert(mapping.created_by.clone());
        *target_counts.entry(mapping.target_field_uri.clone()).or_insert(0) += 1;
    }

    // Get top 10 most used mappings
    let most_used_mappings: Vec<_> = mappings_by_usage.iter()
        .take(10)
        .map(|m| serde_json::json!({
            "id": m.id,
            "source_context": m.source_context,
            "target_field_uri": m.target_field_uri,
            "apply_count": m.usage_stats.apply_count,
            "accept_count": m.usage_stats.accept_count,
            "reject_count": m.usage_stats.reject_count,
        }))
        .collect();

    // Get top 10 most mapped targets
    let mut target_counts_vec: Vec<_> = target_counts.into_iter().collect();
    target_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let most_mapped_targets: Vec<_> = target_counts_vec.iter()
        .take(10)
        .map(|(uri, count)| serde_json::json!({
            "target_uri": uri,
            "mapping_count": count
        }))
        .collect();

    Ok(Json(serde_json::json!({
        "total_mappings": total_mappings,
        "total_users": users.len(),
        "unique_sources": all_mappings.statistics.unique_sources,
        "unique_tables": all_mappings.statistics.unique_tables,
        "unique_fields": all_mappings.statistics.unique_fields,
        "most_used_mappings": most_used_mappings,
        "most_mapped_targets": most_mapped_targets
    })))
}