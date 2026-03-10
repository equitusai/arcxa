//! # Secret Management API
//!
//! RESTful API endpoints for managing secrets across multiple backend stores.
//! All endpoints require admin authentication.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::dto::ApiError;
use crate::api::ApiState;
use graphica_core::secrets::{SecretMetadata, SecretStoreRef, SecretValue};

/// Create secret management router
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/secrets", get(list_secrets))
        .route("/secrets/:path", get(get_secret))
        .route("/secrets/:path", put(put_secret))
        .route("/secrets/:path", delete(delete_secret))
        .route("/secrets/:path/rotate", post(rotate_secret))
        .route("/secrets/:path/metadata", get(get_secret_metadata))
        .route("/secrets/stores", get(list_stores))
        .route("/secrets/stores/:name/health", get(check_store_health))
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to store a secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutSecretRequest {
    /// Secret value (JSON can represent String, KeyValue, or arbitrary JSON)
    pub value: serde_json::Value,
    /// Optional description
    pub description: Option<String>,
    /// Optional tags for categorization
    pub tags: Option<Vec<String>>,
    /// Target store name (defaults to "default")
    pub store: Option<String>,
}

/// Response after storing a secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutSecretResponse {
    pub path: String,
    pub version: String,
    pub store: String,
    pub created_at: String,
}

/// Response for secret retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSecretResponse {
    pub path: String,
    pub value: serde_json::Value,
    pub version: String,
    pub metadata: SecretMetadataDto,
    pub created_at: String,
    pub updated_at: String,
}

/// Secret metadata DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadataDto {
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl From<SecretMetadata> for SecretMetadataDto {
    fn from(metadata: SecretMetadata) -> Self {
        Self {
            description: metadata.description,
            tags: metadata.tags,
        }
    }
}

/// Query parameters for listing secrets
#[derive(Debug, Clone, Deserialize)]
pub struct ListSecretsQuery {
    /// Optional path prefix filter
    pub prefix: Option<String>,
    /// Target store name (defaults to "default")
    pub store: Option<String>,
}

/// Response for listing secrets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSecretsResponse {
    pub secrets: Vec<String>,
    pub count: usize,
    pub store: String,
}

/// Request to rotate a secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateSecretRequest {
    /// New secret value
    pub new_value: serde_json::Value,
    /// Target store name (defaults to "default")
    pub store: Option<String>,
}

/// Response after rotating a secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateSecretResponse {
    pub path: String,
    pub old_version: String,
    pub new_version: String,
    pub store: String,
    pub rotated_at: String,
}

/// Response for metadata retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMetadataResponse {
    pub path: String,
    pub metadata: SecretMetadataDto,
    pub version: String,
    pub store: String,
}

/// Response for listing stores
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListStoresResponse {
    pub stores: Vec<String>,
    pub default_store: Option<String>,
    pub cache_stats: Option<CacheStatsDto>,
}

/// Cache statistics DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatsDto {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
    pub max_entries: usize,
    pub ttl_seconds: u64,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub store: String,
    pub healthy: bool,
    pub message: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// List all secrets (optionally filtered by prefix)
///
/// GET /api/v1/secrets?prefix=datasource&store=default
async fn list_secrets(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListSecretsQuery>,
) -> Result<Json<ListSecretsResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = query.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    let secrets = store
        .list_secrets(query.prefix.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list secrets: {}", e)))?;

    let count = secrets.len();

    Ok(Json(ListSecretsResponse {
        secrets,
        count,
        store: store_name.to_string(),
    }))
}

/// Get a secret by path
///
/// GET /api/v1/secrets/datasource/postgres/credentials
async fn get_secret(
    State(state): State<Arc<ApiState>>,
    Path(path): Path<String>,
    Query(query): Query<ListSecretsQuery>,
) -> Result<Json<GetSecretResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = query.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    let secret = store
        .get_secret(&path, None)
        .await
        .map_err(|e| ApiError::not_found(format!("Secret not found: {}", e)))?;

    // Convert SecretValue to JSON
    let value_json = match secret.value {
        SecretValue::String(s) => serde_json::Value::String(s),
        SecretValue::Json(j) => j,
        SecretValue::KeyValue(map) => serde_json::to_value(map)
            .map_err(|e| ApiError::internal(format!("Failed to serialize KeyValue: {}", e)))?,
        SecretValue::Binary(bytes) => {
            // Base64 encode binary data
            serde_json::Value::String(base64::encode(&bytes))
        }
    };

    Ok(Json(GetSecretResponse {
        path: secret.path,
        value: value_json,
        version: secret.version,
        metadata: secret.metadata.into(),
        created_at: secret.created_at.to_rfc3339(),
        updated_at: secret.updated_at.to_rfc3339(),
    }))
}

