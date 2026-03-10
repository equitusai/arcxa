//! WAL Handler Functions
//!
//! HTTP handlers for Write-Ahead Log (WAL) monitoring and management.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Get WAL health status
pub async fn get_wal_status(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<WalStatusResponse>, ApiError> {
    tracing::info!("Retrieving WAL status");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let _indexes = rdf_store
        .temporal_indexes()
        .ok_or_else(|| ApiError::internal("Temporal indexes not enabled".to_string()))?;

    // Get WAL from indexes - need to access via RDF store
    let wal_stats = rdf_store
        .wal_statistics()
        .map_err(|e| ApiError::internal(format!("Failed to get WAL statistics: {}", e)))?;

    let uncommitted = wal_stats["uncommitted_entries"].as_u64().unwrap_or(0);
    let total = wal_stats["total_entries"].as_u64().unwrap_or(0);

    let healthy = uncommitted < 1000; // Threshold for health
    let message = if healthy {
        format!("WAL healthy - {} uncommitted operations", uncommitted)
    } else {
        format!(
            "WARNING: {} uncommitted operations (threshold: 1000)",
            uncommitted
        )
    };

    Ok(Json(WalStatusResponse {
        healthy,
        total_entries: total as usize,
        uncommitted_entries: uncommitted as usize,
        wal_enabled: true,
        timestamp: Utc::now(),
        message,
    }))
}

/// List uncommitted WAL operations
pub async fn get_wal_operations(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<WalOperationsResponse>, ApiError> {
    tracing::info!("Retrieving WAL operations");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let uncommitted_ops = rdf_store
        .get_uncommitted_wal_operations()
        .map_err(|e| ApiError::internal(format!("Failed to get uncommitted operations: {}", e)))?;

    let operations: Vec<WalOperation> = uncommitted_ops
        .iter()
        .filter_map(|entry| {
            let ts = entry.get("timestamp")?.as_u64()?;
            Some(WalOperation {
                op_id: entry.get("op_id")?.as_str()?.to_string(),
                timestamp: DateTime::from_timestamp(ts as i64, 0)?.with_timezone(&Utc),
                operation_type: entry.get("operation_type")?.as_str()?.to_string(),
                committed: entry.get("committed")?.as_bool()?,
            })
        })
        .collect();

    let uncommitted_count = operations.iter().filter(|op| !op.committed).count();

    Ok(Json(WalOperationsResponse {
        total_count: operations.len(),
        uncommitted_count,
        operations,
    }))
}

/// Manually trigger WAL replay
pub async fn trigger_wal_replay(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<WalReplayResponse>, ApiError> {
    tracing::info!("Manually triggering WAL replay");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let replayed_count = rdf_store
        .replay_wal()
        .map_err(|e| ApiError::internal(format!("WAL replay failed: {}", e)))?;

    let message = if replayed_count == 0 {
        "No uncommitted operations to replay".to_string()
    } else {
        format!("Successfully replayed {} operations", replayed_count)
    };

    Ok(Json(WalReplayResponse {
        success: true,
        replayed_count,
        message,
        timestamp: Utc::now(),
    }))
}
