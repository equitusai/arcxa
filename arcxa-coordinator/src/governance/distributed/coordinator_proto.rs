//! Generated protocol buffer types for CoordinatorService
//!
//! This module exposes the gRPC service definitions for shard auto-registration
//! and cluster management. Shards use this service to:
//!
//! - Register with the coordinator and receive shard IDs
//! - Send heartbeats with statistics and health status
//! - Deregister gracefully or forcefully
//! - Query cluster topology
//! - Request rebalancing operations
//!
//! ## Service Overview
//!
//! ```text
//! ┌─────────────┐                    ┌──────────────────┐
//! │   Shard 0   │───Register────────▶│                  │
//! │             │◀───Shard ID────────│                  │
//! │   UUID:     │                    │   Coordinator    │
//! │   abc-123   │───Heartbeat───────▶│   (gRPC Server)  │
//! │             │◀───Instructions────│                  │
//! └─────────────┘                    │                  │
//!                                    │  ┌─────────────┐ │
//! ┌─────────────┐                    │  │   Shard     │ │
//! │   Shard 1   │───Register────────▶│  │  Registry   │ │
//! │             │◀───Shard ID────────│  │  (RocksDB)  │ │
//! │   UUID:     │                    │  └─────────────┘ │
//! │   def-456   │───Heartbeat───────▶│                  │
//! │             │◀───Instructions────│                  │
//! └─────────────┘                    └──────────────────┘
//! ```
//!
//! ## Registration Flow
//!
//! **New Shard:**
//! 1. Shard starts, loads or creates identity file with machine_id (UUID)
//! 2. Sends `RegisterShard` request with `NewShardRegistration`
//! 3. Coordinator allocates shard ID, stores machine_id mapping
//! 4. Returns `ShardRegistrationResponse` with shard ID and hash range
//! 5. Shard saves shard ID to identity file
//!
//! **Reconnection (after restart):**
//! 1. Shard loads identity file with shard_id and machine_id
//! 2. Sends `RegisterShard` request with `ExistingShardReconnection`
//! 3. Coordinator verifies machine_id matches stored mapping
//! 4. Updates shard address and status to Active
//! 5. Returns existing shard ID and hash range
//!
//! ## Heartbeat Protocol
//!
//! Shards send periodic heartbeats (default: every 30 seconds) with:
//! - Statistics: triple count, disk usage, query latency percentiles
//! - Health status: HEALTHY, DEGRADED, UNHEALTHY, MAINTENANCE
//! - Timestamp for detection of clock skew
//!
//! Coordinator responds with:
//! - Acknowledgment
//! - Instructions: rebalance, enter readonly mode, prepare shutdown
//! - Config updates: feature flags, batch size, query timeout
//!
//! ## Usage Example
//!
//! ```ignore
//! use tonic::transport::Server;
//! use graphica_coordinator::governance::distributed::coordinator_proto::*;
//!
//! struct CoordinatorServiceImpl {
//!     // ... fields
//! }
//!
//! #[tonic::async_trait]
//! impl coordinator_service_server::CoordinatorService for CoordinatorServiceImpl {
//!     async fn register_shard(
//!         &self,
//!         request: tonic::Request<ShardRegistrationRequest>,
//!     ) -> Result<tonic::Response<ShardRegistrationResponse>, tonic::Status> {
//!         // Implementation
//!         todo!()
//!     }
//!
//!     // ... other methods
//! }
//! ```

// Include generated protobuf code
// Package: graphica.coordinator.v1
pub mod proto {
    tonic::include_proto!("graphica.coordinator.v1");
}

// Re-export commonly used types for convenience
pub use proto::{
    coordinator_service_client::CoordinatorServiceClient,
    coordinator_service_server::{CoordinatorService, CoordinatorServiceServer},
    *,
};

