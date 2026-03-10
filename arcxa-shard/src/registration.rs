//! Shard Auto-Registration Client
//!
//! This module handles connecting to the coordinator, registering the shard,
//! and sending periodic heartbeats.
//!
//! ## Registration Flow
//!
//! 1. Load or create shard identity (machine ID)
//! 2. Connect to coordinator via gRPC
//! 3. Send registration request (new shard or reconnection)
//! 4. Receive assigned shard ID and hash range
//! 5. Start heartbeat loop
//!
//! ## Heartbeat Protocol
//!
//! Shards send periodic heartbeats with statistics:
//! - Triple count
//! - Disk usage
//! - Memory usage
//! - Query latency percentiles
//!
//! Coordinator responds with instructions (rebalance, readonly mode, etc.)

use anyhow::{Context, Result};
use graphica_core::distributed::proto::coordinator_service::{
    coordinator_service_client::CoordinatorServiceClient,
    *,
};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tokio::time::interval;
use tonic::transport::Channel;
use tracing::{info, warn, error};

use crate::identity::ShardIdentity;
use crate::instructions::InstructionHandler;
use crate::metrics::{CheckpointTracker, ShardMetricsCollector, SystemMetricsCollector};

/// Configuration for registration client
#[derive(Debug, Clone)]
pub struct RegistrationConfig {
    /// Coordinator URL
    pub coordinator_url: String,

    /// Heartbeat interval in seconds
    pub heartbeat_interval: Duration,

    /// Shard address (for coordinator to connect back)
    pub shard_address: String,

    /// Maximum memory in MB
    pub max_memory_mb: u64,

    /// Disk space in MB
    pub disk_space_mb: u64,

    /// CPU cores
    pub cpu_cores: u32,
}

/// Registration client for shard auto-registration
pub struct RegistrationClient {
    /// gRPC client
    client: CoordinatorServiceClient<Channel>,

    /// Configuration
    config: RegistrationConfig,

    /// Shard identity
    identity: ShardIdentity,

    /// Assigned shard ID (from coordinator)
    shard_id: Option<u32>,

    /// Assigned hash range (from coordinator)
    hash_range: Option<(u64, u64)>,

    /// System metrics collector
    system_metrics: SystemMetricsCollector,

    /// Checkpoint tracker
    checkpoint_tracker: Arc<CheckpointTracker>,

    /// Instruction handler
    instruction_handler: Arc<InstructionHandler>,
}

impl RegistrationClient {
    /// Create a new registration client
    pub async fn new(
        coordinator_url: String,
        heartbeat_interval: Duration,
        shard_address: String,
        identity: ShardIdentity,
        store: Arc<oxigraph::store::Store>,
        is_shutting_down: Arc<AtomicBool>,
        is_readonly: Arc<AtomicBool>,
    ) -> Result<Self> {
        info!("Connecting to coordinator at {}", coordinator_url);

        let client = CoordinatorServiceClient::connect(coordinator_url.clone())
            .await
            .with_context(|| format!("Failed to connect to coordinator at {}", coordinator_url))?;

        info!("Successfully connected to coordinator");

        // Initialize system metrics collector
        let system_metrics = SystemMetricsCollector::new(&identity.data_path);

        // Collect initial metrics
        let initial_metrics = system_metrics.collect()
            .context("Failed to collect initial system metrics")?;

        info!("System resources detected:");
        info!("  Memory: {} MB total, {} MB available",
            initial_metrics.total_memory_mb, initial_metrics.available_memory_mb);
        info!("  Disk: {} MB total, {} MB available",
            initial_metrics.total_disk_mb, initial_metrics.available_disk_mb);
        info!("  CPU cores: {}", initial_metrics.cpu_cores);

        // Initialize checkpoint tracker
        let checkpoint_tracker = Arc::new(
            CheckpointTracker::new(&identity.data_path)
                .context("Failed to initialize checkpoint tracker")?
        );

        // Initialize instruction handler with full shard state access
        let instruction_handler = Arc::new(InstructionHandler::with_shard_state(
            store,
            is_shutting_down.clone(),
            is_readonly.clone(),
        ));

        let config = RegistrationConfig {
            coordinator_url,
            heartbeat_interval,
            shard_address,
            max_memory_mb: initial_metrics.total_memory_mb,
            disk_space_mb: initial_metrics.total_disk_mb,
            cpu_cores: initial_metrics.cpu_cores,
        };

        Ok(Self {
            client,
            config,
            identity,
            shard_id: None,
            hash_range: None,
            system_metrics,
            checkpoint_tracker,
            instruction_handler,
        })
    }

