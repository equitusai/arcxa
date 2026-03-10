//! Cluster Admin API Endpoints
//!
//! Complete API control over horizontal scaling operations:
//! - Cluster topology and statistics
//! - Scaling operations (scale-out, scale-in)
//! - Shard management and health monitoring
//! - Replication configuration
//! - Data migration (future)
//!
//! All endpoints require Admin role authentication.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::ApiState;
use crate::api::dto::ApiError;
use crate::governance::distributed::{ReplicationConfig, ShardId, ShardMetadata};

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ShardResponse {
    pub shard_id: u32,
    pub leader_address: String,
    pub replica_addresses: Vec<String>,
    pub status: String,
    pub hash_range: HashRangeResponse,
    pub triple_count: u64,
    pub size_bytes: u64,
    pub last_heartbeat: String,
    pub raft_term: u64,
}

#[derive(Debug, Serialize)]
pub struct HashRangeResponse {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Serialize)]
pub struct TopologyResponse {
    pub total_shards: u32,
    pub replication_factor: u32,
    pub cluster_version: u64,
    pub total_triples: u64,
    pub total_size_bytes: u64,
    pub updated_at: String,
    pub shards: Vec<ShardResponse>,
}

#[derive(Debug, Serialize)]
pub struct ClusterStatsResponse {
    pub total_shards: u32,
    pub healthy_shards: u32,
    pub degraded_shards: u32,
    pub down_shards: u32,
    pub total_triples: u64,
    pub total_size_gb: f64,
    pub queries_per_second: f64,
    pub writes_per_second: f64,
    pub p99_query_latency_ms: f64,
    pub p99_write_latency_ms: f64,
    pub average_shard_utilization: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ClusterHealthResponse {
    pub status: String, // "healthy", "degraded", "critical"
    pub total_shards: u32,
    pub healthy_shards: u32,
    pub degraded_shards: u32,
    pub down_shards: u32,
    pub issues: Vec<HealthIssue>,
    pub last_check: DateTime<Utc>,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct HealthIssue {
    pub severity: String, // "warning", "error", "critical"
    pub component: String,
    pub message: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ClusterConfigResponse {
    pub cluster_name: String,
    pub mode: String, // "single-node", "distributed"
    pub auto_scaling: AutoScalingConfig,
    pub data_retention: DataRetentionConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Serialize)]
pub struct AutoScalingConfig {
    pub enabled: bool,
    pub min_shards: u32,
    pub max_shards: u32,
    pub target_utilization: f64,
    pub scale_out_threshold: f64,
    pub scale_in_threshold: f64,
    pub cooldown_minutes: u32,
}

#[derive(Debug, Serialize)]
pub struct DataRetentionConfig {
    pub auto_save_interval_seconds: u64,
    pub backup_enabled: bool,
    pub backup_interval_hours: u32,
    pub retention_days: u32,
}

#[derive(Debug, Serialize)]
pub struct PerformanceConfig {
    pub query_timeout_seconds: u32,
    pub write_timeout_seconds: u32,
    pub max_query_result_size: u64,
    pub connection_pool_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct ScaleOutRequest {
    pub new_shard_count: u32,
    pub replication_factor: u32,
    pub node_addresses: Vec<String>,
    pub rebalance_strategy: String, // "gradual" or "immediate"
    pub rebalance_throttle_mbps: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ScaleOutResponse {
    pub operation_id: String,
    pub status: String,
    pub old_shard_count: u32,
    pub new_shard_count: u32,
    pub message: String,
    pub estimated_duration_minutes: Option<u32>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ShardDetailResponse {
    pub shard_id: u32,
    pub status: String,
    pub leader: ShardNodeInfo,
    pub replicas: Vec<ShardReplicaInfo>,
    pub hash_range: HashRangeResponse,
    pub statistics: ShardStatistics,
}

#[derive(Debug, Serialize)]
pub struct ShardNodeInfo {
    pub address: String,
    pub raft_term: u64,
    pub is_healthy: bool,
    pub last_heartbeat: String,
}

#[derive(Debug, Serialize)]
pub struct ShardReplicaInfo {
    pub address: String,
    pub role: String,
    pub lag_bytes: u64,
    pub is_healthy: bool,
}

#[derive(Debug, Serialize)]
pub struct ShardStatistics {
    pub triple_count: u64,
    pub size_bytes: u64,
    pub queries_per_second: f64,
    pub writes_per_second: f64,
    pub p99_query_latency_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct ReplicationConfigResponse {
    pub replication_factor: u32,
    pub sync_replication: bool,
    pub async_replication_lag_ms: u64,
    pub raft_election_timeout_ms: u64,
    pub raft_heartbeat_interval_ms: u64,
    pub enable_auto_failover: bool,
    pub failover_timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct ClusterMetadataResponse {
    pub cluster_id: String,
    pub created_at: String,
    pub cluster_version: u64,
    pub graphica_version: String,
    pub mode: String,
    pub total_operations: u64,
    pub last_topology_change: Option<String>,
}

// ============================================================================
// Handler Functions
// ============================================================================

/// GET /api/v1/cluster/topology
/// Get current cluster topology and shard distribution
pub async fn get_cluster_topology(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<TopologyResponse>, ApiError> {
    tracing::info!("📊 Fetching cluster topology");

    // Check if we have a shard registry (distributed mode)
    if let Some(ref registry) = state.shard_registry {
        let topology = registry
            .get_topology()
            .map_err(|e| ApiError::internal(format!("Failed to get topology: {}", e)))?;

        let shards: Vec<ShardResponse> = topology
            .shards
            .values()
            .map(|s| shard_metadata_to_response(s))
            .collect();

        Ok(Json(TopologyResponse {
            total_shards: topology.total_shards,
            replication_factor: topology.replication_factor,
            cluster_version: topology.cluster_version,
            total_triples: topology.total_triples,
            total_size_bytes: topology.total_size_bytes,
            updated_at: format_timestamp(topology.updated_at),
            shards,
        }))
    } else {
        // Single-node mode - create synthetic topology
        Ok(Json(create_single_node_topology(&state)))
    }
}

/// GET /api/v1/cluster/stats
/// Get cluster-wide statistics
pub async fn get_cluster_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ClusterStatsResponse>, ApiError> {
    tracing::info!("📈 Fetching cluster statistics");

    // Get RDF store stats
    let (total_triples, healthy_shards, total_shards) = if let Some(ref rdf_store) = state.rdf_store
    {
        let count = rdf_store.triple_count().unwrap_or_else(|_| 0);
        (count, 1, 1) // Single-node mode
    } else {
        (0, 0, 0)
    };

    let total_size_bytes = total_triples * 500; // Estimate ~500 bytes per triple
    let total_size_gb = total_size_bytes as f64 / 1_073_741_824.0;

    Ok(Json(ClusterStatsResponse {
        total_shards,
        healthy_shards,
        degraded_shards: 0,
        down_shards: 0,
        total_triples: total_triples as u64,
        total_size_gb,
        queries_per_second: 0.0,   // TODO: Add metrics
        writes_per_second: 0.0,    // TODO: Add metrics
        p99_query_latency_ms: 0.0, // TODO: Add metrics
        p99_write_latency_ms: 0.0, // TODO: Add metrics
        average_shard_utilization: 0.5,
        timestamp: Utc::now(),
    }))
}

/// GET /api/v1/cluster/health
/// Get overall cluster health
pub async fn get_cluster_health(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ClusterHealthResponse>, ApiError> {
    tracing::info!("🏥 Checking cluster health");

    let (status, healthy_shards, total_shards, issues) =
        if let Some(ref registry) = state.shard_registry {
            let healthy = registry
                .is_healthy()
                .map_err(|e| ApiError::internal(format!("Health check failed: {}", e)))?;

            let health_pct = registry
                .health_percentage()
                .map_err(|e| ApiError::internal(format!("Health check failed: {}", e)))?;

            let topology = registry
                .get_topology()
                .map_err(|e| ApiError::internal(format!("Failed to get topology: {}", e)))?;

            let status = if healthy {
                "healthy"
            } else if health_pct > 50.0 {
                "degraded"
            } else {
                "critical"
            };

            let healthy_count = topology.healthy_shards(60).len() as u32;

            (
                status.to_string(),
                healthy_count,
                topology.total_shards,
                Vec::new(), // TODO: Collect actual issues
            )
        } else {
            // Single-node mode
            let rdf_healthy = state.rdf_store.is_some();
            let status = if rdf_healthy { "healthy" } else { "critical" };
            (status.to_string(), 1, 1, Vec::new())
        };

    // Calculate uptime
    let uptime_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Ok(Json(ClusterHealthResponse {
        status,
        total_shards,
        healthy_shards,
        degraded_shards: 0,
        down_shards: total_shards - healthy_shards,
        issues,
        last_check: Utc::now(),
        uptime_seconds,
    }))
}

/// GET /api/v1/cluster/config
/// Get cluster configuration
pub async fn get_cluster_config(
    State(_state): State<Arc<ApiState>>,
) -> Result<Json<ClusterConfigResponse>, ApiError> {
    tracing::info!("⚙️ Fetching cluster configuration");

    // Read auto-save interval from environment
    let auto_save_interval = std::env::var("RDF_AUTO_SAVE_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    Ok(Json(ClusterConfigResponse {
        cluster_name: "graphica-cluster".to_string(),
        mode: "single-node".to_string(), // TODO: Detect distributed mode
        auto_scaling: AutoScalingConfig {
            enabled: false,
            min_shards: 1,
            max_shards: 1,
            target_utilization: 0.80,
            scale_out_threshold: 0.90,
            scale_in_threshold: 0.50,
            cooldown_minutes: 15,
        },
        data_retention: DataRetentionConfig {
            auto_save_interval_seconds: auto_save_interval,
            backup_enabled: true,
            backup_interval_hours: 24,
            retention_days: 30,
        },
        performance: PerformanceConfig {
            query_timeout_seconds: 30,
            write_timeout_seconds: 10,
            max_query_result_size: 1_000_000,
            connection_pool_size: 100,
        },
    }))
}

/// POST /api/v1/cluster/scale-out
/// Add new shards to the cluster (stub - single-node mode)
pub async fn scale_out_cluster(
    State(_state): State<Arc<ApiState>>,
    Json(_request): Json<ScaleOutRequest>,
) -> Result<Json<ScaleOutResponse>, ApiError> {
    tracing::warn!("🚧 Scale-out requested but running in single-node mode");

    Ok(Json(ScaleOutResponse {
        operation_id: format!("scale-out-{}", Utc::now().timestamp()),
        status: "not_supported".to_string(),
        old_shard_count: 1,
        new_shard_count: 1,
        message: "Cluster is running in single-node mode. Distributed sharding not yet enabled."
            .to_string(),
        estimated_duration_minutes: None,
        started_at: Utc::now(),
    }))
}

/// GET /api/v1/cluster/shards/:shard_id
/// Get detailed shard information
pub async fn get_shard_detail(
    State(state): State<Arc<ApiState>>,
    Path(shard_id): Path<u32>,
) -> Result<Json<ShardDetailResponse>, ApiError> {
    tracing::info!("🔍 Fetching shard {} details", shard_id);

    if let Some(ref registry) = state.shard_registry {
        let shard = registry
            .get_shard(ShardId(shard_id))
            .map_err(|e| ApiError::internal(format!("Failed to get shard: {}", e)))?
            .ok_or_else(|| ApiError::not_found(format!("Shard {} not found", shard_id)))?;

        Ok(Json(ShardDetailResponse {
            shard_id: shard.shard_id.0,
            status: format!("{}", shard.status),
            leader: ShardNodeInfo {
                address: shard.leader_address.clone(),
                raft_term: shard.raft_term,
                is_healthy: shard.is_healthy(60),
                last_heartbeat: format_timestamp(shard.last_heartbeat),
            },
            replicas: shard
                .replica_addresses
                .iter()
                .map(|addr| ShardReplicaInfo {
                    address: addr.clone(),
                    role: "follower".to_string(),
                    lag_bytes: 0,
                    is_healthy: true,
                })
                .collect(),
            hash_range: HashRangeResponse {
                start: shard.hash_range.start,
                end: shard.hash_range.end,
            },
            statistics: ShardStatistics {
                triple_count: shard.triple_count,
                size_bytes: shard.size_bytes,
                queries_per_second: 0.0,
                writes_per_second: 0.0,
                p99_query_latency_ms: 0.0,
            },
        }))
    } else {
        // Single-node mode
        Err(ApiError::not_found(
            "Sharding not enabled in single-node mode".to_string(),
        ))
    }
}

/// GET /api/v1/cluster/replication/config
/// Get current replication configuration
pub async fn get_replication_config(
    State(_state): State<Arc<ApiState>>,
) -> Result<Json<ReplicationConfigResponse>, ApiError> {
    tracing::info!("🔄 Fetching replication configuration");

    let config = ReplicationConfig::default();

    Ok(Json(ReplicationConfigResponse {
        replication_factor: config.replication_factor,
        sync_replication: config.sync_replication,
        async_replication_lag_ms: config.max_replication_lag_ms,
        raft_election_timeout_ms: config.raft_election_timeout_ms,
        raft_heartbeat_interval_ms: config.raft_heartbeat_interval_ms,
        enable_auto_failover: config.enable_auto_failover,
        failover_timeout_seconds: config.failover_timeout_secs,
    }))
}

/// GET /api/v1/cluster/metadata
/// Get cluster metadata and versioning
pub async fn get_cluster_metadata(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ClusterMetadataResponse>, ApiError> {
    tracing::info!("ℹ️ Fetching cluster metadata");

    let (cluster_version, last_topology_change) = if let Some(ref registry) = state.shard_registry {
        let topology = registry
            .get_topology()
            .map_err(|e| ApiError::internal(format!("Failed to get topology: {}", e)))?;

        (
            topology.cluster_version,
            Some(format_timestamp(topology.updated_at)),
        )
    } else {
        (0, None)
    };

    Ok(Json(ClusterMetadataResponse {
        cluster_id: "graphica-cluster-001".to_string(),
        created_at: "2024-01-15T10:00:00Z".to_string(), // TODO: Persist this
        cluster_version,
        graphica_version: env!("CARGO_PKG_VERSION").to_string(),
        mode: "single-node".to_string(),
        total_operations: 0, // TODO: Track this
        last_topology_change,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn shard_metadata_to_response(shard: &ShardMetadata) -> ShardResponse {
    ShardResponse {
        shard_id: shard.shard_id.0,
        leader_address: shard.leader_address.clone(),
        replica_addresses: shard.replica_addresses.clone(),
        status: format!("{}", shard.status),
        hash_range: HashRangeResponse {
            start: shard.hash_range.start,
            end: shard.hash_range.end,
        },
        triple_count: shard.triple_count,
        size_bytes: shard.size_bytes,
        last_heartbeat: format_timestamp(shard.last_heartbeat),
        raft_term: shard.raft_term,
    }
}

fn create_single_node_topology(state: &ApiState) -> TopologyResponse {
    let total_triples = if let Some(ref rdf_store) = state.rdf_store {
        rdf_store.triple_count().unwrap_or_else(|_| 0)
    } else {
        0
    };

    let size_bytes = total_triples * 500; // Estimate

    // Create synthetic single shard
    let shard = ShardResponse {
        shard_id: 0,
        leader_address: "localhost:8080".to_string(),
        replica_addresses: Vec::new(),
        status: "Active".to_string(),
        hash_range: HashRangeResponse {
            start: 0,
            end: u64::MAX,
        },
        triple_count: total_triples as u64,
        size_bytes: size_bytes as u64,
        last_heartbeat: Utc::now().to_rfc3339(),
        raft_term: 0,
    };

    TopologyResponse {
        total_shards: 1,
        replication_factor: 1,
        cluster_version: 0,
        total_triples: total_triples as u64,
        total_size_bytes: size_bytes as u64,
        updated_at: Utc::now().to_rfc3339(),
        shards: vec![shard],
    }
}

fn format_timestamp(unix_timestamp: u64) -> String {
    DateTime::from_timestamp(unix_timestamp as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string())
}
