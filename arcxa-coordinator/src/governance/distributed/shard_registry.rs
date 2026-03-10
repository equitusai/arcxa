//! Shard Registry - Persistent storage for cluster topology
//!
//! This module provides the shard registry, which maintains the authoritative
//! cluster topology with RocksDB persistence.

use super::shard_metadata::{ClusterTopology, ShardId, ShardMetadata, ShardStatus};
use anyhow::{Context, Result};
use rocksdb::{Options, DB};
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Column family names for RocksDB
const CF_SHARDS: &str = "shards";
const CF_TOPOLOGY: &str = "topology";
const CF_VERSION: &str = "version";
const CF_MACHINE_IDS: &str = "machine_ids"; // machine_id -> shard_id mapping

/// Shard registry with RocksDB persistence
pub struct ShardRegistry {
    /// In-memory cluster topology (cached)
    topology: Arc<RwLock<ClusterTopology>>,

    /// RocksDB handle for persistence
    db: Arc<DB>,

    /// Heartbeat timeout in seconds
    heartbeat_timeout_secs: u64,
}

impl ShardRegistry {
    /// Create a new shard registry with RocksDB backend
    pub fn new<P: AsRef<Path>>(
        db_path: P,
        replication_factor: u32,
        heartbeat_timeout_secs: u64,
    ) -> Result<Self> {
        // Open RocksDB with column families
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(
            &opts,
            db_path,
            vec![CF_SHARDS, CF_TOPOLOGY, CF_VERSION, CF_MACHINE_IDS],
        )
        .context("Failed to open RocksDB for shard registry")?;

        let db = Arc::new(db);

        // Try to load existing topology from disk
        let topology = Self::load_topology_from_db(&db, replication_factor)?;

        Ok(ShardRegistry {
            topology: Arc::new(RwLock::new(topology)),
            db,
            heartbeat_timeout_secs,
        })
    }

    /// Load topology from RocksDB or create new
    fn load_topology_from_db(db: &DB, replication_factor: u32) -> Result<ClusterTopology> {
        let cf = db
            .cf_handle(CF_TOPOLOGY)
            .context("Missing topology column family")?;

        match db.get_cf(cf, b"current")? {
            Some(bytes) => {
                let topology: ClusterTopology = bincode::deserialize(&bytes)
                    .context("Failed to deserialize cluster topology")?;
                tracing::info!(
                    "📊 Loaded cluster topology: {} shards, version {}",
                    topology.total_shards,
                    topology.cluster_version
                );
                Ok(topology)
            }
            None => {
                tracing::info!("🆕 Creating new cluster topology");
                Ok(ClusterTopology::new(replication_factor))
            }
        }
    }

