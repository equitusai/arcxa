//! Consistent Hashing with Virtual Nodes
//!
//! This module implements a consistent hash ring with virtual nodes
//! for improved load distribution across shards.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::governance::distributed::ShardId;

/// Default number of virtual nodes per shard
const DEFAULT_VIRTUAL_NODES: u32 = 150;

/// Virtual node on the hash ring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualNode {
    /// Hash value (position on the ring)
    pub hash: u64,
    /// Shard that owns this virtual node
    pub shard_id: ShardId,
    /// Virtual node index for this shard
    pub vnode_index: u32,
}

/// Consistent hash router with virtual nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistentHashRouter {
    /// The hash ring
    ring: ConsistentHashRing,
    /// Hasher function name (for documentation)
    hasher: String,
}

impl ConsistentHashRouter {
    /// Create a new consistent hash router
    pub fn new() -> Self {
        ConsistentHashRouter {
            ring: ConsistentHashRing::new(),
            hasher: "xxHash64".to_string(),
        }
    }

    /// Add a shard to the ring with the default number of virtual nodes
    pub fn add_shard(&mut self, shard_id: ShardId) -> Result<()> {
        self.ring.add_shard(shard_id, DEFAULT_VIRTUAL_NODES)
    }

    /// Add a shard with a specific number of virtual nodes
    pub fn add_shard_with_weight(&mut self, shard_id: ShardId, virtual_nodes: u32) -> Result<()> {
        self.ring.add_shard(shard_id, virtual_nodes)
    }

    /// Remove a shard from the ring
    pub fn remove_shard(&mut self, shard_id: ShardId) -> Result<Vec<VirtualNode>> {
        self.ring.remove_shard(shard_id)
    }

    /// Route a key to a shard
    pub fn route(&self, key: &str) -> Option<ShardId> {
        let hash = calculate_hash(key);
        self.ring.find_shard(hash)
    }

    /// Route with replication (get N shards)
    pub fn route_with_replicas(&self, key: &str, replica_count: usize) -> Vec<ShardId> {
        let hash = calculate_hash(key);
        self.ring.find_shards_with_replicas(hash, replica_count)
    }

    /// Get load distribution statistics
    pub fn get_load_distribution(&self) -> HashMap<ShardId, f64> {
        self.ring.calculate_load_distribution()
    }

    /// Get the underlying hash ring
    pub fn ring(&self) -> &ConsistentHashRing {
        &self.ring
    }
}

/// Consistent hash ring implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistentHashRing {
    /// Virtual nodes sorted by hash value
    /// BTreeMap provides O(log n) lookups and maintains sorted order
    nodes: BTreeMap<u64, VirtualNode>,

    /// Mapping from shard ID to its virtual nodes
    shard_nodes: HashMap<ShardId, Vec<VirtualNode>>,

    /// Number of virtual nodes per shard
    virtual_node_counts: HashMap<ShardId, u32>,
}

impl ConsistentHashRing {
    /// Create a new empty hash ring
    pub fn new() -> Self {
        ConsistentHashRing {
            nodes: BTreeMap::new(),
            shard_nodes: HashMap::new(),
            virtual_node_counts: HashMap::new(),
        }
    }

    /// Add a shard with specified number of virtual nodes
    pub fn add_shard(&mut self, shard_id: ShardId, virtual_nodes: u32) -> Result<()> {
        if self.shard_nodes.contains_key(&shard_id) {
            return Err(anyhow::anyhow!("Shard {} already exists in ring", shard_id));
        }

        info!(
            "Adding shard {} with {} virtual nodes to hash ring",
            shard_id, virtual_nodes
        );

        let mut vnodes = Vec::with_capacity(virtual_nodes as usize);

        for i in 0..virtual_nodes {
            // Create unique key for this virtual node
            let vnode_key = format!("shard-{}-vnode-{}", shard_id.0, i);
            let hash = calculate_hash(&vnode_key);

            let vnode = VirtualNode {
                hash,
                shard_id,
                vnode_index: i,
            };

            // Check for hash collision (extremely rare with 64-bit hash)
            if self.nodes.contains_key(&hash) {
                debug!("Hash collision detected for virtual node {}, retrying", i);
                // Retry with a different key
                let retry_key = format!("shard-{}-vnode-{}-retry", shard_id.0, i);
                let hash = calculate_hash(&retry_key);
                let vnode = VirtualNode {
                    hash,
                    shard_id,
                    vnode_index: i,
                };
                self.nodes.insert(hash, vnode);
                vnodes.push(vnode);
            } else {
                self.nodes.insert(hash, vnode);
                vnodes.push(vnode);
            }
        }

        self.shard_nodes.insert(shard_id, vnodes);
        self.virtual_node_counts.insert(shard_id, virtual_nodes);

        debug!(
            "Added {} virtual nodes for shard {}",
            virtual_nodes, shard_id
        );
        Ok(())
    }

