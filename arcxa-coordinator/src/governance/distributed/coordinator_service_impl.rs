//! CoordinatorService gRPC Implementation
//!
//! This module implements the gRPC service for shard auto-registration and
//! cluster management. It's the primary communication interface between shards
//! and the coordinator.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────┐
//! │  Shard Process   │
//! │                  │
//! │  - Loads/creates │
//! │    identity file │
//! │  - Calls gRPC    │
//! └────────┬─────────┘
//!          │ RegisterShard
//!          │ Heartbeat (periodic)
//!          │ Deregister
//!          ▼
//! ┌──────────────────────────────┐
//! │  CoordinatorServiceImpl      │
//! │                              │
//! │  ┌────────────────────────┐  │
//! │  │ AutoRegistrationHandler│  │
//! │  │  - Validates machine ID│  │
//! │  │  - Allocates shard ID  │  │
//! │  │  - Stores mappings     │  │
//! │  └───────────┬────────────┘  │
//! │              │                │
//! │  ┌───────────▼────────────┐  │
//! │  │   ShardRegistry        │  │
//! │  │   (RocksDB)            │  │
//! │  │  - Topology            │  │
//! │  │  - Machine ID mappings │  │
//! │  └────────────────────────┘  │
//! └──────────────────────────────┘
//! ```
//!
//! ## Error Handling Strategy
//!
//! - **InvalidArgument**: Bad request data (invalid UUID, missing fields)
//! - **AlreadyExists**: Duplicate machine ID trying to register
//! - **NotFound**: Shard doesn't exist (reconnection failed)
//! - **PermissionDenied**: Machine ID mismatch on reconnection
//! - **ResourceExhausted**: Max shard count reached
//! - **Internal**: Database errors, unexpected failures
//!
//! ## Security Considerations
//!
//! - **Machine ID Verification**: Prevents shard impersonation
//! - **Rate Limiting**: (Future) Prevents DoS via registration spam
//! - **Auth Tokens**: (Future) JWT-based authentication
//! - **TLS**: (Deployment) mTLS for production environments

use super::auto_registration::{AutoRegistrationHandler, RegistrationConfig};
use super::coordinator_proto::*;
use super::shard_registry::ShardRegistry;
use anyhow::Result;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

/// Coordinator gRPC service implementation
///
/// This is the main service struct that implements the CoordinatorService trait
/// generated from the protobuf definition.
pub struct CoordinatorServiceImpl {
    /// Registration handler for shard lifecycle management
    registration_handler: Arc<AutoRegistrationHandler>,

    /// Direct access to shard registry for queries
    shard_registry: Arc<ShardRegistry>,

    /// Configuration
    config: CoordinatorServiceConfig,
}

/// Configuration for coordinator service
#[derive(Debug, Clone)]
pub struct CoordinatorServiceConfig {
    /// Enable authentication (future)
    pub enable_auth: bool,

    /// Coordinator version string
    pub coordinator_version: String,

    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u32,

    /// Statistics reporting interval in seconds
    pub stats_reporting_interval_secs: u32,

    /// Enable compression for responses
    pub enable_compression: bool,

    /// Maximum number of shards
    pub max_shards: u32,
}

impl Default for CoordinatorServiceConfig {
    fn default() -> Self {
        Self {
            enable_auth: false,
            coordinator_version: env!("CARGO_PKG_VERSION").to_string(),
            heartbeat_interval_secs: 30,
            stats_reporting_interval_secs: 60,
            enable_compression: true,
            max_shards: 100,
        }
    }
}

