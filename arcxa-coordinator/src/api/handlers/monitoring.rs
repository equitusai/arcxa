//! Monitoring Handler Functions
//!
//! HTTP handlers for cache and circuit breaker monitoring in the orchestration system.

use crate::api::ApiState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

/// Get cache statistics
/// GET /api/v1/orchestration/cache/stats
pub async fn get_cache_stats_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Graceful degradation: return default stats if cache not enabled
    if let Some(ref cache) = state.model_cache {
        let stats = cache.stats().await;

        return Ok(Json(serde_json::json!({
            "enabled": true,
            "size": stats.size,
            "capacity": stats.capacity,
            "utilization": if stats.capacity > 0 {
                (stats.size as f64 / stats.capacity as f64) * 100.0
            } else {
                0.0
            },
        })));
    }

    // Model cache not enabled - return default empty stats
    tracing::debug!("Model cache not enabled, returning default stats");
    Ok(Json(serde_json::json!({
        "enabled": false,
        "size": 0,
        "capacity": 0,
        "utilization": 0.0,
    })))
}

/// Clear the model cache
/// POST /api/v1/orchestration/cache/clear
pub async fn clear_model_cache_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cache = state.model_cache.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model cache not available".to_string(),
        )
    })?;

    cache.clear().await;

    Ok(Json(serde_json::json!({
        "status": "cache cleared"
    })))
}

/// Get circuit breaker status for a model
/// GET /api/v1/orchestration/circuit-breaker/:model_id
pub async fn get_circuit_breaker_status_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use graphica_core::reliability::{CircuitBreaker, CircuitBreakerConfig};

    let breakers = state.circuit_breakers.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Circuit breaker tracking not available".to_string(),
        )
    })?;

    // Get or create circuit breaker for this model
    let breaker = breakers
        .entry(model_id.clone())
        .or_insert_with(|| {
            Arc::new(CircuitBreaker::new(
                model_id.clone(),
                CircuitBreakerConfig::default(),
            ))
        })
        .clone();

    // Determine state using available methods
    let (state_str, description) = if breaker.is_open() {
        ("open", "Circuit is open, requests are blocked")
    } else if breaker.is_half_open() {
        (
            "half_open",
            "Circuit is half-open, testing if service recovered",
        )
    } else {
        ("closed", "Circuit is closed, requests are allowed")
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "state": state_str,
        "description": description,
        "consecutive_failures": breaker.consecutive_failures(),
    })))
}

/// Reset circuit breaker for a model
/// POST /api/v1/orchestration/circuit-breaker/:model_id/reset
pub async fn reset_circuit_breaker_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use graphica_core::reliability::{CircuitBreaker, CircuitBreakerConfig};

    let breakers = state.circuit_breakers.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Circuit breaker tracking not available".to_string(),
        )
    })?;

    // Replace with new circuit breaker (effectively resetting it)
    let new_breaker = Arc::new(CircuitBreaker::new(
        model_id.clone(),
        CircuitBreakerConfig::default(),
    ));
    breakers.insert(model_id.clone(), new_breaker);

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "status": "reset",
        "message": "Circuit breaker successfully reset to closed state"
    })))
}
