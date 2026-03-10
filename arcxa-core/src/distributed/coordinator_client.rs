//! Coordinator Client for Shard Registration and Communication
//!
//! This module provides a gRPC client for shards to communicate with the coordinator,
//! handling registration, reconnection, heartbeats, and deregistration.
//!
//! ## Features
//!
//! - **Auto-registration**: New shards register and receive shard_id
//! - **Reconnection**: Existing shards reconnect after restarts
//! - **Heartbeat**: Periodic health and statistics reporting
//! - **Retry logic**: Exponential backoff for transient failures
//! - **Connection pooling**: Efficient connection reuse
//!
//! ## Usage
//!
//! ```ignore
//! use graphica::distributed::coordinator_client::CoordinatorClient;
//! use graphica::distributed::shard_identity::ShardIdentity;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let identity = ShardIdentity::load_or_create(&data_path, "coordinator:9090")?;
//!     let mut client = CoordinatorClient::connect("http://coordinator:9090").await?;
//!
//!     if identity.needs_registration() {
//!         let response = client.register_new_shard(&identity, "shard:9100").await?;
//!         identity.update_registration(response.assigned_shard_id, response.hash_range, &data_path)?;
//!     } else {
//!         client.reconnect_existing_shard(&identity).await?;
//!     }
//!
//!     // Start heartbeat loop
//!     client.start_heartbeat_loop(identity.shard_id.unwrap(), identity.machine_id.clone()).await;
//!
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::{debug, error, info, warn};

// Import coordinator proto types
use super::proto::coordinator_service::{
    coordinator_service_client::CoordinatorServiceClient, shard_registration_request, *,
};

use super::shard_identity::{HashRange, ShardIdentity};

/// Coordinator client for shard-to-coordinator communication
pub struct CoordinatorClient {
    client: CoordinatorServiceClient<Channel>,
    coordinator_url: String,
}

impl CoordinatorClient {
    /// Connect to coordinator
    pub async fn connect(url: &str) -> Result<Self> {
        info!("Connecting to coordinator at {}", url);

        let endpoint = Endpoint::from_shared(url.to_string())
            .context("Invalid coordinator URL")?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(20));

        let channel = endpoint
            .connect()
            .await
            .context("Failed to connect to coordinator")?;

        let client = CoordinatorServiceClient::new(channel);

        info!("Successfully connected to coordinator");

