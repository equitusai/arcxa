//! Raft-based Coordinator Implementation
//!
//! This module implements the Raft consensus protocol for coordinator
//! high availability, including leader election, log replication,
//! and cluster membership management.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use raft::{Config, RawNode, Ready, StateRole, Storage};
use slog::{o, Logger};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};

use super::state_machine::{CoordinatorStateMachine, StateCommand};
use crate::bitemporal::consensus::config::RaftConfig;
use crate::bitemporal::consensus::storage::RaftStorage;

/// Coordinator role in the Raft cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorRole {
    /// This node is the leader
    Leader,
    /// This node is a follower
    Follower,
    /// This node is a candidate (during elections)
    Candidate,
    /// This node is a learner (non-voting member)
    Learner,
}

impl From<StateRole> for CoordinatorRole {
    fn from(role: StateRole) -> Self {
        match role {
            StateRole::Leader => CoordinatorRole::Leader,
            StateRole::Follower => CoordinatorRole::Follower,
            StateRole::Candidate => CoordinatorRole::Candidate,
            StateRole::PreCandidate => CoordinatorRole::Candidate,
        }
    }
}

/// Message types for coordinator communication
#[derive(Debug)]
pub enum CoordinatorMessage {
    /// Raft protocol message
    Raft(raft::prelude::Message),
    /// State machine command
    Command(StateCommand),
    /// Client request (with response channel)
    ClientRequest {
        command: StateCommand,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<u8>>>,
    },
    /// Tick the Raft state machine
    Tick,
    /// Get current status
    GetStatus(tokio::sync::oneshot::Sender<CoordinatorStatus>),
}

/// Coordinator status information
#[derive(Debug, Clone)]
pub struct CoordinatorStatus {
    pub node_id: u64,
    pub role: CoordinatorRole,
    pub term: u64,
    pub leader_id: Option<u64>,
    pub cluster_size: usize,
    pub last_heartbeat: Instant,
    pub is_healthy: bool,
}

/// Network layer abstraction for sending Raft messages
pub type NetworkCallback = Arc<dyn Fn(u64, raft::prelude::Message) -> Result<()> + Send + Sync>;

/// Raft-based high availability coordinator
pub struct RaftCoordinator {
    /// Raft node ID
    node_id: u64,

    /// Raft node
    raw_node: RawNode<RaftStorage>,

    /// State machine
    state_machine: Arc<RwLock<CoordinatorStateMachine>>,

    /// Peer addresses (node_id -> address)
    peers: HashMap<u64, String>,

    /// Message receiver
    message_rx: UnboundedReceiver<CoordinatorMessage>,

    /// Message sender (for cloning)
    message_tx: UnboundedSender<CoordinatorMessage>,

    /// Network connections to peers
    peer_connections: Arc<RwLock<HashMap<u64, PeerConnection>>>,

    /// Last heartbeat time
    last_heartbeat: Arc<RwLock<Instant>>,

    /// Slog logger
    logger: Logger,

    /// Tick interval
    tick_interval: Duration,

    /// Optional network callback for message routing (primarily for testing)
    network_callback: Option<NetworkCallback>,
}

/// Connection to a peer coordinator
struct PeerConnection {
    address: String,
    client: Option<tonic::transport::Channel>,
    last_attempt: Instant,
    consecutive_failures: u32,
}

impl RaftCoordinator {
    /// Create a new Raft coordinator
    pub fn new(
        config: RaftConfig,
        peers: HashMap<u64, String>,
        state_machine: CoordinatorStateMachine,
    ) -> Result<Self> {
        // Validate config
        config
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid Raft config: {}", e))?;

        // Create Raft configuration
        let raft_config = Config {
            id: config.node_id,
            election_tick: config.election_tick,
            heartbeat_tick: config.heartbeat_tick,
            max_inflight_msgs: config.max_inflight_msgs,
            max_size_per_msg: 1024 * 1024, // 1MB
            ..Default::default()
        };

        // Create storage
        let mut storage =
            RaftStorage::new(&config.storage_path).context("Failed to create Raft storage")?;

        // Initialize storage with all peers (including self)
        let mut all_peers = vec![config.node_id];
        all_peers.extend(peers.keys().copied());
        storage
            .initialize_peers(all_peers)
            .context("Failed to initialize peers in storage")?;

        // Create logger
        let logger = Logger::root(slog::Discard, o!("node_id" => config.node_id));

        // Create Raft node
        let raw_node =
            RawNode::new(&raft_config, storage, &logger).context("Failed to create Raft node")?;

        // Create message channels
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        // Initialize peer connections
        let mut peer_connections = HashMap::new();
        for (&peer_id, address) in &peers {
            peer_connections.insert(
                peer_id,
                PeerConnection {
                    address: address.clone(),
                    client: None,
                    last_attempt: Instant::now(),
                    consecutive_failures: 0,
                },
            );
        }

        Ok(RaftCoordinator {
            node_id: config.node_id,
            raw_node,
            state_machine: Arc::new(RwLock::new(state_machine)),
            peers,
            message_rx,
            message_tx,
            peer_connections: Arc::new(RwLock::new(peer_connections)),
            last_heartbeat: Arc::new(RwLock::new(Instant::now())),
            logger,
            tick_interval: config.tick_interval,
            network_callback: None,
        })
    }