impl CoordinatorServiceImpl {
    /// Create a new coordinator service
    ///
    /// # Arguments
    ///
    /// * `shard_registry` - Shared shard registry instance
    /// * `config` - Service configuration
    ///
    /// # Returns
    ///
    /// A new service instance ready to handle gRPC requests
    pub fn new(
        shard_registry: Arc<ShardRegistry>,
        config: CoordinatorServiceConfig,
    ) -> Result<Self> {
        // Create registration configuration from service config
        let reg_config = RegistrationConfig {
            max_shards: config.max_shards,
            auto_rebalance: true,
            rebalance_threshold_percent: 20.0,
            rebalance_cooldown_secs: 300,
            allow_duplicate_machines: false,
        };

        // Create registration handler
        let registration_handler = Arc::new(AutoRegistrationHandler::new(
            shard_registry.clone(),
            reg_config,
        ));

        Ok(Self {
            registration_handler,
            shard_registry,
            config,
        })
    }

    /// Get machine ID for a given shard ID
    fn get_machine_id_for_shard(&self, shard_id: u32) -> Option<String> {
        // Get all machine ID mappings and find the one for this shard
        if let Ok(mappings) = self.shard_registry.get_all_machine_id_mappings() {
            for (machine_id, mapped_shard_id) in mappings {
                if mapped_shard_id.0 == shard_id {
                    return Some(machine_id);
                }
            }
        }
        None
    }

    /// Create shard configuration response
    fn create_shard_config(&self) -> ShardConfiguration {
        ShardConfiguration {
            heartbeat_interval_secs: self.config.heartbeat_interval_secs,
            stats_reporting_interval_secs: self.config.stats_reporting_interval_secs,
            enable_compression: self.config.enable_compression,
            enable_encryption: false, // Future feature
            batch_size: 1000,
            max_concurrent_queries: 100,
            query_timeout_secs: 300,
            feature_flags: std::collections::HashMap::new(),
            replication: None, // Future feature
        }
    }