    /// Remove a shard and all its virtual nodes
    pub fn remove_shard(&mut self, shard_id: ShardId) -> Result<Vec<VirtualNode>> {
        let vnodes = self
            .shard_nodes
            .remove(&shard_id)
            .ok_or_else(|| anyhow::anyhow!("Shard {} not found in ring", shard_id))?;

        info!(
            "Removing shard {} with {} virtual nodes from hash ring",
            shard_id,
            vnodes.len()
        );

        for vnode in &vnodes {
            self.nodes.remove(&vnode.hash);
        }

        self.virtual_node_counts.remove(&shard_id);

        Ok(vnodes)
    }

    /// Find the shard responsible for a given hash
    pub fn find_shard(&self, hash: u64) -> Option<ShardId> {
        if self.nodes.is_empty() {
            return None;
        }

        // Find the first node with hash >= target hash
        // If no such node exists, wrap around to the first node
        let vnode = self
            .nodes
            .range(hash..)
            .next()
            .or_else(|| self.nodes.iter().next())
            .map(|(_, vnode)| vnode)?;

        Some(vnode.shard_id)
    }

    /// Find N shards for replication (primary + replicas)
    pub fn find_shards_with_replicas(&self, hash: u64, replica_count: usize) -> Vec<ShardId> {
        if self.nodes.is_empty() {
            return vec![];
        }

        let mut shards = Vec::with_capacity(replica_count);
        let mut seen_shards = std::collections::HashSet::new();

        // Start from the target hash and walk clockwise
        let iter = self.nodes.range(hash..).chain(self.nodes.iter());

        for (_, vnode) in iter {
            if seen_shards.insert(vnode.shard_id) {
                shards.push(vnode.shard_id);
                if shards.len() >= replica_count {
                    break;
                }
            }
        }

        shards
    }

    /// Reassign a virtual node to a different shard (for rebalancing)
    pub fn reassign_virtual_node(&mut self, vnode_hash: u64, new_shard: ShardId) -> Result<()> {
        let vnode = self
            .nodes
            .get_mut(&vnode_hash)
            .ok_or_else(|| anyhow::anyhow!("Virtual node with hash {} not found", vnode_hash))?;

        let old_shard = vnode.shard_id;
        vnode.shard_id = new_shard;

        // Update shard_nodes mapping
        if let Some(old_vnodes) = self.shard_nodes.get_mut(&old_shard) {
            old_vnodes.retain(|v| v.hash != vnode_hash);
        }

        self.shard_nodes
            .entry(new_shard)
            .or_insert_with(Vec::new)
            .push(*vnode);

        debug!(
            "Reassigned virtual node {} from shard {} to shard {}",
            vnode_hash, old_shard, new_shard
        );

        Ok(())
    }

    /// Calculate load distribution (percentage of ring owned by each shard)
    pub fn calculate_load_distribution(&self) -> HashMap<ShardId, f64> {
        let mut distribution = HashMap::new();

        if self.nodes.is_empty() {
            return distribution;
        }

        // Count virtual nodes per shard
        for shard_id in self.shard_nodes.keys() {
            let vnode_count = self.shard_nodes[shard_id].len() as f64;
            let total_vnodes = self.nodes.len() as f64;
            let percentage = (vnode_count / total_vnodes) * 100.0;
            distribution.insert(*shard_id, percentage);
        }

        distribution
    }

