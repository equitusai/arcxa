//! Raft manager for distributed consensus.
//!
//! Manages a Raft node and provides an API for transaction ID allocation
//! through distributed consensus.
//!
//! ## Implementation Phases
//!
//! - **Week 1**: Basic structure and types
//! - **Week 2**: Proposal submission and waiting
//! - **Week 3-4**: Raft tick loop and multi-node support

use super::config::RaftConfig;
use super::proposal::TransactionProposal;
use super::storage::RaftStorage;
use raft::{Config, RawNode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Manages Raft consensus for transaction ordering.
///
/// Replaces the single-node AtomicU64 with distributed consensus.
pub struct RaftManager {
    /// The Raft node
    #[allow(dead_code)] // Will be used in Week 3-4
    node: RawNode<RaftStorage>,

    /// Peer addresses (node_id -> address)
    #[allow(dead_code)] // Will be used in Week 2
    peers: HashMap<u64, String>,

    /// Channel for submitting proposals
    #[allow(dead_code)] // Will be used in Week 2
    proposals: mpsc::Sender<TransactionProposal>,

    /// Channel for receiving committed transaction IDs
    #[allow(dead_code)] // Will be used in Week 2
    committed: mpsc::Receiver<u64>,
}

impl RaftManager {
    /// Create a new Raft manager.
    ///
    /// # Arguments
    ///
    /// * `config` - Raft configuration
    /// * `peers` - Map of peer node IDs to addresses
    ///
    /// # Errors
    ///
    /// Returns an error if Raft initialization fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use graphica_coordinator::bitemporal::consensus::{RaftManager, RaftConfig};
    /// use std::collections::HashMap;
    ///
    /// let config = RaftConfig::new(1, ":memory:".to_string());
    /// let peers = HashMap::new();
    /// let manager = RaftManager::new(config, peers).unwrap();
    /// ```
    pub fn new(
        config: RaftConfig,
        peers: HashMap<u64, String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Validate configuration
        config.validate()?;

        // Create Raft configuration
        let raft_config = Config {
            id: config.node_id,
            election_tick: config.election_tick,
            heartbeat_tick: config.heartbeat_tick,
            max_inflight_msgs: config.max_inflight_msgs,
            ..Default::default()
        };

        // Create storage
        let storage = RaftStorage::new(&config.storage_path)?;

        // Create logger (using noop logger for now)
        let logger = slog::Logger::root(slog::Discard, slog::o!());

        // Create Raft node
        let node = RawNode::new(&raft_config, storage, &logger)?;

        // Create channels for proposal submission and commits
        let (proposal_tx, _proposal_rx) = mpsc::channel(1000);
        let (_commit_tx, commit_rx) = mpsc::channel(1000);

        Ok(Self {
            node,
            peers,
            proposals: proposal_tx,
            committed: commit_rx,
        })
    }

    /// Propose a new transaction ID allocation.
    ///
    /// Returns when the proposal is committed by Raft quorum.
    ///
    /// TODO (Week 2): Implement proposal submission
    ///
    /// # Errors
    ///
    /// Returns an error if the proposal fails.
    pub async fn propose_transaction(
        &self,
        _proposal: TransactionProposal,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        // Week 2 implementation:
        // 1. Serialize proposal to bytes
        // 2. Submit to Raft via self.node.propose()
        // 3. Wait for commit via self.committed channel
        // 4. Return committed transaction ID

        // Placeholder for Week 1
        Err("Not implemented: Week 2".into())
    }

    /// Start the Raft event loop.
    ///
    /// This runs the Raft tick loop and handles incoming messages.
    ///
    /// TODO (Week 3-4): Implement Raft tick loop
    ///
    /// # Errors
    ///
    /// Returns an error if the event loop fails.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Week 3-4 implementation:
        // 1. Set up tick interval timer
        // 2. Loop:
        //    - Call self.node.tick()
        //    - Process ready messages
        //    - Handle incoming Raft messages from peers
        //    - Apply committed entries
        //    - Send heartbeats

        // Placeholder for Week 1
        Err("Not implemented: Week 3-4".into())
    }

    /// Get the current Raft node ID.
    pub fn node_id(&self) -> u64 {
        self.node.raft.id
    }

    /// Check if this node is the Raft leader.
    ///
    /// TODO (Week 3): Implement leader check
    #[allow(dead_code)]
    pub fn is_leader(&self) -> bool {
        // Week 3 implementation: Check self.node.raft.state == StateRole::Leader
        false
    }
}

// RaftManager is Send + Sync for use across threads
// Safety: RawNode is Send + Sync, channels are Send + Sync
unsafe impl Send for RaftManager {}
unsafe impl Sync for RaftManager {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_manager_creation() {
        let config = RaftConfig::new(1, ":memory:".to_string());
        let peers = HashMap::new();

        let manager = RaftManager::new(config, peers);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_raft_manager_node_id() {
        let config = RaftConfig::new(42, ":memory:".to_string());
        let peers = HashMap::new();

        let manager = RaftManager::new(config, peers).unwrap();
        assert_eq!(manager.node_id(), 42);
    }

    #[test]
    fn test_raft_manager_with_peers() {
        let config = RaftConfig::new(1, ":memory:".to_string());
        let mut peers = HashMap::new();
        peers.insert(2, "localhost:9001".to_string());
        peers.insert(3, "localhost:9002".to_string());

        let manager = RaftManager::new(config, peers);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_raft_manager_invalid_config() {
        let config = RaftConfig {
            node_id: 0, // Invalid: must be > 0
            ..Default::default()
        };
        let peers = HashMap::new();

        let manager = RaftManager::new(config, peers);
        assert!(manager.is_err());
    }

    #[tokio::test]
    async fn test_propose_transaction_not_implemented() {
        let config = RaftConfig::new(1, ":memory:".to_string());
        let peers = HashMap::new();

        let manager = RaftManager::new(config, peers).unwrap();

        let proposal = TransactionProposal::allocate_id();
        let result = manager.propose_transaction(proposal).await;

        // Should return "not implemented" error for Week 1
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not implemented"));
    }

    #[tokio::test]
    async fn test_run_not_implemented() {
        let config = RaftConfig::new(1, ":memory:".to_string());
        let peers = HashMap::new();

        let mut manager = RaftManager::new(config, peers).unwrap();

        let result = manager.run().await;

        // Should return "not implemented" error for Week 1
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not implemented"));
    }

    // TODO (Week 2): Add tests for proposal submission
    // #[tokio::test]
    // async fn test_single_node_proposal() { ... }

    // TODO (Week 3-4): Add tests for multi-node consensus
    // #[tokio::test]
    // async fn test_three_node_consensus() { ... }

    // #[tokio::test]
    // async fn test_leader_election() { ... }
}
