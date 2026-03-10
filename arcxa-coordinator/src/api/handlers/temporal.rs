//! Temporal Handler Functions
//!
//! HTTP handlers for temporal index management, checkpointing, and version chain analysis.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AnalyzeQuery {
    pub threshold: Option<usize>,
}

#[derive(Serialize)]
pub struct CompactionResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct CacheClearResponse {
    pub success: bool,
    pub message: String,
}

/// Create a RocksDB checkpoint for backup
pub async fn create_temporal_checkpoint(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CheckpointRequest>,
) -> Result<Json<CheckpointResponse>, ApiError> {
    tracing::info!("Creating temporal index checkpoint at: {}", req.path);

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let indexes = rdf_store
        .temporal_indexes()
        .ok_or_else(|| ApiError::internal("Temporal indexes not enabled".to_string()))?;

    let checkpoint_path = indexes
        .create_checkpoint(&req.path)
        .map_err(|e| ApiError::internal(format!("Failed to create checkpoint: {}", e)))?;

    Ok(Json(CheckpointResponse {
        success: true,
        checkpoint_path,
        timestamp: Utc::now(),
    }))
}

/// Analyze version chains and return statistics
pub async fn analyze_temporal_chains(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<AnalyzeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let threshold = params.threshold.unwrap_or(1000);
    tracing::info!(
        "Analyzing temporal version chains (threshold: {})",
        threshold
    );

    // If temporal indexes are enabled, perform analysis
    if let Some(ref rdf_store) = state.rdf_store {
        if let Some(indexes) = rdf_store.temporal_indexes() {
            let analysis = indexes.analyze_version_chains(threshold).map_err(|e| {
                ApiError::internal(format!("Failed to analyze version chains: {}", e))
            })?;

            return Ok(Json(serde_json::to_value(analysis).unwrap_or_else(|err| {
                tracing::warn!("Failed to serialize lineage analysis: {}", err);
                serde_json::json!({"error": "Serialization failed"})
            })));
        }
    }

    // Temporal indexes not enabled - return empty analysis
    tracing::debug!("Temporal indexes not enabled, returning empty analysis");
    Ok(Json(serde_json::json!({
        "enabled": false,
        "total_entities": 0,
        "long_chains": [],
        "avg_chain_length": 0.0,
        "max_chain_length": 0
    })))
}

/// Trigger manual RocksDB compaction
pub async fn compact_temporal_indexes(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<CompactionResponse>, ApiError> {
    tracing::info!("Triggering manual compaction of temporal indexes");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let indexes = rdf_store
        .temporal_indexes()
        .ok_or_else(|| ApiError::internal("Temporal indexes not enabled".to_string()))?;

    indexes
        .compact_database()
        .map_err(|e| ApiError::internal(format!("Failed to compact database: {}", e)))?;

    Ok(Json(CompactionResponse {
        success: true,
        message: "Compaction completed successfully".to_string(),
        timestamp: Utc::now(),
    }))
}

/// Get detailed statistics about temporal indexes
pub async fn get_temporal_statistics(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("Retrieving temporal index statistics");

    // Return default stats if temporal indexes are not enabled (graceful degradation)
    if let Some(ref rdf_store) = state.rdf_store {
        if let Some(indexes) = rdf_store.temporal_indexes() {
            let stats = indexes
                .get_statistics()
                .map_err(|e| ApiError::internal(format!("Failed to get statistics: {}", e)))?;

            return Ok(Json(serde_json::to_value(stats).unwrap_or_else(|err| {
                tracing::warn!("Failed to serialize statistics: {}", err);
                serde_json::json!({"error": "Serialization failed"})
            })));
        }
    }

    // Temporal indexes not enabled - return default empty stats
    tracing::debug!("Temporal indexes not enabled, returning default stats");
    Ok(Json(serde_json::json!({
        "enabled": false,
        "total_versions": 0,
        "total_entities": 0,
        "total_transactions": 0,
        "cache_hits": 0,
        "cache_misses": 0,
        "cache_size": 0,
        "index_size_bytes": 0
    })))
}

/// Clear the LRU cache
pub async fn clear_temporal_cache(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<CacheClearResponse>, ApiError> {
    tracing::info!("Clearing temporal index cache");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let indexes = rdf_store
        .temporal_indexes()
        .ok_or_else(|| ApiError::internal("Temporal indexes not enabled".to_string()))?;

    indexes.clear_cache();

    Ok(Json(CacheClearResponse {
        success: true,
        message: "Cache cleared successfully".to_string(),
    }))
}

/// Get aggregated temporal system summary for dashboard
pub async fn get_temporal_summary(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<TemporalSummaryResponse>, ApiError> {
    tracing::info!("Retrieving temporal system summary");

    // Get RDF stats (always available via governance brain)
    let total_triples = if let Some(ref brain) = state.governance_brain {
        brain.triple_count().unwrap_or(0)
    } else {
        0
    };

    // If temporal indexes are enabled, get detailed stats
    if let Some(ref rdf_store) = state.rdf_store {
        if let Some(indexes) = rdf_store.temporal_indexes() {
            // Get WAL stats
            let wal_stats = rdf_store
                .wal_statistics()
                .map_err(|e| ApiError::internal(format!("Failed to get WAL statistics: {}", e)))?;

            // Get temporal index stats
            let index_stats = indexes.get_statistics().map_err(|e| {
                ApiError::internal(format!("Failed to get index statistics: {}", e))
            })?;

            // Calculate cache utilization (cache_size / cache_capacity)
            let cache_hit_rate = if index_stats.cache_capacity > 0 {
                (index_stats.cache_size as f64) / (index_stats.cache_capacity as f64)
            } else {
                0.0
            };

            let uncommitted = wal_stats["uncommitted_entries"].as_u64().unwrap_or(0);
            let wal_healthy = uncommitted < 1000;
            let overall_healthy = wal_healthy;

            return Ok(Json(TemporalSummaryResponse {
                wal_healthy,
                wal_uncommitted_ops: uncommitted as usize,
                total_versions: index_stats.total_versions,
                cache_hit_rate,
                total_triples,
                overall_healthy,
                timestamp: Utc::now(),
            }));
        }
    }

    // Temporal indexes not enabled - return default summary
    tracing::debug!("Temporal indexes not enabled, returning default summary");
    Ok(Json(TemporalSummaryResponse {
        wal_healthy: true,
        wal_uncommitted_ops: 0,
        total_versions: 0,
        cache_hit_rate: 0.0,
        total_triples,
        overall_healthy: true,
        timestamp: Utc::now(),
    }))
}