    /// Get virtual nodes for a specific shard
    pub fn get_shard_vnodes(&self, shard_id: ShardId) -> Option<&[VirtualNode]> {
        self.shard_nodes.get(&shard_id).map(|v| v.as_slice())
    }

    /// Get total number of virtual nodes
    pub fn total_vnodes(&self) -> usize {
        self.nodes.len()
    }

    /// Get number of shards
    pub fn shard_count(&self) -> usize {
        self.shard_nodes.len()
    }

    /// Clear the ring
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.shard_nodes.clear();
        self.virtual_node_counts.clear();
    }

    /// Check if ring is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Calculate hash using xxHash-like algorithm
/// This is a simplified version - in production, use twox_hash crate
#[inline]
pub fn calculate_hash(key: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Calculate hash for a byte slice
#[inline]
pub fn calculate_hash_bytes(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_ring() {
        let router = ConsistentHashRouter::new();
        assert!(router.route("test").is_none());
    }

    #[test]
    fn test_add_shard() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard(ShardId(1)).unwrap();

        // Any key should route to the only shard
        assert_eq!(router.route("test").unwrap(), ShardId(1));
        assert_eq!(router.route("another").unwrap(), ShardId(1));
    }

    #[test]
    fn test_multiple_shards() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard(ShardId(1)).unwrap();
        router.add_shard(ShardId(2)).unwrap();
        router.add_shard(ShardId(3)).unwrap();

        // Different keys should potentially route to different shards
        let shard1 = router.route("key1");
        let shard2 = router.route("key2");
        let shard3 = router.route("key3");

        assert!(shard1.is_some());
        assert!(shard2.is_some());
        assert!(shard3.is_some());
    }

    #[test]
    fn test_consistent_routing() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard(ShardId(1)).unwrap();
        router.add_shard(ShardId(2)).unwrap();

        // Same key should always route to the same shard
        let key = "consistent-key";
        let shard1 = router.route(key).unwrap();
        let shard2 = router.route(key).unwrap();
        assert_eq!(shard1, shard2);
    }

    #[test]
    fn test_remove_shard() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard(ShardId(1)).unwrap();
        router.add_shard(ShardId(2)).unwrap();

        let key = "test-key";
        let original_shard = router.route(key);

        // Remove shard 1
        router.remove_shard(ShardId(1)).unwrap();

        // Key should still route somewhere
        let new_shard = router.route(key);
        assert!(new_shard.is_some());

        // If originally routed to shard 1, should now route to shard 2
        if original_shard == Some(ShardId(1)) {
            assert_eq!(new_shard, Some(ShardId(2)));
        }
    }

    #[test]
    fn test_load_distribution() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard_with_weight(ShardId(1), 100).unwrap();
        router.add_shard_with_weight(ShardId(2), 200).unwrap();

        let distribution = router.get_load_distribution();

        // Shard 2 should have roughly twice the load of shard 1
        assert!(distribution[&ShardId(2)] > distribution[&ShardId(1)]);
        assert!((distribution[&ShardId(2)] / distribution[&ShardId(1)] - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_replicas() {
        let mut router = ConsistentHashRouter::new();
        router.add_shard(ShardId(1)).unwrap();
        router.add_shard(ShardId(2)).unwrap();
        router.add_shard(ShardId(3)).unwrap();

        let replicas = router.route_with_replicas("test-key", 2);
        assert_eq!(replicas.len(), 2);

        // Should return unique shards
        assert_ne!(replicas[0], replicas[1]);
    }

    #[test]
    fn test_virtual_node_collision_handling() {
        let mut ring = ConsistentHashRing::new();

        // Add multiple shards - collision handling should work
        ring.add_shard(ShardId(1), 150).unwrap();
        ring.add_shard(ShardId(2), 150).unwrap();

        assert_eq!(ring.shard_count(), 2);
        assert_eq!(ring.total_vnodes(), 300);
    }
}