/// File descriptor for gRPC reflection
///
/// This is useful for debugging with tools like `grpcurl`:
///
/// ```bash
/// grpcurl -plaintext -d '{}' \
///   -proto coordinator_service.proto \
///   localhost:9090 \
///   graphica.coordinator.v1.CoordinatorService/GetTopology
/// ```
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/coordinator_service_descriptor.bin"
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_descriptor_set_exists() {
        // Verify the file descriptor set was generated
        assert!(!FILE_DESCRIPTOR_SET.is_empty());
        assert!(FILE_DESCRIPTOR_SET.len() > 100); // Should be substantial
    }

    #[test]
    fn test_message_types_exist() {
        // Verify key message types are generated
        let _: ShardRegistrationRequest;
        let _: ShardRegistrationResponse;
        let _: HeartbeatRequest;
        let _: HeartbeatResponse;
        let _: DeregistrationRequest;
        let _: DeregistrationResponse;
        let _: TopologyRequest;
        let _: TopologyResponse;
    }

    #[test]
    fn test_shard_registration_request_new_shard() {
        // Test creating a new shard registration request
        let request = ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec!["text".to_string(), "spatial".to_string()],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        };

        // Verify the request is valid
        assert!(request.registration_type.is_some());

        if let Some(shard_registration_request::RegistrationType::NewShard(new_shard)) =
            request.registration_type
        {
            assert_eq!(new_shard.machine_id, "550e8400-e29b-41d4-a716-446655440000");
            assert_eq!(new_shard.address, "localhost:9100");
            assert!(new_shard.capabilities.is_some());
        } else {
            panic!("Expected NewShard registration type");
        }
    }

    #[test]
    fn test_shard_registration_request_reconnection() {
        // Test creating a reconnection request
        let request = ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::Reconnect(
                ExistingShardReconnection {
                    shard_id: 42,
                    machine_id: "650e8400-e29b-41d4-a716-446655440001".to_string(),
                    address: "localhost:9101".to_string(),
                    last_checkpoint: 12345,
                    version: "0.2.0".to_string(),
                },
            )),
            auth_token: String::new(),
        };

        assert!(request.registration_type.is_some());

        if let Some(shard_registration_request::RegistrationType::Reconnect(reconnect)) =
            request.registration_type
        {
            assert_eq!(reconnect.shard_id, 42);
            assert_eq!(reconnect.machine_id, "650e8400-e29b-41d4-a716-446655440001");
            assert_eq!(reconnect.last_checkpoint, 12345);
        } else {
            panic!("Expected Reconnect registration type");
        }
    }

    #[test]
    fn test_heartbeat_request() {
        // Test creating a heartbeat request
        let request = HeartbeatRequest {
            shard_id: 1,
            machine_id: "machine-uuid".to_string(),
            stats: Some(ShardStatistics {
                triple_count: 1_000_000,
                disk_usage_bytes: 10_737_418_240,  // 10 GB
                memory_usage_bytes: 2_147_483_648, // 2 GB
                queries_processed: 50_000,
                inserts_processed: 10_000,
                deletes_processed: 500,
                p50_latency_ms: 5.0,
                p95_latency_ms: 25.0,
                p99_latency_ms: 100.0,
                query_errors: 10,
                insert_errors: 2,
                replication_lag_ms: 50,
            }),
            timestamp: 1678900000,
            health: Some(HealthStatus {
                status: health_status::Status::Healthy as i32,
                message: "All systems operational".to_string(),
                component_health: std::collections::HashMap::from([
                    ("rocksdb".to_string(), true),
                    ("oxigraph".to_string(), true),
                    ("network".to_string(), true),
                ]),
            }),
        };

        assert_eq!(request.shard_id, 1);
        assert!(request.stats.is_some());
        assert!(request.health.is_some());

        let stats = request.stats.unwrap();
        assert_eq!(stats.triple_count, 1_000_000);
        assert_eq!(stats.p99_latency_ms, 100.0);
    }

    #[test]
    fn test_topology_response() {
        // Test creating a topology response
        let response = TopologyResponse {
            version: 42,
            total_shards: 3,
            replication_factor: 2,
            shards: vec![ShardInfo {
                shard_id: 0,
                machine_id: "machine-0".to_string(),
                hash_range: Some(HashRange {
                    start: 0,
                    end: u64::MAX / 3,
                }),
                status: Some(ShardStatus {
                    state: shard_status::State::Active as i32,
                    message: "Running".to_string(),
                }),
                leader_address: "shard-0:9100".to_string(),
                replica_addresses: vec!["shard-0-replica:9100".to_string()],
                stats: None,
                last_heartbeat: 1678900000,
            }],
            cluster_stats: Some(ClusterStatistics {
                total_triples: 3_000_000,
                total_disk_bytes: 32_212_254_720,  // 30 GB
                total_memory_bytes: 6_442_450_944, // 6 GB
                queries_per_second: 1000,
                inserts_per_second: 200,
                healthy_shards: 3,
                degraded_shards: 0,
                down_shards: 0,
                load_balance_ratio: 0.05, // 5% imbalance
            }),
            updated_at: 1678900100,
        };

        assert_eq!(response.version, 42);
        assert_eq!(response.total_shards, 3);
        assert_eq!(response.shards.len(), 1);
        assert!(response.cluster_stats.is_some());
    }
}