    /// Save topology to RocksDB
    fn save_topology_to_db(&self, topology: &ClusterTopology) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_TOPOLOGY)
            .context("Missing topology column family")?;

        let bytes = bincode::serialize(topology).context("Failed to serialize topology")?;

        self.db
            .put_cf(cf, b"current", bytes)
            .context("Failed to write topology to RocksDB")?;

        Ok(())
    }

    /// Register a new shard
    pub fn register_shard(&self, shard: ShardMetadata) -> Result<()> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        // Add shard to topology
        topology.add_shard(shard.clone());

        // Persist individual shard metadata
        let cf_shards = self
            .db
            .cf_handle(CF_SHARDS)
            .context("Missing shards column family")?;

        let key = shard.shard_id.0.to_le_bytes();
        let value = bincode::serialize(&shard).context("Failed to serialize shard metadata")?;

        self.db
            .put_cf(cf_shards, key, value)
            .context("Failed to write shard to RocksDB")?;

        // Persist updated topology
        self.save_topology_to_db(&topology)?;

        tracing::info!(
            "✅ Registered shard {} at {} (version {})",
            shard.shard_id,
            shard.leader_address,
            topology.cluster_version
        );

        Ok(())
    }

    /// Register multiple shards with automatic hash range distribution
    ///
    /// Automatically calculates equal hash ranges for N shards.
    ///
    /// # Arguments
    /// * `shard_addresses` - Vector of (shard_id, leader_address, replica_addresses) tuples
    ///
    /// # Example
    /// ```ignore
    /// let shards = vec![
    ///     (0, "shard-0:9090".to_string(), vec![]),
    ///     (1, "shard-1:9090".to_string(), vec![]),
    ///     (2, "shard-2:9090".to_string(), vec![]),
    ///     (3, "shard-3:9090".to_string(), vec![]),
    /// ];
    /// registry.register_shards_auto(shards)?;
    /// // Hash ranges automatically distributed: 0-25%, 25-50%, 50-75%, 75-100%
    /// ```
    pub fn register_shards_auto(
        &self,
        shard_addresses: Vec<(u32, String, Vec<String>)>,
    ) -> Result<()> {
        let num_shards = shard_addresses.len() as u32;

        // Automatically calculate hash ranges
        let hash_ranges = super::shard_metadata::HashRange::distribute(num_shards);

        // Register each shard with its assigned hash range
        for (i, (shard_id, leader_address, replica_addresses)) in
            shard_addresses.into_iter().enumerate()
        {
            let hash_range = hash_ranges[i];

            let shard = ShardMetadata::new(
                ShardId(shard_id),
                hash_range,
                leader_address,
                replica_addresses,
            );

            self.register_shard(shard)?;

            tracing::info!(
                "📊 Auto-assigned hash range {:016x}..{:016x} to shard {}",
                hash_range.start,
                hash_range.end,
                shard_id
            );
        }

        Ok(())
    }

    /// Unregister (remove) a shard
    pub fn unregister_shard(&self, shard_id: ShardId) -> Result<Option<ShardMetadata>> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        // Remove from topology
        let removed = topology.remove_shard(shard_id);

        if let Some(ref shard) = removed {
            // Delete from RocksDB
            let cf_shards = self
                .db
                .cf_handle(CF_SHARDS)
                .context("Missing shards column family")?;

            let key = shard_id.0.to_le_bytes();
            self.db
                .delete_cf(cf_shards, key)
                .context("Failed to delete shard from RocksDB")?;

            // Persist updated topology
            self.save_topology_to_db(&topology)?;

            tracing::info!(
                "🗑️ Unregistered shard {} (version {})",
                shard_id,
                topology.cluster_version
            );
        }

        Ok(removed)
    }

    /// Update shard heartbeat
    pub fn update_heartbeat(&self, shard_id: ShardId) -> Result<()> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let shard = topology
            .get_shard_mut(shard_id)
            .context("Shard not found")?;

        shard.heartbeat();

        // Persist updated shard
        let cf_shards = self
            .db
            .cf_handle(CF_SHARDS)
            .context("Missing shards column family")?;

        let key = shard_id.0.to_le_bytes();
        let value = bincode::serialize(&*shard).context("Failed to serialize shard metadata")?;

        self.db
            .put_cf(cf_shards, key, value)
            .context("Failed to update shard in RocksDB")?;

        Ok(())
    }

    /// Update shard statistics
    pub fn update_shard_stats(
        &self,
        shard_id: ShardId,
        triple_count: u64,
        size_bytes: u64,
    ) -> Result<()> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let shard = topology
            .get_shard_mut(shard_id)
            .context("Shard not found")?;

        shard.update_stats(triple_count, size_bytes);

        // Persist updated shard
        let cf_shards = self
            .db
            .cf_handle(CF_SHARDS)
            .context("Missing shards column family")?;

        let key = shard_id.0.to_le_bytes();
        let value = bincode::serialize(&*shard).context("Failed to serialize shard metadata")?;

        self.db
            .put_cf(cf_shards, key, value)
            .context("Failed to update shard in RocksDB")?;

        // Recalculate topology totals and save
        topology.recalculate_totals();
        self.save_topology_to_db(&topology)?;

        Ok(())
    }

    /// Update shard status (for manual control in development/operations)
    pub fn update_shard_status(&self, shard_id: ShardId, status: ShardStatus) -> Result<()> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let shard = topology
            .get_shard_mut(shard_id)
            .context("Shard not found")?;

        shard.status = status;

        // Persist updated shard
        let cf_shards = self
            .db
            .cf_handle(CF_SHARDS)
            .context("Missing shards column family")?;

        let key = shard_id.0.to_le_bytes();
        let value = bincode::serialize(&*shard).context("Failed to serialize shard metadata")?;

        self.db
            .put_cf(cf_shards, key, value)
            .context("Failed to update shard in RocksDB")?;

        Ok(())
    }

    /// Get shard metadata by ID
    pub fn get_shard(&self, shard_id: ShardId) -> Result<Option<ShardMetadata>> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.get_shard(shard_id).cloned())
    }

    /// Find shard responsible for a given hash
    pub fn find_shard_for_hash(&self, hash: u64) -> Result<Option<ShardMetadata>> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.find_shard_for_hash(hash).cloned())
    }

    /// Get current cluster topology (read-only snapshot)
    pub fn get_topology(&self) -> Result<ClusterTopology> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.clone())
    }

    /// Get all active shards
    pub fn get_active_shards(&self) -> Result<Vec<ShardMetadata>> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.active_shards().into_iter().cloned().collect())
    }

    /// Get all healthy shards
    pub fn get_healthy_shards(&self) -> Result<Vec<ShardMetadata>> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology
            .healthy_shards(self.heartbeat_timeout_secs)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Check if cluster is healthy
    pub fn is_healthy(&self) -> Result<bool> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.is_healthy(self.heartbeat_timeout_secs))
    }

    /// Get cluster health percentage
    pub fn health_percentage(&self) -> Result<f64> {
        let topology = self
            .topology
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(topology.health_percentage(self.heartbeat_timeout_secs))
    }

    /// Store machine ID to shard ID mapping
    ///
    /// This allows us to detect duplicate registrations and enable shard reconnection
    /// after restarts using the same machine ID.
    pub fn store_machine_id_mapping(&self, machine_id: &str, shard_id: ShardId) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_MACHINE_IDS)
            .context("Missing machine_ids column family")?;

        let key = machine_id.as_bytes();
        let value = shard_id.0.to_le_bytes();

        self.db
            .put_cf(cf, key, value)
            .context("Failed to store machine ID mapping")?;

        tracing::debug!(
            "Stored machine ID mapping: {} -> shard {}",
            machine_id,
            shard_id
        );

        Ok(())
    }

    /// Retrieve shard ID for a given machine ID
    ///
    /// Returns None if the machine ID has never been registered.
    pub fn get_shard_id_for_machine(&self, machine_id: &str) -> Result<Option<ShardId>> {
        let cf = self
            .db
            .cf_handle(CF_MACHINE_IDS)
            .context("Missing machine_ids column family")?;

        let key = machine_id.as_bytes();

        match self.db.get_cf(cf, key)? {
            Some(bytes) => {
                if bytes.len() != 4 {
                    return Err(anyhow::anyhow!(
                        "Invalid shard ID length in database: expected 4 bytes, got {}",
                        bytes.len()
                    ));
                }

                let shard_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Ok(Some(ShardId(shard_id)))
            }
            None => Ok(None),
        }
    }

    /// Delete machine ID mapping (used during deregistration)
    pub fn delete_machine_id_mapping(&self, machine_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_MACHINE_IDS)
            .context("Missing machine_ids column family")?;

        let key = machine_id.as_bytes();

        self.db
            .delete_cf(cf, key)
            .context("Failed to delete machine ID mapping")?;

        tracing::debug!("Deleted machine ID mapping for: {}", machine_id);

        Ok(())
    }

    /// Get all machine ID mappings (for debugging/monitoring)
    pub fn get_all_machine_id_mappings(&self) -> Result<Vec<(String, ShardId)>> {
        let cf = self
            .db
            .cf_handle(CF_MACHINE_IDS)
            .context("Missing machine_ids column family")?;

        let mut mappings = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let machine_id =
                String::from_utf8(key.to_vec()).context("Invalid UTF-8 in machine ID key")?;

            if value.len() != 4 {
                tracing::warn!(
                    "Skipping invalid shard ID entry for machine {}: wrong length",
                    machine_id
                );
                continue;
            }

            let shard_id = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            mappings.push((machine_id, ShardId(shard_id)));
        }

        Ok(mappings)
    }

    /// Update shard address (for reconnection scenarios)
    pub fn update_shard_address(&self, shard_id: ShardId, new_address: String) -> Result<()> {
        let mut topology = self
            .topology
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let shard = topology
            .get_shard_mut(shard_id)
            .context("Shard not found")?;

        shard.leader_address = new_address.clone();
        shard.updated_at = chrono::Utc::now().timestamp() as u64;

        // Persist updated shard
        let cf_shards = self
            .db
            .cf_handle(CF_SHARDS)
            .context("Missing shards column family")?;

        let key = shard_id.0.to_le_bytes();
        let value = bincode::serialize(&*shard).context("Failed to serialize shard metadata")?;

        self.db
            .put_cf(cf_shards, key, value)
            .context("Failed to update shard address in RocksDB")?;

        tracing::info!("Updated shard {} address to: {}", shard_id, new_address);

        Ok(())
    }

    /// Compact RocksDB (reclaim space)
    pub fn compact(&self) -> Result<()> {
        self.db.compact_range::<&[u8], &[u8]>(None, None);
        tracing::info!("🗜️ Compacted shard registry RocksDB");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::distributed::shard_metadata::HashRange;
    use tempfile::TempDir;

    fn create_test_registry() -> (ShardRegistry, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let registry = ShardRegistry::new(temp_dir.path(), 3, 60).unwrap();
        (registry, temp_dir)
    }

    #[test]
    fn test_registry_creation() {
        let (registry, _temp) = create_test_registry();
        let topology = registry.get_topology().unwrap();
        assert_eq!(topology.total_shards, 0);
        assert_eq!(topology.replication_factor, 3);
    }

    #[test]
    fn test_register_shard() {
        let (registry, _temp) = create_test_registry();

        let shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec!["localhost:9091".to_string()],
        );

        registry.register_shard(shard).unwrap();

        let topology = registry.get_topology().unwrap();
        assert_eq!(topology.total_shards, 1);

        let retrieved = registry.get_shard(ShardId(1)).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().leader_address, "localhost:9090");
    }

    #[test]
    fn test_unregister_shard() {
        let (registry, _temp) = create_test_registry();

        let shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        );

        registry.register_shard(shard).unwrap();
        assert_eq!(registry.get_topology().unwrap().total_shards, 1);

        let removed = registry.unregister_shard(ShardId(1)).unwrap();
        assert!(removed.is_some());
        assert_eq!(registry.get_topology().unwrap().total_shards, 0);
    }

    #[test]
    fn test_heartbeat_update() {
        let (registry, _temp) = create_test_registry();

        let shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        );

        registry.register_shard(shard).unwrap();

        let old_heartbeat = registry
            .get_shard(ShardId(1))
            .unwrap()
            .unwrap()
            .last_heartbeat;

        std::thread::sleep(std::time::Duration::from_millis(1100)); // Sleep >1 second for timestamp to change
        registry.update_heartbeat(ShardId(1)).unwrap();

        let new_heartbeat = registry
            .get_shard(ShardId(1))
            .unwrap()
            .unwrap()
            .last_heartbeat;

        assert!(new_heartbeat > old_heartbeat);
    }

    #[test]
    fn test_find_shard_for_hash() {
        let (registry, _temp) = create_test_registry();

        registry
            .register_shard(ShardMetadata::new(
                ShardId(1),
                HashRange::new(0, 1000),
                "localhost:9090".to_string(),
                vec![],
            ))
            .unwrap();

        registry
            .register_shard(ShardMetadata::new(
                ShardId(2),
                HashRange::new(1000, 2000),
                "localhost:9091".to_string(),
                vec![],
            ))
            .unwrap();

        let shard = registry.find_shard_for_hash(500).unwrap();
        assert!(shard.is_some());
        assert_eq!(shard.unwrap().shard_id, ShardId(1));

        let shard = registry.find_shard_for_hash(1500).unwrap();
        assert!(shard.is_some());
        assert_eq!(shard.unwrap().shard_id, ShardId(2));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create registry and add shard
        {
            let registry = ShardRegistry::new(temp_dir.path(), 3, 60).unwrap();
            let shard = ShardMetadata::new(
                ShardId(1),
                HashRange::new(0, 1000),
                "localhost:9090".to_string(),
                vec![],
            );
            registry.register_shard(shard).unwrap();
        }

        // Reopen registry and verify shard persisted
        {
            let registry = ShardRegistry::new(temp_dir.path(), 3, 60).unwrap();
            let topology = registry.get_topology().unwrap();
            assert_eq!(topology.total_shards, 1);

            let shard = registry.get_shard(ShardId(1)).unwrap();
            assert!(shard.is_some());
            assert_eq!(shard.unwrap().leader_address, "localhost:9090");
        }
    }

    #[test]
    fn test_cluster_health() {
        let (registry, _temp) = create_test_registry();

        registry
            .register_shard(ShardMetadata::new(
                ShardId(1),
                HashRange::new(0, 1000),
                "localhost:9090".to_string(),
                vec![],
            ))
            .unwrap();

        assert!(registry.is_healthy().unwrap());
        assert_eq!(registry.health_percentage().unwrap(), 100.0);
    }

    #[test]
    fn test_machine_id_mapping() {
        let (registry, _temp) = create_test_registry();

        let machine_id = "550e8400-e29b-41d4-a716-446655440000";
        let shard_id = ShardId(42);

        // Store mapping
        registry
            .store_machine_id_mapping(machine_id, shard_id)
            .unwrap();

        // Retrieve mapping
        let retrieved = registry.get_shard_id_for_machine(machine_id).unwrap();

        assert_eq!(retrieved, Some(shard_id));

        // Non-existent machine ID should return None
        let non_existent = registry
            .get_shard_id_for_machine("non-existent-uuid")
            .unwrap();

        assert_eq!(non_existent, None);
    }

    #[test]
    fn test_machine_id_mapping_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let machine_id = "650e8400-e29b-41d4-a716-446655440001";
        let shard_id = ShardId(99);

        // Store mapping
        {
            let registry = ShardRegistry::new(temp_dir.path(), 3, 60).unwrap();
            registry
                .store_machine_id_mapping(machine_id, shard_id)
                .unwrap();
        }

        // Reopen and verify persistence
        {
            let registry = ShardRegistry::new(temp_dir.path(), 3, 60).unwrap();
            let retrieved = registry.get_shard_id_for_machine(machine_id).unwrap();

            assert_eq!(retrieved, Some(shard_id));
        }
    }

    #[test]
    fn test_delete_machine_id_mapping() {
        let (registry, _temp) = create_test_registry();

        let machine_id = "750e8400-e29b-41d4-a716-446655440002";
        let shard_id = ShardId(10);

        // Store and verify
        registry
            .store_machine_id_mapping(machine_id, shard_id)
            .unwrap();

        assert_eq!(
            registry.get_shard_id_for_machine(machine_id).unwrap(),
            Some(shard_id)
        );

        // Delete mapping
        registry.delete_machine_id_mapping(machine_id).unwrap();

        // Verify deletion
        assert_eq!(registry.get_shard_id_for_machine(machine_id).unwrap(), None);
    }

    #[test]
    fn test_get_all_machine_id_mappings() {
        let (registry, _temp) = create_test_registry();

        // Store multiple mappings
        registry
            .store_machine_id_mapping("machine-1", ShardId(1))
            .unwrap();

        registry
            .store_machine_id_mapping("machine-2", ShardId(2))
            .unwrap();

        registry
            .store_machine_id_mapping("machine-3", ShardId(3))
            .unwrap();

        // Retrieve all mappings
        let mappings = registry.get_all_machine_id_mappings().unwrap();

        assert_eq!(mappings.len(), 3);
        assert!(mappings.contains(&("machine-1".to_string(), ShardId(1))));
        assert!(mappings.contains(&("machine-2".to_string(), ShardId(2))));
        assert!(mappings.contains(&("machine-3".to_string(), ShardId(3))));
    }

    #[test]
    fn test_update_shard_address() {
        let (registry, _temp) = create_test_registry();

        let shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        );

        registry.register_shard(shard).unwrap();

        // Update address
        registry
            .update_shard_address(ShardId(1), "new-host:9091".to_string())
            .unwrap();

        // Verify update
        let updated_shard = registry.get_shard(ShardId(1)).unwrap().unwrap();
        assert_eq!(updated_shard.leader_address, "new-host:9091");
    }
}
