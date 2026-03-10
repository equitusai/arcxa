//! Model Registry Handler Functions
//!
//! HTTP handlers for ML model registry operations in the orchestration system.
//! These handlers manage external ML models that can be invoked during workflow execution.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use graphica_core::orchestration::ml::registry::FeatureSchema;
use graphica_core::orchestration::ml::{
    ModelEndpoint, ModelMetadata, ModelProtocol, ServingFramework,
};
use std::sync::Arc;

/// Register a new ML model in the orchestration registry
/// POST /api/v1/orchestration/models
pub async fn register_model_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterOrchestrationModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let registry = state.model_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model registry not available".to_string(),
        )
    })?;

    let protocol = match request.endpoint.protocol.as_str() {
        "http" => ModelProtocol::Http,
        "grpc" => ModelProtocol::Grpc,
        "lambda" => ModelProtocol::Lambda,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown protocol: {}", request.endpoint.protocol),
            ))
        }
    };

    let framework = match request.framework.as_str() {
        "tensorflow" => ServingFramework::TensorFlowServing,
        "torch" => ServingFramework::TorchServe,
        "sagemaker" => ServingFramework::SageMaker,
        "custom" => ServingFramework::Custom,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown framework: {}", request.framework),
            ))
        }
    };

    let input_schema = request
        .input_schema
        .into_iter()
        .map(|s| FeatureSchema {
            name: s.name,
            data_type: parse_feature_data_type(&s.data_type),
            required: s.required,
        })
        .collect();

    let model = ModelMetadata {
        id: request.id.clone(),
        name: request.name,
        version: request.version,
        endpoint: ModelEndpoint {
            protocol,
            url: request.endpoint.url,
            timeout_ms: request.endpoint.timeout_ms,
            headers: request.endpoint.headers,
        },
        framework,
        input_schema,
        output_schema: request.output_schema,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    registry.register(model).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Registration failed: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "model_id": request.id,
        "status": "registered"
    })))
}

/// Parse feature data type from string
fn parse_feature_data_type(s: &str) -> graphica_core::orchestration::ml::registry::FeatureDataType {
    use graphica_core::orchestration::ml::registry::FeatureDataType;
    match s {
        "string" => FeatureDataType::String,
        "integer" => FeatureDataType::Integer,
        "float" => FeatureDataType::Float,
        "boolean" => FeatureDataType::Boolean,
        _ => FeatureDataType::String,
    }
}

/// List all registered ML models
/// GET /api/v1/orchestration/models
pub async fn list_models_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<Vec<ModelSummaryResponseDto>>, (StatusCode, String)> {
    // Graceful degradation: return empty list if model registry not enabled
    if let Some(ref registry) = state.model_registry {
        let models = registry.list_models().await;

        let dto_models = models
            .into_iter()
            .map(|m| ModelSummaryResponseDto {
                id: m.id,
                name: m.name,
                version: m.version,
                protocol: format!("{:?}", m.protocol),
            })
            .collect();

        return Ok(Json(dto_models));
    }

    // Model registry not enabled - return empty list
    tracing::debug!("Model registry not enabled, returning empty model list");
    Ok(Json(vec![]))
}

/// Get specific ML model metadata by ID
/// GET /api/v1/orchestration/models/:model_id
pub async fn get_model_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let registry = state.model_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model registry not available".to_string(),
        )
    })?;

    let model = registry.get_model(&model_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Model not found: {}", model_id),
        )
    })?;

    Ok(Json(serde_json::json!({
        "id": model.id,
        "name": model.name,
        "version": model.version,
        "framework": format!("{:?}", model.framework),
        "endpoint": {
            "protocol": format!("{:?}", model.endpoint.protocol),
            "url": model.endpoint.url,
            "timeout_ms": model.endpoint.timeout_ms,
        },
        "input_schema": model.input_schema,
        "output_schema": model.output_schema,
        "created_at": model.created_at,
        "updated_at": model.updated_at,
    })))
}

/// Update an existing ML model
/// PUT /api/v1/orchestration/models/:model_id
pub async fn update_model_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
    Json(request): Json<RegisterOrchestrationModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let registry = state.model_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model registry not available".to_string(),
        )
    })?;

    // Verify model exists
    let existing = registry.get_model(&model_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Model {} not found", model_id),
        )
    })?;

    // Convert DTO to domain types
    let protocol = match request.endpoint.protocol.as_str() {
        "http" => ModelProtocol::Http,
        "grpc" => ModelProtocol::Grpc,
        "lambda" => ModelProtocol::Lambda,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid protocol".to_string())),
    };

    let framework = match request.framework.as_str() {
        "tensorflow" => ServingFramework::TensorFlowServing,
        "pytorch" => ServingFramework::TorchServe,
        "sagemaker" => ServingFramework::SageMaker,
        "custom" => ServingFramework::Custom,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid framework".to_string())),
    };

    let endpoint = ModelEndpoint {
        protocol,
        url: request.endpoint.url.clone(),
        timeout_ms: request.endpoint.timeout_ms,
        headers: request.endpoint.headers.clone(),
    };

    use graphica_core::orchestration::ml::registry::FeatureDataType;

    let input_schema: Vec<FeatureSchema> = request
        .input_schema
        .into_iter()
        .map(|dto| {
            let data_type = match dto.data_type.as_str() {
                "string" => FeatureDataType::String,
                "integer" => FeatureDataType::Integer,
                "float" => FeatureDataType::Float,
                "boolean" => FeatureDataType::Boolean,
                "array" => FeatureDataType::Array,
                "object" => FeatureDataType::Object,
                _ => FeatureDataType::String, // default
            };
            FeatureSchema {
                name: dto.name,
                data_type,
                required: dto.required,
            }
        })
        .collect();

    // Create updated metadata (re-register to update all fields)
    let updated_metadata = ModelMetadata {
        id: model_id.clone(),
        name: request.name,
        version: request.version,
        endpoint,
        framework,
        input_schema,
        output_schema: request.output_schema,
        created_at: existing.created_at, // Preserve original creation time
        updated_at: Utc::now(),          // Update timestamp
    };

    // Re-register with updated metadata
    registry.register(updated_metadata).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Update failed: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "status": "updated",
        "model_id": model_id,
        "message": "Model successfully updated"
    })))
}

/// Delete an ML model from the registry
/// DELETE /api/v1/orchestration/models/:model_id
pub async fn delete_model_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let registry = state.model_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model registry not available".to_string(),
        )
    })?;

    registry.unregister(&model_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Delete failed: {}", e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