/// Store a secret
///
/// PUT /api/v1/secrets/datasource/postgres/credentials
async fn put_secret(
    State(state): State<Arc<ApiState>>,
    Path(path): Path<String>,
    Json(request): Json<PutSecretRequest>,
) -> Result<Json<PutSecretResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = request.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    // Convert JSON value to SecretValue
    let secret_value = json_to_secret_value(request.value)?;

    // Build metadata
    let metadata = SecretMetadata {
        description: request.description,
        tags: request.tags.unwrap_or_default(),
        ..Default::default()
    };

    // Store the secret
    let version = store
        .put_secret(&path, secret_value, Some(metadata))
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store secret: {}", e)))?;

    Ok(Json(PutSecretResponse {
        path,
        version,
        store: store_name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Delete a secret
///
/// DELETE /api/v1/secrets/datasource/postgres/credentials
async fn delete_secret(
    State(state): State<Arc<ApiState>>,
    Path(path): Path<String>,
    Query(query): Query<ListSecretsQuery>,
) -> Result<StatusCode, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = query.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    store
        .delete_secret(&path, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete secret: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Rotate a secret (creates new version)
///
/// POST /api/v1/secrets/datasource/postgres/credentials/rotate
async fn rotate_secret(
    State(state): State<Arc<ApiState>>,
    Path(path): Path<String>,
    Json(request): Json<RotateSecretRequest>,
) -> Result<Json<RotateSecretResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = request.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    // Get current version
    let old_secret = store
        .get_secret(&path, None)
        .await
        .map_err(|e| ApiError::not_found(format!("Secret not found: {}", e)))?;
    let old_version = old_secret.version;

    // Convert JSON value to SecretValue
    let new_secret_value = json_to_secret_value(request.new_value)?;

    // Rotate the secret
    let new_version = store
        .rotate_secret(&path, new_secret_value)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to rotate secret: {}", e)))?;

    Ok(Json(RotateSecretResponse {
        path,
        old_version,
        new_version,
        store: store_name.to_string(),
        rotated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get secret metadata (without revealing the secret value)
///
/// GET /api/v1/secrets/datasource/postgres/credentials/metadata
async fn get_secret_metadata(
    State(state): State<Arc<ApiState>>,
    Path(path): Path<String>,
    Query(query): Query<ListSecretsQuery>,
) -> Result<Json<GetMetadataResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store_name = query.store.as_deref().unwrap_or("default");
    let store = registry
        .get(store_name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", store_name)))?;

    let metadata = store
        .get_metadata(&path)
        .await
        .map_err(|e| ApiError::not_found(format!("Secret metadata not found: {}", e)))?;

    // Get version from the secret itself
    let secret = store
        .get_secret(&path, None)
        .await
        .map_err(|e| ApiError::not_found(format!("Secret not found: {}", e)))?;

    Ok(Json(GetMetadataResponse {
        path,
        metadata: metadata.into(),
        version: secret.version,
        store: store_name.to_string(),
    }))
}

/// List all registered secret stores
///
/// GET /api/v1/secrets/stores
async fn list_stores(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ListStoresResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let stores = registry.list_stores();
    let default_store = registry.default().map(|store| store.name().to_string());

    let cache_stats = registry.cache_stats().map(|stats| CacheStatsDto {
        total_entries: stats.total_entries,
        active_entries: stats.active_entries,
        expired_entries: stats.expired_entries,
        max_entries: stats.max_entries,
        ttl_seconds: stats.ttl_seconds,
    });

    Ok(Json(ListStoresResponse {
        stores,
        default_store,
        cache_stats,
    }))
}

/// Health check for a specific store
///
/// GET /api/v1/secrets/stores/default/health
async fn check_store_health(
    State(state): State<Arc<ApiState>>,
    Path(name): Path<String>,
) -> Result<Json<HealthCheckResponse>, ApiError> {
    let registry = state
        .secret_store_registry
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Secret store not available".to_string()))?;

    let store = registry
        .get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Secret store '{}' not found", name)))?;

    let healthy = store.health_check().await.unwrap_or(false);

    Ok(Json(HealthCheckResponse {
        store: name,
        healthy,
        message: if healthy {
            Some("Store is healthy".to_string())
        } else {
            Some("Store health check failed".to_string())
        },
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert JSON value to SecretValue
fn json_to_secret_value(value: serde_json::Value) -> Result<SecretValue, ApiError> {
    match value {
        serde_json::Value::String(s) => Ok(SecretValue::String(s)),
        serde_json::Value::Object(map) => {
            // Check if this is a credentials object (has username/password)
            if map.contains_key("username") && map.contains_key("password") {
                let username = map
                    .get("username")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ApiError::bad_request("username must be a string".to_string()))?
                    .to_string();
                let password = map
                    .get("password")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ApiError::bad_request("password must be a string".to_string()))?
                    .to_string();
                Ok(SecretValue::from_credentials(username, password))
            } else {
                // Otherwise treat as generic JSON
                Ok(SecretValue::Json(serde_json::Value::Object(map)))
            }
        }
        other => Ok(SecretValue::Json(other)),
    }
}

// Note: base64 crate is used for Binary encoding
// Add to Cargo.toml: base64 = "0.21"