    /// Set the network callback for message routing (primarily for testing)
    pub fn with_network(mut self, callback: NetworkCallback) -> Self {
        self.network_callback = Some(callback);
        self
    }

    /// Get a handle for sending messages to this coordinator
    pub fn get_sender(&self) -> UnboundedSender<CoordinatorMessage> {
        self.message_tx.clone()
    }

    /// Get the current role of this coordinator
    pub fn role(&self) -> CoordinatorRole {
        self.raw_node.raft.state.into()
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> bool {
        self.role() == CoordinatorRole::Leader
    }

    /// Get the current leader ID
    pub fn leader_id(&self) -> Option<u64> {
        let leader_id = self.raw_node.raft.leader_id;
        if leader_id == raft::INVALID_ID {
            None
        } else {
            Some(leader_id)
        }
    }

    /// Get current status
    pub fn status(&self) -> CoordinatorStatus {
        let last_heartbeat = *self.last_heartbeat.read().unwrap();

        CoordinatorStatus {
            node_id: self.node_id,
            role: self.role(),
            term: self.raw_node.raft.term,
            leader_id: self.leader_id(),
            cluster_size: self.peers.len() + 1,
            last_heartbeat,
            is_healthy: last_heartbeat.elapsed() < Duration::from_secs(5),
        }
    }

    /// Run the coordinator event loop
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Raft coordinator node {}", self.node_id);

        // Create tick interval
        let mut tick_timer = interval(self.tick_interval);
        tick_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // For a new cluster, nodes will naturally elect a leader through election timeouts
        // No explicit bootstrap needed - Raft handles this automatically

        loop {
            tokio::select! {
                // Handle tick timer
                _ = tick_timer.tick() => {
                    self.raw_node.tick();
                }

                // Handle incoming messages
                Some(msg) = self.message_rx.recv() => {
                    match msg {
                        CoordinatorMessage::Raft(raft_msg) => {
                            self.handle_raft_message(raft_msg)?;
                        }
                        CoordinatorMessage::Command(cmd) => {
                            self.handle_state_command(cmd).await?;
                        }
                        CoordinatorMessage::ClientRequest { command, response_tx } => {
                            self.handle_client_request(command, response_tx).await;
                        }
                        CoordinatorMessage::Tick => {
                            self.raw_node.tick();
                        }
                        CoordinatorMessage::GetStatus(response_tx) => {
                            let _ = response_tx.send(self.status());
                        }
                    }
                }
            }

            // Process Raft ready state
            if self.raw_node.has_ready() {
                let ready = self.raw_node.ready();
                self.process_ready(ready).await?;
            }

            // Update heartbeat timestamp
            *self.last_heartbeat.write().unwrap() = Instant::now();
        }
    }

    /// Handle incoming Raft message
    fn handle_raft_message(&mut self, msg: raft::prelude::Message) -> Result<()> {
        debug!(
            "Received Raft message from node {}: {:?}",
            msg.from, msg.msg_type
        );
        self.raw_node
            .step(msg)
            .context("Failed to step Raft message")?;
        Ok(())
    }

    /// Handle state machine command
    async fn handle_state_command(&mut self, cmd: StateCommand) -> Result<()> {
        if !self.is_leader() {
            warn!("Received command but not leader, dropping");
            return Ok(());
        }

        // Serialize command
        let data = bincode::serialize(&cmd).context("Failed to serialize command")?;

        // Propose to Raft
        self.raw_node
            .propose(vec![], data)
            .context("Failed to propose command")?;

        Ok(())
    }

