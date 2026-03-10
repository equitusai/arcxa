//! Rule Management Handler Functions
//!
//! HTTP handlers for WASM-based rule execution in the orchestration system.
//! These handlers manage loading, unloading, executing, and caching rules.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

/// Load a WASM rule into the executor
/// POST /api/v1/orchestration/rules/:rule_id
pub async fn load_rule_handler(
    State(state): State<Arc<ApiState>>,
    Path(rule_id): Path<String>,
    Json(request): Json<LoadRuleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let executor = state.rule_executor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Rule executor not available".to_string(),
        )
    })?;

    let wasm_bytes = base64::decode(&request.wasm_bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid base64: {}", e)))?;

    executor.load_rule(&rule_id, &wasm_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Rule load failed: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "rule_id": rule_id,
        "status": "loaded"
    })))
}

/// Unload a WASM rule from the executor
/// DELETE /api/v1/orchestration/rules/:rule_id
pub async fn unload_rule_handler(
    State(state): State<Arc<ApiState>>,
    Path(rule_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let executor = state.rule_executor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Rule executor not available".to_string(),
        )
    })?;

    executor.unload_rule(&rule_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unload failed: {}", e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Execute a loaded WASM rule with input data
/// POST /api/v1/orchestration/rules/:rule_id/execute
pub async fn execute_rule_handler(
    State(state): State<Arc<ApiState>>,
    Path(rule_id): Path<String>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let executor = state.rule_executor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Rule executor not available".to_string(),
        )
    })?;

    // RuleExecutor.execute_heuristic() expects serde_json::Value, not string
    let result = executor
        .execute_heuristic(&rule_id, &input)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Rule execution failed: {}", e),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": result.success,
        "output": result.output,
        "confidence": result.confidence,
    })))
}

/// Clear the rule cache
/// POST /api/v1/orchestration/rules/cache/clear
pub async fn clear_rule_cache_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let executor = state.rule_executor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Rule executor not available".to_string(),
        )
    })?;

    executor.clear_cache();

    Ok(Json(serde_json::json!({
        "status": "cache cleared"
    })))
}
