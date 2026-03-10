//! Auto-Registration Handler for Shard Discovery
//!
//! This module implements the coordinator-side logic for automatic shard
//! registration, ID assignment, and reconnection handling.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::coordinator_proto::ShardCapabilities;
use super::shard_metadata::{HashRange, ShardId, ShardMetadata, ShardStatus};
use super::shard_registry::ShardRegistry;

/// Result of shard registration
#[derive(Debug, Clone)]
pub struct ShardRegistrationResult {
    pub shard_id: u32,
    pub hash_range: HashRange,
    pub topology_version: u64,
}

/// Auto-registration handler
pub struct AutoRegistrationHandler {
    /// Shard registry
    registry: Arc<ShardRegistry>,

    /// Machine ID to shard ID mapping (cached)
    machine_id_map: Arc<RwLock<HashMap<String, ShardId>>>,

    /// Registration locks to prevent concurrent registration of same machine
    /// Arc<Mutex> allows us to return MutexGuards without lifetime issues
    registration_locks: Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,

    /// Configuration
    config: RegistrationConfig,
}

/// Registration configuration
#[derive(Debug, Clone)]
pub struct RegistrationConfig {
    /// Maximum number of shards allowed
    pub max_shards: u32,

    /// Whether to auto-rebalance on new shard registration
    pub auto_rebalance: bool,

    /// Rebalance threshold (percentage imbalance)
    pub rebalance_threshold_percent: f64,

    /// Minimum time between rebalances (seconds)
    pub rebalance_cooldown_secs: u64,

    /// Allow registration of shards with duplicate machine IDs (for testing)
    pub allow_duplicate_machines: bool,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            max_shards: 100,
            auto_rebalance: true,
            rebalance_threshold_percent: 20.0,
            rebalance_cooldown_secs: 300, // 5 minutes
            allow_duplicate_machines: false,
        }
    }
}