    /// Validate authentication token (future implementation)
    fn validate_auth_token(&self, _token: &str) -> Result<(), Status> {
        if self.config.enable_auth {
            // TODO: Implement JWT validation
            return Err(Status::unauthenticated("Authentication not implemented"));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl CoordinatorService for CoordinatorServiceImpl {
    /// Register a new shard or reconnect existing one
    ///
    /// This is the entry point for shard auto-discovery. Shards call this on startup
    /// to either register as a new shard or reconnect after a restart.
    async fn register_shard(
        &self,
        request: Request<ShardRegistrationRequest>,
    ) -> Result<Response<ShardRegistrationResponse>, Status> {
        let req = request.into_inner();

        // Validate auth token
        self.validate_auth_token(&req.auth_token)?;

        // Process registration based on type
        match req.registration_type {
            Some(shard_registration_request::RegistrationType::NewShard(new_shard)) => {
                self.handle_new_shard_registration(new_shard).await
            }
            Some(shard_registration_request::RegistrationType::Reconnect(reconnect)) => {
                self.handle_shard_reconnection(reconnect).await
            }
            None => Err(Status::invalid_argument(
                "Registration type must be specified (NewShard or Reconnect)",
            )),
        }
    }

    /// Send heartbeat with statistics
    ///
    /// Shards send periodic heartbeats to:
    /// - Prove liveness (coordinator tracks last heartbeat timestamp)
    /// - Report statistics (triple count, disk usage, query latency)
    /// - Report health status
    /// - Receive instructions from coordinator (rebalance, config updates, etc.)
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        // Update heartbeat timestamp
        self.shard_registry
            .update_heartbeat(super::shard_metadata::ShardId(req.shard_id))
            .map_err(|e| {
                error!(
                    "Failed to update heartbeat for shard {}: {}",
                    req.shard_id, e
                );
                Status::internal(format!("Failed to update heartbeat: {}", e))
            })?;

        // Update statistics if provided
        if let Some(stats) = req.stats {
            self.shard_registry
                .update_shard_stats(
                    super::shard_metadata::ShardId(req.shard_id),
                    stats.triple_count,
                    stats.disk_usage_bytes,
                )
                .map_err(|e| {
                    error!("Failed to update stats for shard {}: {}", req.shard_id, e);
                    Status::internal(format!("Failed to update statistics: {}", e))
                })?;
        }

        // Get current topology version
        let topology = self.shard_registry.get_topology().map_err(|e| {
            error!("Failed to get topology: {}", e);
            Status::internal("Failed to get cluster topology")
        })?;

        // Create response with instructions (future: rebalancing, config updates)
        let response = HeartbeatResponse {
            acknowledged: true,
            instructions: vec![], // TODO: Implement instruction generation
            updated_config: None,
            topology_version: topology.cluster_version,
        };

        info!(
            "Heartbeat received from shard {} (machine_id: {})",
            req.shard_id, req.machine_id
        );

        Ok(Response::new(response))
    }

    /// Gracefully deregister shard
    ///
    /// Called when a shard is shutting down gracefully. This allows the coordinator
    /// to:
    /// - Mark shard as draining (stop sending new queries)
    /// - Initiate data migration to other shards
    /// - Clean up resources
    async fn deregister_shard(
        &self,
        request: Request<DeregistrationRequest>,
    ) -> Result<Response<DeregistrationResponse>, Status> {
        let req = request.into_inner();

        info!(
            "Deregistration request from shard {} (graceful: {}, reason: {})",
            req.shard_id, req.graceful, req.reason
        );

        // Deregister via handler
        self.registration_handler
            .deregister_shard(req.shard_id, req.machine_id.clone(), req.graceful)
            .await
            .map_err(|e| {
                error!("Failed to deregister shard {}: {}", req.shard_id, e);
                Status::internal(format!("Deregistration failed: {}", e))
            })?;

        let response = DeregistrationResponse {
            success: true,
            migration_tasks: vec![], // TODO: Implement migration tasks for graceful shutdown
            message: format!(
                "Shard {} deregistered successfully ({})",
                req.shard_id,
                if req.graceful { "graceful" } else { "forced" }
            ),
        };

        Ok(Response::new(response))
    }

    /// Get current cluster topology
    ///
    /// Returns the current state of the cluster including:
    /// - All registered shards with their hash ranges
    /// - Cluster statistics (total triples, QPS, etc.)
    /// - Health status
    async fn get_topology(
        &self,
        request: Request<TopologyRequest>,
    ) -> Result<Response<TopologyResponse>, Status> {
        let req = request.into_inner();

        let topology = self.shard_registry.get_topology().map_err(|e| {
            error!("Failed to get topology: {}", e);
            Status::internal("Failed to get cluster topology")
        })?;

        // Build shard info list
        let mut shards = Vec::new();
        for (shard_id, shard_meta) in &topology.shards {
            // Skip inactive shards unless requested
            if !req.include_inactive
                && shard_meta.status != super::shard_metadata::ShardStatus::Active
            {
                continue;
            }

            // Look up machine_id from the registry's machine ID mappings
            let machine_id = self
                .get_machine_id_for_shard(shard_id.0)
                .unwrap_or_default();

            let shard_info = ShardInfo {
                shard_id: shard_id.0,
                machine_id,
                hash_range: Some(HashRange {
                    start: shard_meta.hash_range.start,
                    end: shard_meta.hash_range.end,
                }),
                status: Some(ShardStatus {
                    state: match shard_meta.status {
                        super::shard_metadata::ShardStatus::Provisioning => {
                            shard_status::State::Provisioning as i32
                        }
                        super::shard_metadata::ShardStatus::Active => {
                            shard_status::State::Active as i32
                        }
                        super::shard_metadata::ShardStatus::Draining => {
                            shard_status::State::Draining as i32
                        }
                        super::shard_metadata::ShardStatus::Degraded => {
                            shard_status::State::Degraded as i32
                        }
                        super::shard_metadata::ShardStatus::Down => {
                            shard_status::State::Down as i32
                        }
                    },
                    message: "Operational".to_string(),
                }),
                leader_address: shard_meta.leader_address.clone(),
                replica_addresses: shard_meta.replica_addresses.clone(),
                stats: if req.include_stats {
                    Some(ShardStatistics {
                        triple_count: shard_meta.triple_count,
                        disk_usage_bytes: shard_meta.size_bytes,
                        memory_usage_bytes: 0, // TODO: Track memory usage
                        queries_processed: 0,  // TODO: Track queries
                        inserts_processed: 0,
                        deletes_processed: 0,
                        p50_latency_ms: 0.0,
                        p95_latency_ms: 0.0,
                        p99_latency_ms: 0.0,
                        query_errors: 0,
                        insert_errors: 0,
                        replication_lag_ms: 0,
                    })
                } else {
                    None
                },
                last_heartbeat: shard_meta.last_heartbeat,
            };

            shards.push(shard_info);
        }

        // Calculate cluster statistics
        let total_triples: u64 = topology.shards.values().map(|s| s.triple_count).sum();
        let total_disk_bytes: u64 = topology.shards.values().map(|s| s.size_bytes).sum();

        let cluster_stats = ClusterStatistics {
            total_triples,
            total_disk_bytes,
            total_memory_bytes: 0,
            queries_per_second: 0,
            inserts_per_second: 0,
            healthy_shards: topology.total_shards as u32,
            degraded_shards: 0,
            down_shards: 0,
            load_balance_ratio: 0.0, // TODO: Calculate imbalance ratio
        };

        let response = TopologyResponse {
            version: topology.cluster_version,
            total_shards: topology.total_shards,
            replication_factor: topology.replication_factor,
            shards,
            cluster_stats: Some(cluster_stats),
            updated_at: chrono::Utc::now().timestamp() as u64,
        };

        Ok(Response::new(response))
    }

    /// Request hash range rebalancing
    ///
    /// Triggers rebalancing of data across shards to maintain even distribution.
    /// This is typically called:
    /// - After adding new shards
    /// - After detecting significant load imbalance
    /// - Manually by operators
    async fn request_rebalance(
        &self,
        request: Request<RebalanceRequest>,
    ) -> Result<Response<RebalanceResponse>, Status> {
        let req = request.into_inner();

        warn!(
            "Rebalance requested (force: {}, target_ratio: {})",
            req.force, req.target_balance_ratio
        );

        // TODO: Implement rebalancing logic
        // For now, return not implemented

        let response = RebalanceResponse {
            initiated: false,
            planned_moves: vec![],
            estimated_duration_secs: 0,
            rejection_reason: "Rebalancing not yet implemented".to_string(),
        };

        Ok(Response::new(response))
    }
}

// Private implementation methods
impl CoordinatorServiceImpl {
    /// Handle new shard registration
    async fn handle_new_shard_registration(
        &self,
        new_shard: NewShardRegistration,
    ) -> Result<Response<ShardRegistrationResponse>, Status> {
        info!(
            "New shard registration: machine_id={}, address={}",
            new_shard.machine_id, new_shard.address
        );

        // Extract capabilities from request
        let capabilities = new_shard
            .capabilities
            .ok_or_else(|| Status::invalid_argument("Shard capabilities are required"))?;

        // Register via handler
        let result = self
            .registration_handler
            .register_new_shard(
                new_shard.machine_id.clone(),
                new_shard.address.clone(),
                capabilities,
                Some(new_shard.data_path.clone()),
            )
            .await
            .map_err(|e| {
                error!("Failed to register shard {}: {}", new_shard.machine_id, e);

                // Convert specific errors to appropriate gRPC status codes
                if e.to_string().contains("already registered") {
                    Status::already_exists(format!("Machine ID already registered: {}", e))
                } else if e.to_string().contains("Maximum shard count") {
                    Status::resource_exhausted(e.to_string())
                } else if e.to_string().contains("Invalid machine ID format") {
                    Status::invalid_argument(e.to_string())
                } else {
                    Status::internal(format!("Registration failed: {}", e))
                }
            })?;

        info!(
            "Shard registered successfully: id={}, machine_id={}, range={:?}-{:?}",
            result.shard_id, new_shard.machine_id, result.hash_range.start, result.hash_range.end
        );

        // Build response
        let response = ShardRegistrationResponse {
            success: true,
            assigned_shard_id: result.shard_id,
            hash_range: Some(HashRange {
                start: result.hash_range.start,
                end: result.hash_range.end,
            }),
            coordinator_version: self.config.coordinator_version.clone(),
            config: Some(self.create_shard_config()),
            rejection_reason: String::new(),
            topology_version: result.topology_version,
        };

        Ok(Response::new(response))
    }

    /// Handle shard reconnection
    async fn handle_shard_reconnection(
        &self,
        reconnect: ExistingShardReconnection,
    ) -> Result<Response<ShardRegistrationResponse>, Status> {
        info!(
            "Shard reconnection: shard_id={}, machine_id={}, address={}",
            reconnect.shard_id, reconnect.machine_id, reconnect.address
        );

        // Reconnect via handler
        let result = self
            .registration_handler
            .reconnect_shard(
                reconnect.shard_id,
                reconnect.machine_id.clone(),
                reconnect.address.clone(),
                Some(reconnect.last_checkpoint),
            )
            .await
            .map_err(|e| {
                error!("Failed to reconnect shard {}: {}", reconnect.shard_id, e);

                // Convert specific errors to appropriate gRPC status codes
                if e.to_string().contains("not found") {
                    Status::not_found(format!(
                        "Shard {} not found in registry",
                        reconnect.shard_id
                    ))
                } else if e.to_string().contains("Machine ID mismatch") {
                    Status::permission_denied(format!(
                        "Machine ID verification failed for shard {}: {}",
                        reconnect.shard_id, e
                    ))
                } else {
                    Status::internal(format!("Reconnection failed: {}", e))
                }
            })?;

        info!(
            "Shard reconnected successfully: id={}, machine_id={}",
            reconnect.shard_id, reconnect.machine_id
        );

        // Build response
        let response = ShardRegistrationResponse {
            success: true,
            assigned_shard_id: result.shard_id,
            hash_range: Some(HashRange {
                start: result.hash_range.start,
                end: result.hash_range.end,
            }),
            coordinator_version: self.config.coordinator_version.clone(),
            config: Some(self.create_shard_config()),
            rejection_reason: String::new(),
            topology_version: result.topology_version,
        };

        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_service() -> (CoordinatorServiceImpl, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let registry = Arc::new(ShardRegistry::new(temp_dir.path(), 3, 60).unwrap());
        let config = CoordinatorServiceConfig::default();
        let service = CoordinatorServiceImpl::new(registry, config).unwrap();
        (service, temp_dir)
    }

    #[tokio::test]
    async fn test_register_new_shard() {
        let (service, _temp) = create_test_service().await;

        let request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: uuid::Uuid::new_v4().to_string(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec!["text".to_string()],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        let response = service.register_shard(request).await.unwrap();
        let resp = response.into_inner();

        assert!(resp.success);
        assert_eq!(resp.assigned_shard_id, 0, "First shard should get ID 0");
        assert!(resp.hash_range.is_some());
        assert!(resp.config.is_some());
    }

    #[tokio::test]
    async fn test_register_duplicate_machine_id() {
        let (service, _temp) = create_test_service().await;

        let machine_id = uuid::Uuid::new_v4().to_string();

        // First registration should succeed
        let request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        service.register_shard(request).await.unwrap();

        // Second registration with same machine ID should fail
        let request2 = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id,
                    address: "localhost:9101".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-1".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 1,
                },
            )),
            auth_token: String::new(),
        });