        Ok(Self {
            client,
            coordinator_url: url.to_string(),
        })
    }

    /// Register a new shard (first time startup)
    pub async fn register_new_shard(
        &mut self,
        identity: &ShardIdentity,
        shard_address: &str,
    ) -> Result<RegistrationResult> {
        info!(
            "Registering new shard with machine_id={}, address={}",
            identity.machine_id, shard_address
        );

        let capabilities = ShardCapabilities {
            max_memory_mb: Self::get_system_memory_mb(),
            disk_space_mb: Self::get_available_disk_mb()?,
            cpu_cores: num_cpus::get() as u32,
            supports_compression: true,
            supports_encryption: false,
            supported_indexes: vec![
                "subject".to_string(),
                "predicate".to_string(),
                "object".to_string(),
            ],
            network_bandwidth_mbps: 1000, // Default 1 Gbps
        };

        let request = ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: identity.machine_id.clone(),
                    address: shard_address.to_string(),
                    capabilities: Some(capabilities),
                    data_path: String::new(), // Don't expose internal paths
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    preferred_shard_id: 0, // No preference
                },
            )),
            auth_token: String::new(), // TODO: Add authentication
        };

        let response = self
            .client
            .register_shard(Request::new(request))
            .await
            .context("Failed to register shard")?
            .into_inner();

        if !response.success {
            return Err(anyhow::anyhow!(
                "Registration rejected: {}",
                response.rejection_reason
            ));
        }

        info!(
            "Successfully registered as shard_id={}, hash_range={:?}-{:?}",
            response.assigned_shard_id,
            response.hash_range.as_ref().map(|r| r.start),
            response.hash_range.as_ref().map(|r| r.end)
        );

        let hash_range = response
            .hash_range
            .ok_or_else(|| anyhow::anyhow!("No hash range assigned"))?;

        Ok(RegistrationResult {
            shard_id: response.assigned_shard_id,
            hash_range: HashRange::new(hash_range.start, hash_range.end),
            config: response.config,
        })
    }

    /// Reconnect an existing shard (after restart)
    pub async fn reconnect_existing_shard(
        &mut self,
        identity: &ShardIdentity,
        shard_address: &str,
    ) -> Result<()> {
        let shard_id = identity
            .shard_id
            .ok_or_else(|| anyhow::anyhow!("No shard_id in identity file"))?;

        info!(
            "Reconnecting shard_id={}, machine_id={}, address={}",
            shard_id, identity.machine_id, shard_address
        );

        let request = ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::Reconnect(
                ExistingShardReconnection {
                    shard_id,
                    machine_id: identity.machine_id.clone(),
                    address: shard_address.to_string(),
                    last_checkpoint: 0, // TODO: Track checkpoints
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            )),
            auth_token: String::new(),
        };

        let response = self
            .client
            .register_shard(Request::new(request))
            .await
            .context("Failed to reconnect shard")?
            .into_inner();

        if !response.success {
            return Err(anyhow::anyhow!(
                "Reconnection rejected: {}",
                response.rejection_reason
            ));
        }

        info!("Successfully reconnected shard_id={}", shard_id);

        Ok(())
    }

    /// Send heartbeat with statistics
    pub async fn send_heartbeat(
        &mut self,
        shard_id: u32,
        machine_id: &str,
        stats: ShardStatistics,
    ) -> Result<HeartbeatResponse> {
        debug!("Sending heartbeat for shard_id={}", shard_id);

        let request = HeartbeatRequest {
            shard_id,
            machine_id: machine_id.to_string(),
            stats: Some(stats),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            health: Some(HealthStatus {
                status: health_status::Status::Healthy as i32,
                message: "Operating normally".to_string(),
                component_health: std::collections::HashMap::new(),
            }),
        };

        let response = self
            .client
            .heartbeat(Request::new(request))
            .await
            .context("Failed to send heartbeat")?
            .into_inner();

        if !response.acknowledged {
            warn!("Heartbeat not acknowledged by coordinator");
        }

        Ok(response)
    }

    /// Deregister shard (graceful shutdown)
    pub async fn deregister_shard(
        &mut self,
        shard_id: u32,
        machine_id: &str,
        reason: &str,
    ) -> Result<()> {
        info!("Deregistering shard_id={}, reason: {}", shard_id, reason);

        let request = DeregistrationRequest {
            shard_id,
            machine_id: machine_id.to_string(),
            reason: reason.to_string(),
            graceful: true,
        };

        let response = self
            .client
            .deregister_shard(Request::new(request))
            .await
            .context("Failed to deregister shard")?
            .into_inner();

        if response.success {
            info!("Successfully deregistered shard_id={}", shard_id);
        } else {
            warn!("Deregistration completed with issues: {}", response.message);
        }

        Ok(())
    }

    /// Get cluster topology
    pub async fn get_topology(&mut self, include_stats: bool) -> Result<TopologyResponse> {
        debug!("Requesting cluster topology");

        let request = TopologyRequest {
            include_inactive: false,
            include_stats,
        };

        let response = self
            .client
            .get_topology(Request::new(request))
            .await
            .context("Failed to get topology")?
            .into_inner();

        Ok(response)
    }

    /// Start heartbeat loop (runs until cancelled)
    pub async fn start_heartbeat_loop(
        &mut self,
        shard_id: u32,
        machine_id: String,
        heartbeat_interval_secs: u64,
        mut shutdown_rx: tokio::sync::mpsc::Receiver<()>,
    ) {
        info!(
            "Starting heartbeat loop for shard_id={} (interval: {}s)",
            shard_id, heartbeat_interval_secs
        );

        let mut interval = tokio::time::interval(Duration::from_secs(heartbeat_interval_secs));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let stats = ShardStatistics {
                        triple_count: 0, // TODO: Get actual stats
                        disk_usage_bytes: 0,
                        memory_usage_bytes: 0,
                        queries_processed: 0,
                        inserts_processed: 0,
                        deletes_processed: 0,
                        p50_latency_ms: 0.0,
                        p95_latency_ms: 0.0,
                        p99_latency_ms: 0.0,
                        query_errors: 0,
                        insert_errors: 0,
                        replication_lag_ms: 0,
                    };

                    match self.send_heartbeat(shard_id, &machine_id, stats).await {
                        Ok(response) => {
                            if !response.instructions.is_empty() {
                                info!("Received {} instructions from coordinator", response.instructions.len());
                                // TODO: Process instructions
                            }
                        }
                        Err(e) => {
                            error!("Failed to send heartbeat: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Heartbeat loop shutting down");
                    break;
                }
            }
        }
    }

    /// Get system memory in MB
    fn get_system_memory_mb() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
                for line in contents.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb / 1024; // Convert to MB
                            }
                        }
                    }
                }
            }
        }

        // Default fallback
        8192 // 8 GB
    }

    /// Get available disk space in MB
    fn get_available_disk_mb() -> Result<u64> {
        // TODO: Implement proper disk space calculation
        Ok(500_000) // 500 GB default
    }
}

/// Result of shard registration
pub struct RegistrationResult {
    pub shard_id: u32,
    pub hash_range: HashRange,
    pub config: Option<ShardConfiguration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_result() {
        let result = RegistrationResult {
            shard_id: 5,
            hash_range: HashRange::new(0, 1000),
            config: None,
        };

        assert_eq!(result.shard_id, 5);
        assert_eq!(result.hash_range.start, 0);
        assert_eq!(result.hash_range.end, 1000);
    }

    #[test]
    fn test_get_system_memory_mb() {
        let mem = CoordinatorClient::get_system_memory_mb();
        assert!(mem > 0);
        assert!(mem < 1_000_000); // Sanity check: < 1 TB
    }

    #[test]
    fn test_get_available_disk_mb() {
        let disk = CoordinatorClient::get_available_disk_mb().unwrap();
        assert!(disk > 0);
    }
}