impl AutoRegistrationHandler {
    /// Create new auto-registration handler
    pub fn new(registry: Arc<ShardRegistry>, config: RegistrationConfig) -> Self {
        Self {
            registry,
            machine_id_map: Arc::new(RwLock::new(HashMap::new())),
            registration_locks: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register a new shard with auto-assigned ID
    pub async fn register_new_shard(
        &self,
        machine_id: String,
        address: String,
        capabilities: ShardCapabilities,
        data_path: Option<String>,
    ) -> Result<ShardRegistrationResult> {
        // Acquire per-machine lock to prevent concurrent registrations
        let lock = self.acquire_registration_lock(&machine_id).await;
        let _guard = lock.lock().await;

        // Validate machine ID format
        self.validate_machine_id(&machine_id)?;

        // Check if machine already registered
        if !self.config.allow_duplicate_machines {
            if let Some(existing_id) = self.find_existing_shard(&machine_id).await? {
                return Err(anyhow!(
                    "Machine {} already registered as shard {}",
                    machine_id,
                    existing_id
                ));
            }
        }

        // Check shard count limit
        let topology = self.registry.get_topology()?;
        if topology.total_shards >= self.config.max_shards {
            return Err(anyhow!(
                "Maximum shard count ({}) reached",
                self.config.max_shards
            ));
        }

        // Allocate new shard ID
        let shard_id = self.allocate_shard_id().await?;

        // Calculate hash range for new shard
        let hash_range = self.calculate_hash_range_for_new_shard(&topology, shard_id)?;

        // Create shard metadata
        let metadata = ShardMetadata {
            shard_id: ShardId(shard_id),
            hash_range: hash_range.clone(),
            status: ShardStatus::Provisioning,
            leader_address: address.clone(),
            replica_addresses: Vec::new(),
            triple_count: 0,
            size_bytes: 0,
            last_heartbeat: chrono::Utc::now().timestamp() as u64,
            raft_term: 0,
            created_at: chrono::Utc::now().timestamp() as u64,
            updated_at: chrono::Utc::now().timestamp() as u64,
        };

        // Register in shard registry
        self.registry.register_shard(metadata)?;

        // Update machine ID mapping
        {
            let mut map = self.machine_id_map.write().await;
            map.insert(machine_id.clone(), ShardId(shard_id));
        }

        // Store machine ID in registry metadata
        self.store_machine_id_mapping(shard_id, &machine_id)?;

        // Rebalance hash ranges for all shards (including the new one)
        self.rebalance_hash_ranges().await?;

        // Get the updated hash range for the newly registered shard
        let updated_topology = self.registry.get_topology()?;
        let final_hash_range = updated_topology
            .get_shard(ShardId(shard_id))
            .map(|s| s.hash_range.clone())
            .unwrap_or(hash_range.clone());

        info!(
            "Successfully registered new shard: id={}, machine_id={}, address={}, range={:?}-{:?}",
            shard_id, machine_id, address, final_hash_range.start, final_hash_range.end
        );

        Ok(ShardRegistrationResult {
            shard_id,
            hash_range: final_hash_range,
            topology_version: updated_topology.cluster_version,
        })
    }

    /// Reconnect an existing shard
    pub async fn reconnect_shard(
        &self,
        shard_id: u32,
        machine_id: String,
        address: String,
        last_checkpoint: Option<u64>,
    ) -> Result<ShardRegistrationResult> {
        // Acquire per-machine lock
        let lock = self.acquire_registration_lock(&machine_id).await;
        let _guard = lock.lock().await;

        // Validate shard exists
        let shard = self
            .registry
            .get_shard(ShardId(shard_id))?
            .ok_or_else(|| anyhow!("Shard {} not found in registry", shard_id))?;

        // Verify machine ID matches
        let stored_machine_id = self.get_stored_machine_id(shard_id)?;
        if let Some(stored) = stored_machine_id {
            if stored != machine_id {
                error!(
                    "Machine ID mismatch for shard {}: stored={}, provided={}",
                    shard_id, stored, machine_id
                );
                return Err(anyhow!(
                    "Machine ID mismatch for shard {}. Expected: {}, Got: {}",
                    shard_id,
                    stored,
                    machine_id
                ));
            }
        } else {
            // No stored machine ID - this is a legacy shard, store it now
            warn!(
                "No stored machine ID for shard {} - storing {} (legacy migration)",
                shard_id, machine_id
            );
            self.store_machine_id_mapping(shard_id, &machine_id)?;
        }

        // Update shard address and status
        self.registry
            .update_shard_address(ShardId(shard_id), address.clone())?;
        self.registry
            .update_shard_status(ShardId(shard_id), ShardStatus::Active)?;
        self.registry.update_heartbeat(ShardId(shard_id))?;

        // Update machine ID mapping cache
        {
            let mut map = self.machine_id_map.write().await;
            map.insert(machine_id.clone(), ShardId(shard_id));
        }

        info!(
            "Successfully reconnected shard: id={}, machine_id={}, address={}, checkpoint={:?}",
            shard_id, machine_id, address, last_checkpoint
        );

        let topology = self.registry.get_topology()?;

        Ok(ShardRegistrationResult {
            shard_id,
            hash_range: shard.hash_range,
            topology_version: topology.cluster_version,
        })
    }

    /// Deregister a shard
    pub async fn deregister_shard(
        &self,
        shard_id: u32,
        machine_id: String,
        graceful: bool,
    ) -> Result<()> {
        // Verify machine ID matches
        let stored_machine_id = self.get_stored_machine_id(shard_id)?;
        if let Some(stored) = stored_machine_id {
            if stored != machine_id {
                return Err(anyhow!(
                    "Machine ID mismatch for deregistration of shard {}",
                    shard_id
                ));
            }
        }

        if graceful {
            // Mark shard as draining
            self.registry
                .update_shard_status(ShardId(shard_id), ShardStatus::Draining)?;

            // TODO: Initiate data migration to other shards

            info!("Initiated graceful deregistration for shard {}", shard_id);
        } else {
            // Immediate removal
            self.registry.unregister_shard(ShardId(shard_id))?;

            // Remove from machine ID mapping (both in-memory cache and RocksDB)
            {
                let mut map = self.machine_id_map.write().await;
                map.retain(|_, v| v.0 != shard_id);
            }

            // Delete machine ID mapping from RocksDB
            self.registry.delete_machine_id_mapping(&machine_id)?;

            info!("Forcefully deregistered shard {}", shard_id);
        }

        Ok(())
    }

    /// Find existing shard by machine ID
    async fn find_existing_shard(&self, machine_id: &str) -> Result<Option<ShardId>> {
        // Check cache first
        {
            let map = self.machine_id_map.read().await;
            if let Some(shard_id) = map.get(machine_id) {
                return Ok(Some(*shard_id));
            }
        }

        // Check registry
        let topology = self.registry.get_topology()?;
        for shard in topology.shards.values() {
            if let Some(stored_id) = self.get_stored_machine_id(shard.shard_id.0)? {
                if stored_id == machine_id {
                    // Update cache
                    let mut map = self.machine_id_map.write().await;
                    map.insert(machine_id.to_string(), shard.shard_id);
                    return Ok(Some(shard.shard_id));
                }
            }
        }

        Ok(None)
    }

    /// Allocate next available shard ID
    async fn allocate_shard_id(&self) -> Result<u32> {
        // This would typically use atomic increment in RocksDB
        // For now, find the highest existing ID and add 1
        let topology = self.registry.get_topology()?;

        if topology.shards.is_empty() {
            return Ok(0); // First shard gets ID 0
        }

        let max_id = topology.shards.keys().map(|id| id.0).max().unwrap(); // Safe to unwrap because we checked is_empty

        Ok(max_id + 1)
    }

    /// Calculate hash range for new shard
    fn calculate_hash_range_for_new_shard(
        &self,
        topology: &super::shard_metadata::ClusterTopology,
        shard_id: u32,
    ) -> Result<HashRange> {
        // If this is the very first shard, it owns the full space.
        if topology.shards.is_empty() {
            return Ok(HashRange::new(0, u64::MAX));
        }

        // Total shards after adding this one.
        let total_shards = (topology.total_shards as usize) + 1;
        let index = shard_id as usize;

        // Defensive: ensure this shard_id looks like the next slot.
        // (Your allocator gives max+1 so this should hold; if not, we still compute a valid slot.)
        if index >= total_shards {
            return Err(anyhow!(
                "shard_id {} not valid for computed total_shards {}",
                shard_id,
                total_shards
            ));
        }

        Ok(Self::partition_range(index, total_shards))
    }

    /// Compute an exclusive-end partition for shard `index` in `[0, total_shards)`,
    /// covering the full 64-bit space [0, u64::MAX) without gaps or overlaps.
    /// Uses u128 intermediates to avoid overflow and truncation issues.
    fn partition_range(index: usize, total_shards: usize) -> HashRange {
        assert!(total_shards > 0, "total_shards must be > 0");
        assert!(index < total_shards, "index out of range");

        // 2^64 as u128
        const TWO64: u128 = 1u128 << 64;

        let i = index as u128;
        let n = total_shards as u128;

        // Exclusive-end math:
        // start = floor(i * 2^64 / n)
        // end   = floor((i+1) * 2^64 / n)
        let start_u128 = (i * TWO64) / n;
        let end_u128 = ((i + 1) * TWO64) / n;

        // Clamp the final end to u64::MAX so it fits in u64 while preserving contiguity.
        let start = start_u128 as u64;
        let end = if end_u128 >= TWO64 {
            u64::MAX
        } else {
            end_u128 as u64
        };

        HashRange::new(start, end)
    }

    /// Check if rebalancing is needed
    fn should_rebalance(&self, topology: &super::shard_metadata::ClusterTopology) -> bool {
        if topology.shards.len() < 2 {
            return false;
        }

        // Calculate load imbalance
        let triple_counts: Vec<u64> = topology.shards.values().map(|s| s.triple_count).collect();

        if triple_counts.is_empty() {
            return false;
        }

        let max_count = *triple_counts.iter().max().unwrap();
        let min_count = *triple_counts.iter().min().unwrap();
        let avg_count = triple_counts.iter().sum::<u64>() / triple_counts.len() as u64;

        if avg_count == 0 {
            return false;
        }

        let imbalance_percent = ((max_count - min_count) as f64 / avg_count as f64) * 100.0;

        imbalance_percent > self.config.rebalance_threshold_percent
    }

    /// Schedule rebalancing operation
    async fn schedule_rebalancing(&self) {
        // TODO: Implement rebalancing scheduler
        info!("Rebalancing scheduled (not yet implemented)");
    }

    /// Rebalance hash ranges for all registered shards
    async fn rebalance_hash_ranges(&self) -> Result<()> {
        let topology = self.registry.get_topology()?;
        let shard_count = topology.total_shards as usize;

        if shard_count == 0 {
            return Ok(());
        }

        // Get shard IDs in sorted order to assign contiguous slots [0..shard_count)
        let mut shard_ids: Vec<u32> = topology.shards.keys().map(|id| id.0).collect();
        shard_ids.sort_unstable();

        for (index, &shard_id) in shard_ids.iter().enumerate() {
            let new_range = Self::partition_range(index, shard_count);

            if let Some(mut shard) = self.registry.get_shard(ShardId(shard_id))? {
                // Only update if the range actually changed
                if shard.hash_range.start != new_range.start
                    || shard.hash_range.end != new_range.end
                {
                    shard.hash_range = new_range;
                    shard.updated_at = chrono::Utc::now().timestamp() as u64;

                    // Persist
                    self.registry.register_shard(shard)?;
                    info!(
                        "Rebalanced shard {}: new range {:016x}..{:016x}",
                        shard_id, new_range.start, new_range.end
                    );
                }
            }
        }

        info!("Hash range rebalancing complete for {} shards", shard_count);
        Ok(())
    }

    /// Validate machine ID format
    fn validate_machine_id(&self, machine_id: &str) -> Result<()> {
        if machine_id.is_empty() {
            return Err(anyhow!("Machine ID cannot be empty"));
        }

        // Validate UUID format
        uuid::Uuid::parse_str(machine_id).context("Invalid machine ID format (expected UUID)")?;

        Ok(())
    }

    /// Acquire registration lock for machine
    ///
    /// Returns an Arc<Mutex> which can be locked independently without lifetime issues.
    /// This prevents concurrent registrations from the same machine ID.
    async fn acquire_registration_lock(&self, machine_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.registration_locks.write().await;
        locks
            .entry(machine_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Store machine ID mapping in registry
    fn store_machine_id_mapping(&self, shard_id: u32, machine_id: &str) -> Result<()> {
        self.registry
            .store_machine_id_mapping(machine_id, ShardId(shard_id))
    }

    /// Get stored machine ID for shard
    fn get_stored_machine_id(&self, shard_id: u32) -> Result<Option<String>> {
        // We need to iterate through all machine ID mappings to find the one for this shard
        // This is inefficient but acceptable for reconnection scenarios
        let mappings = self.registry.get_all_machine_id_mappings()?;

        for (machine_id, mapped_shard_id) in mappings {
            if mapped_shard_id.0 == shard_id {
                return Ok(Some(machine_id));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_handler() -> (AutoRegistrationHandler, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let registry = Arc::new(ShardRegistry::new(temp_dir.path(), 3, 60).unwrap());
        let config = RegistrationConfig::default();
        let handler = AutoRegistrationHandler::new(registry, config);
        (handler, temp_dir)
    }

    #[tokio::test]
    async fn test_register_new_shard() {
        let (handler, _temp) = create_test_handler().await;

        let machine_id = uuid::Uuid::new_v4().to_string();
        let result = handler
            .register_new_shard(
                machine_id.clone(),
                "localhost:9100".to_string(),
                ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 100000,
                    cpu_cores: 4,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec!["text".to_string()],
                    network_bandwidth_mbps: 1000,
                },
                Some("/data/shard".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(result.shard_id, 0);
        assert_eq!(result.hash_range.start, 0);
        assert_eq!(result.hash_range.end, u64::MAX);
    }

    #[tokio::test]
    async fn test_reconnect_shard() {
        let (handler, _temp) = create_test_handler().await;

        // Register shard first
        let machine_id = uuid::Uuid::new_v4().to_string();
        let reg_result = handler
            .register_new_shard(
                machine_id.clone(),
                "localhost:9100".to_string(),
                ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 100000,
                    cpu_cores: 4,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                },
                None,
            )
            .await
            .unwrap();

        // Reconnect with same machine ID
        let reconnect_result = handler
            .reconnect_shard(
                reg_result.shard_id,
                machine_id,
                "localhost:9101".to_string(), // Different address
                Some(12345),
            )
            .await
            .unwrap();

        assert_eq!(reconnect_result.shard_id, reg_result.shard_id);
        assert_eq!(
            reconnect_result.hash_range.start,
            reg_result.hash_range.start
        );
    }

    #[tokio::test]
    async fn test_duplicate_machine_id_rejected() {
        let (handler, _temp) = create_test_handler().await;

        let machine_id = uuid::Uuid::new_v4().to_string();

        // First registration should succeed
        handler
            .register_new_shard(
                machine_id.clone(),
                "localhost:9100".to_string(),
                ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 100000,
                    cpu_cores: 4,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                },
                None,
            )
            .await
            .unwrap();

        // Second registration with same machine ID should fail
        let result = handler
            .register_new_shard(
                machine_id.clone(),
                "localhost:9101".to_string(),
                ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 100000,
                    cpu_cores: 4,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                },
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already registered"));
    }

    #[tokio::test]
    async fn test_invalid_machine_id_format() {
        let (handler, _temp) = create_test_handler().await;

        let result = handler
            .register_new_shard(
                "not-a-uuid".to_string(),
                "localhost:9100".to_string(),
                ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 100000,
                    cpu_cores: 4,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                },
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid machine ID format"));
    }
}
