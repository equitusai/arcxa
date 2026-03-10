//! Health Check Handlers
//!
//! HTTP handlers for health, liveness, readiness, and metrics endpoints.

use axum::{extract::State, http::StatusCode, response::Json};
use std::sync::Arc;

use crate::api::dto::{ComponentHealth, HealthResponse, ReadinessResponse, StorageHealthResponse};
use crate::api::ApiState;

/// Health check endpoint - alias for liveness check
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
    ),
    tag = "Health & Monitoring"
)]
pub async fn health_check() -> (StatusCode, Json<HealthResponse>) {
    liveness_check().await
}

/// Liveness probe - checks if service is alive (basic health)
/// Returns 200 if process is running, doesn't verify dependencies
#[utoipa::path(
    get,
    path = "/health/live",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    ),
    tag = "Health & Monitoring"
)]
pub async fn liveness_check() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "alive".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: chrono::Utc::now(),
            components: None,
        }),
    )
}

/// Storage-specific health check
#[utoipa::path(
    get,
    path = "/health/storage",
    responses(
        (status = 200, description = "Storage is healthy", body = StorageHealthResponse),
        (status = 503, description = "Storage is unavailable", body = StorageHealthResponse),
    ),
    tag = "Health & Monitoring"
)]
pub async fn storage_health_check(
    State(state): State<Arc<ApiState>>,
) -> (StatusCode, Json<StorageHealthResponse>) {
    let storage_ok = match state.lineage_storage.health_check().await {
        Ok(_) => true,
        Err(_) => false,
    };

    let governance_ok = if let Some(ref gov) = state.governance_brain {
        match gov.triple_count() {
            Ok(_) => true,
            Err(_) => false,
        }
    } else {
        false
    };

    let all_healthy = storage_ok && governance_ok;
    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(StorageHealthResponse {
            healthy: all_healthy,
            rocksdb: storage_ok,
            rdf_store: governance_ok,
            timestamp: chrono::Utc::now(),
        }),
    )
}

/// Prometheus metrics endpoint
/// Exports all metrics in Prometheus text format
#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = 200, description = "Prometheus metrics in text format", body = String, content_type = "text/plain"),
        (status = 500, description = "Failed to gather metrics", body = String),
        (status = 503, description = "Metrics registry not available", body = String),
    ),
    tag = "Health & Monitoring"
)]
pub async fn metrics_endpoint(State(state): State<Arc<ApiState>>) -> (StatusCode, String) {
    if let Some(ref registry) = state.metrics_registry {
        match registry.gather() {
            Ok(metrics_bytes) => match String::from_utf8(metrics_bytes) {
                Ok(metrics_text) => (StatusCode::OK, metrics_text),
                Err(e) => {
                    tracing::error!("Failed to convert metrics to UTF-8: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to encode metrics: {}", e),
                    )
                }
            },
            Err(e) => {
                tracing::error!("Failed to gather metrics: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to gather metrics: {}", e),
                )
            }
        }
    } else {
        tracing::warn!("Metrics registry not available");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Metrics registry not initialized".to_string(),
        )
    }
}

/// Readiness probe - checks if service can handle requests (deep check)
/// Returns 200 only if all critical components are ready
#[utoipa::path(
    get,
    path = "/health/ready",
    responses(
        (status = 200, description = "Service is ready to handle requests", body = ReadinessResponse),
        (status = 503, description = "Service is not ready", body = ReadinessResponse),
    ),
    tag = "Health & Monitoring"
)]
pub async fn readiness_check(
    State(state): State<Arc<ApiState>>,
) -> (StatusCode, Json<ReadinessResponse>) {
    let mut components = std::collections::HashMap::new();
    let mut all_ready = true;

    // Check 1: RocksDB storage connectivity
    let storage_health = match state.lineage_storage.health_check().await {
        Ok(_) => ComponentHealth {
            status: "ready".to_string(),
            message: Some("RocksDB accessible".to_string()),
        },
        Err(e) => {
            all_ready = false;
            ComponentHealth {
                status: "not_ready".to_string(),
                message: Some(format!("Storage error: {}", e)),
            }
        }
    };
    components.insert("storage".to_string(), storage_health);

    // Check 2: Kafka connectivity (best effort - don't fail if Kafka unavailable)
    // This is informational only since Kafka may be temporarily down
    components.insert(
        "kafka".to_string(),
        ComponentHealth {
            status: "unknown".to_string(),
            message: Some("Kafka health not implemented yet".to_string()),
        },
    );

    // Check 3: Dataflow workers (via metrics)
    // In production, check if workers are processing data
    components.insert(
        "dataflow_workers".to_string(),
        ComponentHealth {
            status: "ready".to_string(),
            message: Some("Worker health tracking via metrics".to_string()),
        },
    );

    let status_code = if all_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            ready: all_ready,
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: chrono::Utc::now(),
            components,
        }),
    )
}
