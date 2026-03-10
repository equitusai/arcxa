//! Shard Metadata Types for Horizontal RDF Scaling
//!
//! This module defines the core metadata types for distributed RDF storage:
//! - Shard identification and status
//! - Hash range assignment
//! - Cluster topology
//! - Replication configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Unique identifier for a shard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ShardId(pub u32);

impl fmt::Display for ShardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shard-{}", self.0)
    }
}

impl From<u32> for ShardId {
    fn from(id: u32) -> Self {
        ShardId(id)
    }
}

impl From<ShardId> for u32 {
    fn from(shard_id: ShardId) -> Self {
        shard_id.0
    }
}

/// Hash range assigned to a shard (consistent hashing)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashRange {
    /// Start of hash range (inclusive)
    pub start: u64,
    /// End of hash range (exclusive)
    pub end: u64,
}

impl HashRange {
    /// Create a new hash range
    pub fn new(start: u64, end: u64) -> Self {
        HashRange { start, end }
    }

    /// Check if a hash value falls within this range
    pub fn contains(&self, hash: u64) -> bool {
        hash >= self.start && hash < self.end
    }

    /// Get the size of this hash range
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// Calculate hash ranges for N shards (equal distribution)
    pub fn distribute(num_shards: u32) -> Vec<HashRange> {
        let range_size = u64::MAX / num_shards as u64;
        (0..num_shards)
            .map(|i| {
                let start = (i as u64) * range_size;
                let end = if i == num_shards - 1 {
                    u64::MAX
                } else {
                    (i as u64 + 1) * range_size
                };
                HashRange { start, end }
            })
            .collect()
    }
}

/// Operational status of a shard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShardStatus {
    /// Shard is healthy and serving requests
    Active,
    /// Shard is receiving data but not serving reads (migration in progress)
    Draining,
    /// Shard is being provisioned
    Provisioning,
    /// Shard is degraded (some replicas down)
    Degraded,
    /// Shard is completely unavailable
    Down,
}

impl fmt::Display for ShardStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShardStatus::Active => write!(f, "Active"),
            ShardStatus::Draining => write!(f, "Draining"),
            ShardStatus::Provisioning => write!(f, "Provisioning"),
            ShardStatus::Degraded => write!(f, "Degraded"),
            ShardStatus::Down => write!(f, "Down"),
        }
    }
}

/// Metadata for a single shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMetadata {
    /// Unique shard identifier
    pub shard_id: ShardId,

    /// Hash range this shard is responsible for
    pub hash_range: HashRange,

    /// Operational status
    pub status: ShardStatus,

    /// Leader node address (for Raft)
    pub leader_address: String,

    /// Replica node addresses
    pub replica_addresses: Vec<String>,

    /// Approximate number of triples in this shard
    pub triple_count: u64,

    /// Approximate size in bytes
    pub size_bytes: u64,

    /// Last heartbeat timestamp (Unix timestamp)
    pub last_heartbeat: u64,

    /// Raft term (for consensus)
    pub raft_term: u64,

    /// Created timestamp
    pub created_at: u64,

    /// Last updated timestamp
    pub updated_at: u64,
}