        let result = service.register_shard(request2).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn test_reconnect_shard() {
        let (service, _temp) = create_test_service().await;

        let machine_id = uuid::Uuid::new_v4().to_string();

        // First, register a new shard
        let register_request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        let register_response = service.register_shard(register_request).await.unwrap();
        let shard_id = register_response.into_inner().assigned_shard_id;

        // Now reconnect with same machine ID
        let reconnect_request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::Reconnect(
                ExistingShardReconnection {
                    shard_id,
                    machine_id,
                    address: "localhost:9101".to_string(), // Different address
                    last_checkpoint: 12345,
                    version: "0.2.0".to_string(),
                },
            )),
            auth_token: String::new(),
        });

        let reconnect_response = service.register_shard(reconnect_request).await.unwrap();
        let resp = reconnect_response.into_inner();

        assert!(resp.success);
        assert_eq!(resp.assigned_shard_id, shard_id);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let (service, _temp) = create_test_service().await;

        // Register a shard first
        let machine_id = uuid::Uuid::new_v4().to_string();
        let register_request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        let register_response = service.register_shard(register_request).await.unwrap();
        let shard_id = register_response.into_inner().assigned_shard_id;

        // Send heartbeat
        let heartbeat_request = Request::new(HeartbeatRequest {
            shard_id,
            machine_id,
            stats: Some(ShardStatistics {
                triple_count: 1_000_000,
                disk_usage_bytes: 10_737_418_240,
                memory_usage_bytes: 2_147_483_648,
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
            timestamp: chrono::Utc::now().timestamp() as u64,
            health: Some(HealthStatus {
                status: health_status::Status::Healthy as i32,
                message: "All systems operational".to_string(),
                component_health: std::collections::HashMap::new(),
            }),
        });

        let heartbeat_response = service.heartbeat(heartbeat_request).await.unwrap();
        let resp = heartbeat_response.into_inner();

        assert!(resp.acknowledged);
    }

    #[tokio::test]
    async fn test_get_topology() {
        let (service, _temp) = create_test_service().await;

        // Register a couple of shards
        for i in 0..2 {
            let machine_id = uuid::Uuid::new_v4().to_string();
            let request = Request::new(ShardRegistrationRequest {
                registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                    NewShardRegistration {
                        machine_id,
                        address: format!("localhost:910{}", i),
                        capabilities: Some(ShardCapabilities {
                            max_memory_mb: 8192,
                            disk_space_mb: 500_000,
                            cpu_cores: 8,
                            supports_compression: true,
                            supports_encryption: false,
                            supported_indexes: vec![],
                            network_bandwidth_mbps: 10_000,
                        }),
                        data_path: format!("/data/shard-{}", i),
                        version: "0.2.0".to_string(),
                        preferred_shard_id: i,
                    },
                )),
                auth_token: String::new(),
            });

            service.register_shard(request).await.unwrap();
        }

        // Get topology (include inactive to see provisioning shards)
        let topology_request = Request::new(TopologyRequest {
            include_inactive: true, // Include all shards, even those provisioning
            include_stats: true,
        });

        let topology_response = service.get_topology(topology_request).await.unwrap();
        let resp = topology_response.into_inner();

        assert_eq!(resp.total_shards, 2);
        assert_eq!(resp.shards.len(), 2);
        assert!(resp.cluster_stats.is_some());
    }

    #[tokio::test]
    async fn test_deregister_shard() {
        let (service, _temp) = create_test_service().await;

        // Register a shard first
        let machine_id = uuid::Uuid::new_v4().to_string();
        let register_request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: "localhost:9100".to_string(),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 10_000,
                    }),
                    data_path: "/data/shard-0".to_string(),
                    version: "0.2.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        let register_response = service.register_shard(register_request).await.unwrap();
        let shard_id = register_response.into_inner().assigned_shard_id;

        // Deregister
        let deregister_request = Request::new(DeregistrationRequest {
            shard_id,
            machine_id,
            reason: "Shutting down for maintenance".to_string(),
            graceful: false,
        });

        let deregister_response = service.deregister_shard(deregister_request).await.unwrap();
        let resp = deregister_response.into_inner();

        assert!(resp.success);
    }
}