    /// Handle client request with response
    async fn handle_client_request(
        &mut self,
        command: StateCommand,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<u8>>>,
    ) {
        if !self.is_leader() {
            let _ = response_tx.send(Err(anyhow::anyhow!(
                "Not leader, current leader: {:?}",
                self.leader_id()
            )));
            return;
        }

        // Serialize command
        let data = match bincode::serialize(&command) {
            Ok(d) => d,
            Err(e) => {
                let _ = response_tx.send(Err(anyhow::anyhow!("Failed to serialize: {}", e)));
                return;
            }
        };

        // Propose to Raft
        if let Err(e) = self.raw_node.propose(vec![], data) {
            let _ = response_tx.send(Err(anyhow::anyhow!("Failed to propose: {}", e)));
            return;
        }

        // TODO: Track proposal and respond when committed
        // For now, respond immediately (not ideal for consistency)
        let _ = response_tx.send(Ok(vec![]));
    }

    /// Process Raft ready state
    async fn process_ready(&mut self, mut ready: Ready) -> Result<()> {
        // Store entries
        if !ready.entries().is_empty() {
            let storage = self.raw_node.mut_store();
            storage
                .append(&ready.entries())
                .context("Failed to append entries")?;
        }

        // Send messages to peers
        for msg in ready.take_messages() {
            self.send_raft_message(msg).await?;
        }

        // Apply committed entries to state machine
        for entry in ready.take_committed_entries() {
            if entry.data.is_empty() {
                // Skip empty entries (e.g., from leader election)
                continue;
            }

            // Deserialize and apply command
            match bincode::deserialize::<StateCommand>(&entry.data) {
                Ok(cmd) => {
                    let mut state_machine = self.state_machine.write().unwrap();
                    if let Err(e) = state_machine.apply(cmd) {
                        error!("Failed to apply command to state machine: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to deserialize command: {}", e);
                }
            }
        }

        // Advance the node
        self.raw_node.advance(ready);

        Ok(())
    }

    /// Send Raft message to peer
    async fn send_raft_message(&self, msg: raft::prelude::Message) -> Result<()> {
        let target = msg.to;

        // Skip sending to self
        if target == self.node_id {
            return Ok(());
        }

        // Use network callback if available (for testing)
        if let Some(ref callback) = self.network_callback {
            return callback(target, msg);
        }

        // Get peer address
        let address = self
            .peers
            .get(&target)
            .ok_or_else(|| anyhow::anyhow!("Unknown peer: {}", target))?
            .clone();

        // TODO: Implement actual network sending via gRPC/HTTP
        // For now, log the attempt
        debug!(
            "Would send Raft message to node {} at {}: {:?}",
            target, address, msg.msg_type
        );

        Ok(())
    }

    /// Send a message directly to this coordinator (for testing)
    pub fn send_message(&self, msg: CoordinatorMessage) -> Result<()> {
        self.message_tx
            .send(msg)
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))
    }

    /// Campaign to become leader
    pub fn campaign(&mut self) -> Result<()> {
        info!("Node {} starting leader election", self.node_id);
        self.raw_node
            .campaign()
            .context("Failed to start campaign")?;
        Ok(())
    }

    /// Step down from leadership
    pub fn step_down(&mut self) -> Result<()> {
        if self.is_leader() {
            info!("Node {} stepping down from leadership", self.node_id);
            self.raw_node
                .raft
                .become_follower(self.raw_node.raft.term, raft::INVALID_ID);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_coordinator(node_id: u64) -> (RaftCoordinator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = RaftConfig::new(node_id, temp_dir.path().to_str().unwrap().to_string());

        let mut peers = HashMap::new();
        if node_id == 1 {
            peers.insert(2, "localhost:8082".to_string());
            peers.insert(3, "localhost:8083".to_string());
        } else if node_id == 2 {
            peers.insert(1, "localhost:8081".to_string());
            peers.insert(3, "localhost:8083".to_string());
        } else {
            peers.insert(1, "localhost:8081".to_string());
            peers.insert(2, "localhost:8082".to_string());
        }

        let state_machine = CoordinatorStateMachine::new();
        let coordinator = RaftCoordinator::new(config, peers, state_machine).unwrap();

        (coordinator, temp_dir)
    }

    #[test]
    fn test_coordinator_creation() {
        let (coordinator, _temp) = create_test_coordinator(1);
        assert_eq!(coordinator.node_id, 1);
        assert_eq!(coordinator.peers.len(), 2);
    }

    #[test]
    fn test_initial_role() {
        let (coordinator, _temp) = create_test_coordinator(1);
        // Initially should be follower
        assert_eq!(coordinator.role(), CoordinatorRole::Follower);
        assert!(!coordinator.is_leader());
    }

    #[test]
    fn test_status() {
        let (coordinator, _temp) = create_test_coordinator(1);
        let status = coordinator.status();
        assert_eq!(status.node_id, 1);
        assert_eq!(status.role, CoordinatorRole::Follower);
        assert_eq!(status.cluster_size, 3);
        assert!(status.is_healthy);
    }
}