impl ShardMetadata {
    /// Create new shard metadata
    pub fn new(
        shard_id: ShardId,
        hash_range: HashRange,
        leader_address: String,
        replica_addresses: Vec<String>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        ShardMetadata {
            shard_id,
            hash_range,
            status: ShardStatus::Provisioning,
            leader_address,
            replica_addresses,
            triple_count: 0,
            size_bytes: 0,
            last_heartbeat: now,
            raft_term: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update heartbeat timestamp
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.updated_at = self.last_heartbeat;
    }

    /// Check if shard is healthy (heartbeat within threshold)
    pub fn is_healthy(&self, heartbeat_timeout_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now - self.last_heartbeat < heartbeat_timeout_secs
    }

    /// Update shard statistics
    pub fn update_stats(&mut self, triple_count: u64, size_bytes: u64) {
        self.triple_count = triple_count;
        self.size_bytes = size_bytes;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Get all node addresses (leader + replicas)
    pub fn all_addresses(&self) -> Vec<&str> {
        let mut addresses = vec![self.leader_address.as_str()];
        addresses.extend(self.replica_addresses.iter().map(|s| s.as_str()));
        addresses
    }
}

/// Complete cluster topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterTopology {
    /// Total number of shards
    pub total_shards: u32,

    /// Replication factor (how many copies of each shard)
    pub replication_factor: u32,

    /// Cluster version (incremented on topology changes)
    pub cluster_version: u64,

    /// Shard metadata indexed by shard ID
    pub shards: HashMap<ShardId, ShardMetadata>,

    /// Total triples across all shards
    pub total_triples: u64,

    /// Total size in bytes across all shards
    pub total_size_bytes: u64,

    /// Last topology update timestamp
    pub updated_at: u64,
}

impl ClusterTopology {
    /// Create a new empty cluster topology
    pub fn new(replication_factor: u32) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        ClusterTopology {
            total_shards: 0,
            replication_factor,
            cluster_version: 0,
            shards: HashMap::new(),
            total_triples: 0,
            total_size_bytes: 0,
            updated_at: now,
        }
    }

    /// Add a shard to the topology
    pub fn add_shard(&mut self, shard: ShardMetadata) {
        self.shards.insert(shard.shard_id, shard);
        self.total_shards = self.shards.len() as u32;
        self.cluster_version += 1;
        self.update_timestamp();
        self.recalculate_totals();
    }

    /// Remove a shard from the topology
    pub fn remove_shard(&mut self, shard_id: ShardId) -> Option<ShardMetadata> {
        let removed = self.shards.remove(&shard_id);
        if removed.is_some() {
            self.total_shards = self.shards.len() as u32;
            self.cluster_version += 1;
            self.update_timestamp();
            self.recalculate_totals();
        }
        removed
    }

    /// Get shard metadata by ID
    pub fn get_shard(&self, shard_id: ShardId) -> Option<&ShardMetadata> {
        self.shards.get(&shard_id)
    }

    /// Get mutable shard metadata by ID
    pub fn get_shard_mut(&mut self, shard_id: ShardId) -> Option<&mut ShardMetadata> {
        self.shards.get_mut(&shard_id)
    }

    /// Find which shard is responsible for a given hash
    pub fn find_shard_for_hash(&self, hash: u64) -> Option<&ShardMetadata> {
        self.shards
            .values()
            .find(|shard| shard.hash_range.contains(hash))
    }

    /// Get all active shards
    pub fn active_shards(&self) -> Vec<&ShardMetadata> {
        self.shards
            .values()
            .filter(|s| s.status == ShardStatus::Active)
            .collect()
    }

    /// Get all healthy shards
    pub fn healthy_shards(&self, heartbeat_timeout_secs: u64) -> Vec<&ShardMetadata> {
        self.shards
            .values()
            .filter(|s| s.is_healthy(heartbeat_timeout_secs))
            .collect()
    }

    /// Count shards by status
    pub fn count_by_status(&self) -> HashMap<ShardStatus, usize> {
        let mut counts = HashMap::new();
        for shard in self.shards.values() {
            *counts.entry(shard.status).or_insert(0) += 1;
        }
        counts
    }

    /// Update timestamp
    fn update_timestamp(&mut self) {
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Recalculate total statistics
    pub fn recalculate_totals(&mut self) {
        self.total_triples = self.shards.values().map(|s| s.triple_count).sum();
        self.total_size_bytes = self.shards.values().map(|s| s.size_bytes).sum();
    }

    /// Check overall cluster health
    pub fn is_healthy(&self, heartbeat_timeout_secs: u64) -> bool {
        if self.shards.is_empty() {
            return false;
        }

        // Cluster is healthy if all shards are healthy
        self.shards
            .values()
            .all(|s| s.is_healthy(heartbeat_timeout_secs))
    }

    /// Get cluster health percentage
    pub fn health_percentage(&self, heartbeat_timeout_secs: u64) -> f64 {
        if self.shards.is_empty() {
            return 0.0;
        }

        let healthy_count = self
            .shards
            .values()
            .filter(|s| s.is_healthy(heartbeat_timeout_secs))
            .count();

        (healthy_count as f64 / self.shards.len() as f64) * 100.0
    }
}

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Replication factor (number of replicas)
    pub replication_factor: u32,