    /// Register with coordinator
    pub async fn register(&mut self) -> Result<(u32, (u64, u64))> {
        info!("Registering with coordinator (machine_id: {})", self.identity.machine_id());

        // Build registration request - use reconnection flow if we have a shard_id
        let request = if let Some(existing_shard_id) = self.identity.shard_id {
            // RECONNECTION FLOW - shard restarting
            info!("Reconnecting as existing shard (shard_id: {})", existing_shard_id);

            tonic::Request::new(ShardRegistrationRequest {
                registration_type: Some(
                    shard_registration_request::RegistrationType::Reconnect(ExistingShardReconnection {
                        shard_id: existing_shard_id,
                        machine_id: self.identity.machine_id().to_string(),
                        address: self.config.shard_address.clone(),
                        last_checkpoint: self.checkpoint_tracker.get_latest_position(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    }),
                ),
                auth_token: String::new(),
            })
        } else {
            // NEW REGISTRATION FLOW - first time startup
            info!("Registering as new shard");

            // Build capabilities
            let capabilities = ShardCapabilities {
                max_memory_mb: self.config.max_memory_mb,
                disk_space_mb: self.config.disk_space_mb,
                cpu_cores: self.config.cpu_cores,
                supports_compression: true,
                supports_encryption: false,
                supported_indexes: vec!["text".to_string()],
                network_bandwidth_mbps: 1000,
            };

            tonic::Request::new(ShardRegistrationRequest {
                registration_type: Some(
                    shard_registration_request::RegistrationType::NewShard(NewShardRegistration {
                        machine_id: self.identity.machine_id().to_string(),
                        address: self.config.shard_address.clone(),
                        capabilities: Some(capabilities),
                        data_path: self.identity.data_path.to_string_lossy().to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        preferred_shard_id: 0,
                    }),
                ),
                auth_token: String::new(),
            })
        };

        // Send registration request
        let response = self.client.register_shard(request).await
            .context("Registration request failed")?
            .into_inner();

        // Extract shard ID and hash range
        let shard_id = response.assigned_shard_id;
        let hash_range = response.hash_range
            .ok_or_else(|| anyhow::anyhow!("Coordinator did not assign hash range"))?;

        let hash_range_tuple = (hash_range.start, hash_range.end);

        info!(
            "Registration successful! Assigned shard_id={}, hash_range={:?}",
            shard_id, hash_range_tuple
        );

        // Store assigned values
        self.shard_id = Some(shard_id);
        self.hash_range = Some(hash_range_tuple);

        // Save shard_id to identity file (only for new registrations, not reconnections)
        if self.identity.shard_id.is_none() {
            self.identity.save_shard_id(shard_id)
                .context("Failed to save shard_id to identity file")?;
        }

        Ok((shard_id, hash_range_tuple))
    }

    /// Start heartbeat loop
    pub async fn start_heartbeat_loop(
        mut self,
        triple_count_fn: impl Fn() -> u64 + Send + 'static,
        shard_metrics: ShardMetricsCollector,
    ) -> Result<()> {
        let shard_id = self.shard_id
            .ok_or_else(|| anyhow::anyhow!("Cannot start heartbeat: shard not registered"))?;

        info!(
            "Starting heartbeat loop (interval: {:?})",
            self.config.heartbeat_interval
        );

        let mut ticker = interval(self.config.heartbeat_interval);

        loop {
            ticker.tick().await;

            // Collect system metrics
            let system_metrics = match self.system_metrics.collect() {
                Ok(metrics) => metrics,
                Err(e) => {
                    error!("Failed to collect system metrics: {}. Using defaults.", e);
                    // Continue with heartbeat using default values
                    crate::metrics::SystemMetrics {
                        total_memory_mb: self.config.max_memory_mb,
                        available_memory_mb: self.config.max_memory_mb / 2,
                        total_disk_mb: self.config.disk_space_mb,
                        available_disk_mb: self.config.disk_space_mb / 2,
                        cpu_cores: self.config.cpu_cores,
                        memory_usage_bytes: 0,
                        disk_usage_bytes: 0,
                    }
                }
            };

            // Collect shard metrics
            let shard_stats = shard_metrics.snapshot();

            // Get triple count from provided function
            let triple_count = triple_count_fn();

            // Build heartbeat request with collected metrics
            let request = tonic::Request::new(HeartbeatRequest {
                shard_id,
                machine_id: self.identity.machine_id().to_string(),
                stats: Some(ShardStatistics {
                    triple_count,
                    disk_usage_bytes: system_metrics.disk_usage_bytes,
                    memory_usage_bytes: system_metrics.memory_usage_bytes,
                    queries_processed: shard_stats.queries_processed,
                    inserts_processed: shard_stats.inserts_processed,
                    deletes_processed: shard_stats.deletes_processed,
                    p50_latency_ms: shard_stats.p50_latency_ms,
                    p95_latency_ms: shard_stats.p95_latency_ms,
                    p99_latency_ms: shard_stats.p99_latency_ms,
                    query_errors: shard_stats.query_errors,
                    insert_errors: shard_stats.insert_errors,
                    replication_lag_ms: shard_stats.replication_lag_ms,
                }),
                timestamp: chrono::Utc::now().timestamp() as u64,
                health: Some(HealthStatus {
                    status: health_status::Status::Healthy as i32,
                    message: "Operational".to_string(),
                    component_health: std::collections::HashMap::new(),
                }),
            });

            // Send heartbeat
            match self.client.heartbeat(request).await {
                Ok(response) => {
                    let resp = response.into_inner();

                    if resp.acknowledged {
                        // Heartbeat acknowledged
                    }

                    // Process instructions using the instruction handler
                    if !resp.instructions.is_empty() {
                        let handler = self.instruction_handler.clone();
                        let instructions = resp.instructions;

                        // Spawn task to handle instructions asynchronously
                        tokio::spawn(async move {
                            if let Err(e) = handler.handle_instructions(instructions).await {
                                error!("Failed to process coordinator instructions: {}", e);
                            }
                        });
                    }
                }
                Err(e) => {
                    error!("Heartbeat failed: {}. Will retry on next interval.", e);
                    // Continue the loop - will retry on next tick
                }
            }
        }
    }

    /// Get assigned shard ID
    pub fn shard_id(&self) -> Option<u32> {
        self.shard_id
    }

    /// Get assigned hash range
    pub fn hash_range(&self) -> Option<(u64, u64)> {
        self.hash_range
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_config() {
        let config = RegistrationConfig {
            coordinator_url: "http://localhost:9091".to_string(),
            heartbeat_interval: Duration::from_secs(30),
            shard_address: "localhost:9100".to_string(),
            max_memory_mb: 8192,
            disk_space_mb: 500_000,
            cpu_cores: 8,
        };

        assert_eq!(config.coordinator_url, "http://localhost:9091");
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    }
}
