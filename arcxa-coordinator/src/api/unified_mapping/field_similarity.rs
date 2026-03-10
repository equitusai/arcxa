// graphica-coordinator/src/api/unified_mapping/field_similarity.rs
//! Field similarity and mapping suggestion endpoints

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use graphica_core::inference::mapping::{DatasetSchema, FieldMapper, MappingSuggestions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use utoipa;

use crate::api::ApiState;

/// Request to suggest field mappings between datasets
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SuggestMappingsRequest {
    /// List of dataset IDs or inline dataset schemas
    pub datasets: Vec<DatasetInput>,
}

/// Input dataset (either by ID or inline schema)
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum DatasetInput {
    /// Reference to existing dataset by ID
    ById { dataset_id: String },

    /// Inline dataset schema
    Inline(DatasetSchema),
}

/// Response with mapping suggestions
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SuggestMappingsResponse {
    /// All suggestions categorized by confidence
    pub suggestions: MappingSuggestions,

    /// Number of dataset pairs analyzed
    pub datasets_analyzed: usize,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Suggest field mappings using AI/ML-powered multi-dimensional similarity analysis
///
/// Analyzes multiple datasets to automatically suggest field mappings based on:
/// - Name similarity (Levenshtein distance, phonetic matching)
/// - Data type compatibility
/// - Statistical profile similarity (cardinality, distribution)
/// - Semantic type inference
/// - Contextual neighbor analysis
///
/// Results are categorized into auto-mapped (>0.90 confidence), recommended (0.70-0.90),
/// and possible (0.50-0.70) matches to facilitate human review and approval.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/suggest",
    request_body = SuggestMappingsRequest,
    responses(
        (status = 200, description = "Field mapping suggestions generated successfully with confidence scores", body = SuggestMappingsResponse),
        (status = 400, description = "Invalid request - requires at least 2 datasets", body = ApiErrorResponse),
        (status = 500, description = "Internal server error - failed to analyze mappings", body = ApiErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn suggest_field_mappings(
    State(_state): State<Arc<ApiState>>,
    Json(request): Json<SuggestMappingsRequest>,
) -> Result<Response, ApiError> {
    info!(
        "Suggesting field mappings for {} datasets",
        request.datasets.len()
    );

    let start_time = std::time::Instant::now();

    // TODO: Load dataset schemas from database if referenced by ID
    // For now, we only support inline schemas
    let mut schemas = Vec::new();
    for input in request.datasets {
        match input {
            DatasetInput::Inline(schema) => schemas.push(schema),
            DatasetInput::ById { dataset_id } => {
                // TODO: Load from database/catalog
                error!("Dataset loading by ID not yet implemented: {}", dataset_id);
                return Err(ApiError::NotImplemented(
                    "Dataset loading by ID not yet implemented. Please provide inline schemas."
                        .to_string(),
                ));
            }
        }
    }

    if schemas.len() < 2 {
        return Err(ApiError::BadRequest(
            "At least 2 datasets are required for mapping suggestions".to_string(),
        ));
    }

    // Create field mapper
    let mapper = FieldMapper::new();

    // Find all mappings between dataset pairs
    let mut all_similarities = Vec::new();
    let mut pairs_analyzed = 0;

    for i in 0..schemas.len() {
        for j in (i + 1)..schemas.len() {
            info!(
                "Analyzing {} → {}",
                schemas[i].dataset_name, schemas[j].dataset_name
            );

            let mappings = mapper
                .find_mappings(&schemas[i], &schemas[j])
                .map_err(|e| {
                    error!("Failed to find mappings: {}", e);
                    ApiError::InternalError(format!("Failed to analyze mappings: {}", e))
                })?;

            pairs_analyzed += 1;

            // Flatten to individual field similarities
            for mapping in mappings {
                all_similarities.extend(mapping.candidates);
            }
        }
    }

    // Categorize by confidence
    let suggestions = mapper.categorize_mappings(all_similarities);

    let processing_time_ms = start_time.elapsed().as_millis() as u64;

    info!(
        "Found {} auto-mapped, {} recommended, {} possible (processed in {}ms)",
        suggestions.auto_mapped.len(),
        suggestions.recommended.len(),
        suggestions.possible.len(),
        processing_time_ms
    );

    Ok((
        StatusCode::OK,
        Json(SuggestMappingsResponse {
            suggestions,
            datasets_analyzed: pairs_analyzed,
            processing_time_ms,
        }),
    )
        .into_response())
}

/// API error types
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    NotImplemented(String),
    InternalError(String),
}

/// API error response schema for OpenAPI documentation
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApiErrorResponse {
    /// Error message
    pub error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::NotImplemented(msg) => (StatusCode::NOT_IMPLEMENTED, msg),
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (
            status,
            Json(serde_json::json!({
                "error": message
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::inference::mapping::{
        DataType, FieldMetadata, FieldProfile, ValueDistribution,
    };

    fn create_test_field(name: &str, distinct: u64, total: u64) -> FieldMetadata {
        FieldMetadata {
            qualified_name: format!("test.{}", name),
            column_name: name.to_string(),
            source_id: "test".to_string(),
            data_type: DataType::Integer,
            profile: FieldProfile {
                distinct_count: distinct,
                total_rows: total,
                null_percentage: 0.0,
                distribution: ValueDistribution {
                    min: Some("1".to_string()),
                    max: Some(total.to_string()),
                    ..Default::default()
                },
                samples: vec![],
            },
            semantic_type: None,
            position: 0,
            neighbors: vec![],
        }
    }

    #[test]
    fn test_suggest_request_serialization() {
        let request = SuggestMappingsRequest {
            datasets: vec![
                DatasetInput::ById {
                    dataset_id: "customers".to_string(),
                },
                DatasetInput::Inline(DatasetSchema {
                    dataset_id: "orders".to_string(),
                    dataset_name: "Orders".to_string(),
                    fields: vec![create_test_field("order_id", 1000, 1000)],
                }),
            ],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("customers"));
        assert!(json.contains("orders"));
    }

    #[test]
    fn test_field_mapper_integration() {
        let mapper = FieldMapper::new();

        let schema1 = DatasetSchema {
            dataset_id: "customers".to_string(),
            dataset_name: "Customers".to_string(),
            fields: vec![
                create_test_field("customer_id", 10000, 10000),
                create_test_field("email", 10000, 10000),
            ],
        };

        let schema2 = DatasetSchema {
            dataset_id: "orders".to_string(),
            dataset_name: "Orders".to_string(),
            fields: vec![
                create_test_field("cust_id", 8500, 10000),
                create_test_field("customer_email", 8500, 10000),
            ],
        };

        let mappings = mapper.find_mappings(&schema1, &schema2).unwrap();
        assert!(!mappings.is_empty(), "Should find some mappings");
    }
}