    /// Synchronous replication (wait for all replicas)
    pub sync_replication: bool,

    /// Maximum replication lag in milliseconds (for async replication)
    pub max_replication_lag_ms: u64,

    /// Raft election timeout in milliseconds
    pub raft_election_timeout_ms: u64,

    /// Raft heartbeat interval in milliseconds
    pub raft_heartbeat_interval_ms: u64,

    /// Enable automatic failover
    pub enable_auto_failover: bool,

    /// Failover timeout in seconds
    pub failover_timeout_secs: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        ReplicationConfig {
            replication_factor: 3,
            sync_replication: false,
            max_replication_lag_ms: 100,
            raft_election_timeout_ms: 5000,
            raft_heartbeat_interval_ms: 1000,
            enable_auto_failover: true,
            failover_timeout_secs: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_id_display() {
        let shard_id = ShardId(42);
        assert_eq!(shard_id.to_string(), "shard-42");
    }

    #[test]
    fn test_hash_range_contains() {
        let range = HashRange::new(1000, 2000);
        assert!(range.contains(1000));
        assert!(range.contains(1500));
        assert!(!range.contains(2000));
        assert!(!range.contains(500));
    }

    #[test]
    fn test_hash_range_distribute() {
        let ranges = HashRange::distribute(10);
        assert_eq!(ranges.len(), 10);

        // Check continuity (no gaps)
        for i in 0..ranges.len() - 1 {
            assert_eq!(ranges[i].end, ranges[i + 1].start);
        }

        // Check coverage (first starts at 0, last ends at MAX)
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[9].end, u64::MAX);
    }

    #[test]
    fn test_shard_metadata_creation() {
        let shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec!["localhost:9091".to_string(), "localhost:9092".to_string()],
        );

        assert_eq!(shard.shard_id, ShardId(1));
        assert_eq!(shard.status, ShardStatus::Provisioning);
        assert_eq!(shard.triple_count, 0);
        assert!(shard.is_healthy(60));
    }

    #[test]
    fn test_shard_heartbeat() {
        let mut shard = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        );

        let old_heartbeat = shard.last_heartbeat;
        std::thread::sleep(std::time::Duration::from_millis(1100)); // Sleep >1 second for timestamp to change
        shard.heartbeat();
        assert!(shard.last_heartbeat > old_heartbeat);
    }

    #[test]
    fn test_cluster_topology_add_remove() {
        let mut topology = ClusterTopology::new(3);
        assert_eq!(topology.total_shards, 0);

        let shard1 = ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        );

        topology.add_shard(shard1);
        assert_eq!(topology.total_shards, 1);
        assert_eq!(topology.cluster_version, 1);

        topology.remove_shard(ShardId(1));
        assert_eq!(topology.total_shards, 0);
        assert_eq!(topology.cluster_version, 2);
    }

    #[test]
    fn test_find_shard_for_hash() {
        let mut topology = ClusterTopology::new(3);

        topology.add_shard(ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        ));

        topology.add_shard(ShardMetadata::new(
            ShardId(2),
            HashRange::new(1000, 2000),
            "localhost:9091".to_string(),
            vec![],
        ));

        let shard = topology.find_shard_for_hash(500);
        assert!(shard.is_some());
        assert_eq!(shard.unwrap().shard_id, ShardId(1));

        let shard = topology.find_shard_for_hash(1500);
        assert!(shard.is_some());
        assert_eq!(shard.unwrap().shard_id, ShardId(2));
    }

    #[test]
    fn test_cluster_health() {
        let mut topology = ClusterTopology::new(3);

        topology.add_shard(ShardMetadata::new(
            ShardId(1),
            HashRange::new(0, 1000),
            "localhost:9090".to_string(),
            vec![],
        ));

        assert!(topology.is_healthy(60));
        assert_eq!(topology.health_percentage(60), 100.0);
    }
}
