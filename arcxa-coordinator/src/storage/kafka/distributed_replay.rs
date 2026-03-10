//! Distributed Replay Coordination for High Availability
//!
//! This module provides distributed coordination for Kafka replay operations
//! in multi-coordinator deployments. It uses Raft consensus for leader election
//! to ensure only one coordinator performs recovery at a time.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │          Multi-Coordinator HA Deployment                    │
//! │                                                             │
//! │  Coordinator 1        Coordinator 2        Coordinator 3   │
//! │       │                    │                     │         │
//! │       ├───── Raft Election ────────────────────┤          │
//! │       │                    │                     │         │
//! │   [LEADER]             [FOLLOWER]            [FOLLOWER]    │
//! │       │                    │                     │         │
//! │   Replay                   │                     │         │
//! │   Manager                  │                     │         │
//! │       │                    │                     │         │
//! │   Recovery ───(heartbeat)──┼────────────────────┤          │
//! │                            │                     │         │
//! └─────────────────────────────────────────────────────────────┘
//!
//! Leader Transition:
//! ┌──────────────────────────────────────────────────────────────┐
//! │  Coordinator 1 (LEADER) ──[crash]──> Election Timeout        │
//! │                                            │                 │
//! │  Coordinator 2 ────────────────────> [NEW LEADER]            │
//! │       │                                    │                 │
//! │       └──────── Continues Replay ──────────┘                 │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::storage::kafka::{
//!     DistributedReplayCoordinator, ReplayConfig
//! };
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create coordinator with peer URLs
//! let coordinator = DistributedReplayCoordinator::new(
//!     "coordinator-1",
//!     vec![
//!         "http://coordinator-1:8080".to_string(),
//!         "http://coordinator-2:8080".to_string(),
//!         "http://coordinator-3:8080".to_string(),
//!     ],
//!     ReplayConfig::default(),
//! ).await?;
//!
//! // Start leader election
//! coordinator.start_election().await?;
//!
//! // Replay will only run if this coordinator becomes leader
//! # let wal = todo!();
//! # let sink = todo!();
//! # let ack_tracker = todo!();
//! let report = coordinator.replay_if_leader(wal, sink, ack_tracker).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::acknowledgment_tracker::AcknowledgmentTracker;
use super::durable_sink::DurableKafkaLineageSink;
use super::replay_manager::{RecoveryReport, ReplayConfig, ReplayManager};
use crate::storage::wal::WriteAheadLog;

/// Raft node state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaftState {
    /// Follower state - not performing replay
    Follower,
    /// Candidate state - election in progress
    Candidate,
    /// Leader state - performs replay
    Leader,
}

/// Raft election configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Election timeout (150-300ms recommended)
    pub election_timeout: Duration,

    /// Heartbeat interval (50ms recommended, must be < election_timeout)
    pub heartbeat_interval: Duration,

    /// Maximum number of election retries
    pub max_election_retries: u32,

    /// Request timeout for peer communication
    pub request_timeout: Duration,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            election_timeout: Duration::from_millis(300),
            heartbeat_interval: Duration::from_millis(50),
            max_election_retries: 5,
            request_timeout: Duration::from_secs(2),
        }
    }
}

/// Raft log entry for replay coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayLogEntry {
    /// Unique entry ID
    pub id: Uuid,

    /// Raft term number
    pub term: u64,

    /// Entry type
    pub entry_type: ReplayEntryType,

    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Types of replay log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplayEntryType {
    /// Recovery started
    RecoveryStarted {
        leader_id: String,
        total_events: usize,
    },

    /// Recovery progress update
    RecoveryProgress {
        replayed_events: usize,
        failed_events: usize,
    },

    /// Recovery completed
    RecoveryCompleted { report: RecoveryReportSummary },

    /// Leader election
    LeaderElected { leader_id: String },
}

/// Serializable recovery report summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryReportSummary {
    pub total_events: usize,
    pub replayed_events: usize,
    pub failed_events: usize,
    pub duration_secs: f64,
    pub success_rate: f64,
}

impl From<&RecoveryReport> for RecoveryReportSummary {
    fn from(report: &RecoveryReport) -> Self {
        Self {
            total_events: report.total_events,
            replayed_events: report.replayed_events,
            failed_events: report.failed_events,
            duration_secs: report.duration.as_secs_f64(),
            success_rate: report.success_rate(),
        }
    }
}

/// Peer coordinator information
#[derive(Debug, Clone)]
struct PeerInfo {
    /// Peer coordinator ID
    id: String,

    /// Peer URL for communication
    url: String,

    /// Last heartbeat received
    last_heartbeat: Instant,

    /// Current term (from last communication)
    term: u64,

    /// Is peer healthy?
    healthy: bool,
}

/// Distributed replay coordinator with Raft consensus
pub struct DistributedReplayCoordinator {
    /// This coordinator's ID
    id: String,

    /// Raft state
    state: Arc<RwLock<RaftState>>,

    /// Current Raft term
    term: Arc<RwLock<u64>>,

    /// Voted for in current term
    voted_for: Arc<RwLock<Option<String>>>,

    /// Peer coordinators
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,

    /// Raft log
    log: Arc<RwLock<Vec<ReplayLogEntry>>>,

    /// Raft configuration
    raft_config: RaftConfig,

    /// Replay configuration
    replay_config: ReplayConfig,

    /// Last election time
    last_election: Arc<RwLock<Instant>>,

    /// HTTP client for peer communication
    http_client: reqwest::Client,
}

impl DistributedReplayCoordinator {
    /// Create new distributed replay coordinator
    pub async fn new(
        id: impl Into<String>,
        peer_urls: Vec<String>,
        replay_config: ReplayConfig,
    ) -> Result<Self> {
        let id = id.into();
        let mut peers = HashMap::new();

        // Initialize peer info
        for url in peer_urls {
            // Extract peer ID from URL (e.g., coordinator-1 from http://coordinator-1:8080)
            let peer_id = url
                .split("://")
                .nth(1)
                .and_then(|s| s.split(':').next())
                .unwrap_or(&url)
                .to_string();

            // Skip self
            if peer_id == id {
                continue;
            }

            peers.insert(
                peer_id.clone(),
                PeerInfo {
                    id: peer_id,
                    url,
                    last_heartbeat: Instant::now(),
                    term: 0,
                    healthy: true,
                },
            );
        }

        info!(
            "Distributed replay coordinator '{}' initialized with {} peers",
            id,
            peers.len()
        );

        Ok(Self {
            id,
            state: Arc::new(RwLock::new(RaftState::Follower)),
            term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            peers: Arc::new(RwLock::new(peers)),
            log: Arc::new(RwLock::new(Vec::new())),
            raft_config: RaftConfig::default(),
            replay_config,
            last_election: Arc::new(RwLock::new(Instant::now())),
            http_client: reqwest::Client::builder()
                .timeout(RaftConfig::default().request_timeout)
                .build()?,
        })
    }

    /// Start leader election process
    pub async fn start_election(&self) -> Result<()> {
        info!(
            "Starting Raft leader election for coordinator '{}'",
            self.id
        );

        // Transition to Candidate state
        {
            let mut state = self.state.write().await;
            *state = RaftState::Candidate;
        }

        // Increment term
        let new_term = {
            let mut term = self.term.write().await;
            *term += 1;
            *term
        };

        // Vote for self
        {
            let mut voted_for = self.voted_for.write().await;
            *voted_for = Some(self.id.clone());
        }

        info!(
            "Coordinator '{}' starting election for term {}",
            self.id, new_term
        );

        // Request votes from peers
        let votes = self.request_votes(new_term).await?;

        // Check if won election (majority)
        let peers_count = self.peers.read().await.len();
        let majority = (peers_count + 1) / 2 + 1; // +1 for self

        if votes >= majority {
            info!(
                "Coordinator '{}' won election for term {} ({}/{} votes)",
                self.id,
                new_term,
                votes,
                peers_count + 1
            );

            // Transition to Leader
            {
                let mut state = self.state.write().await;
                *state = RaftState::Leader;
            }

            // Append leader elected entry to log
            self.append_log_entry(ReplayEntryType::LeaderElected {
                leader_id: self.id.clone(),
            })
            .await?;

            // Start heartbeat loop
            self.start_heartbeat_loop().await;

            Ok(())
        } else {
            warn!(
                "Coordinator '{}' lost election for term {} ({}/{} votes)",
                self.id,
                new_term,
                votes,
                peers_count + 1
            );

            // Revert to Follower
            {
                let mut state = self.state.write().await;
                *state = RaftState::Follower;
            }

            Err(anyhow!("Lost election: insufficient votes"))
        }
    }

    /// Request votes from peers
    async fn request_votes(&self, term: u64) -> Result<usize> {
        let peers = self.peers.read().await.clone();
        let mut votes = 1; // Vote for self

        for (_peer_id, peer) in peers.iter() {
            match self.request_vote_from_peer(peer, term).await {
                Ok(true) => {
                    votes += 1;
                    debug!("Received vote from peer '{}'", peer.id);
                }
                Ok(false) => {
                    debug!("Peer '{}' denied vote", peer.id);
                }
                Err(e) => {
                    warn!("Failed to request vote from peer '{}': {}", peer.id, e);
                }
            }
        }

        Ok(votes)
    }

    /// Request vote from single peer
    async fn request_vote_from_peer(&self, peer: &PeerInfo, term: u64) -> Result<bool> {
        let url = format!("{}/kafka/raft/vote", peer.url);

        #[derive(Serialize)]
        struct VoteRequest {
            candidate_id: String,
            term: u64,
        }

        #[derive(Deserialize)]
        struct VoteResponse {
            vote_granted: bool,
            term: u64,
        }

        let request = VoteRequest {
            candidate_id: self.id.clone(),
            term,
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send vote request")?
            .json::<VoteResponse>()
            .await
            .context("Failed to parse vote response")?;

        // If peer has higher term, step down
        if response.term > term {
            warn!(
                "Peer '{}' has higher term ({}), stepping down",
                peer.id, response.term
            );
            let mut state = self.state.write().await;
            *state = RaftState::Follower;

            let mut our_term = self.term.write().await;
            *our_term = response.term;
        }

        Ok(response.vote_granted)
    }

    /// Start heartbeat loop (leader only)
    async fn start_heartbeat_loop(&self) {
        let peers = self.peers.clone();
        let state = self.state.clone();
        let term = self.term.clone();
        let id = self.id.clone();
        let http_client = self.http_client.clone();
        let interval = self.raft_config.heartbeat_interval;

        tokio::spawn(async move {
            loop {
                // Check if still leader
                {
                    let current_state = state.read().await;
                    if *current_state != RaftState::Leader {
                        info!("Coordinator '{}' no longer leader, stopping heartbeats", id);
                        break;
                    }
                }

                // Send heartbeats to all peers
                let current_term = *term.read().await;
                let peer_map = peers.read().await.clone();

                for (_peer_id, peer) in peer_map.iter() {
                    let http_client = http_client.clone();
                    let peer = peer.clone();
                    let id = id.clone();

                    tokio::spawn(async move {
                        if let Err(e) = send_heartbeat(&http_client, &peer, &id, current_term).await
                        {
                            warn!("Failed to send heartbeat to peer '{}': {}", peer.id, e);
                        }
                    });
                }

                tokio::time::sleep(interval).await;
            }
        });
    }

    /// Replay events if this coordinator is the leader
    pub async fn replay_if_leader(
        &self,
        wal: Arc<dyn WriteAheadLog>,
        sink: Arc<DurableKafkaLineageSink>,
        ack_tracker: Arc<AcknowledgmentTracker>,
    ) -> Result<Option<RecoveryReport>> {
        // Check if leader
        let state = *self.state.read().await;
        if state != RaftState::Leader {
            info!(
                "Coordinator '{}' is not leader (state: {:?}), skipping replay",
                self.id, state
            );
            return Ok(None);
        }

        info!(
            "Coordinator '{}' is leader, performing recovery replay",
            self.id
        );

        // Create replay manager
        let replay_manager = ReplayManager::new(wal, sink, ack_tracker, self.replay_config.clone());

        // Append recovery started entry
        let unacked_count = replay_manager.count_unacknowledged_events().await?;
        self.append_log_entry(ReplayEntryType::RecoveryStarted {
            leader_id: self.id.clone(),
            total_events: unacked_count,
        })
        .await?;

        // Perform recovery
        let report = replay_manager.recover_on_startup().await?;

        // Append recovery completed entry
        self.append_log_entry(ReplayEntryType::RecoveryCompleted {
            report: RecoveryReportSummary::from(&report),
        })
        .await?;

        info!(
            "Coordinator '{}' completed recovery: {}/{} events replayed",
            self.id, report.replayed_events, report.total_events
        );

        Ok(Some(report))
    }

    /// Append entry to Raft log
    async fn append_log_entry(&self, entry_type: ReplayEntryType) -> Result<()> {
        let term = *self.term.read().await;

        let entry = ReplayLogEntry {
            id: Uuid::new_v4(),
            term,
            entry_type,
            timestamp: chrono::Utc::now(),
        };

        let mut log = self.log.write().await;
        log.push(entry);

        Ok(())
    }

    /// Get current state
    pub async fn state(&self) -> RaftState {
        *self.state.read().await
    }

    /// Get current term
    pub async fn term(&self) -> u64 {
        *self.term.read().await
    }

    /// Check if this coordinator is the leader
    pub async fn is_leader(&self) -> bool {
        *self.state.read().await == RaftState::Leader
    }

    /// Get replay log
    pub async fn get_log(&self) -> Vec<ReplayLogEntry> {
        self.log.read().await.clone()
    }
}

/// Send heartbeat to peer
async fn send_heartbeat(
    http_client: &reqwest::Client,
    peer: &PeerInfo,
    leader_id: &str,
    term: u64,
) -> Result<()> {
    let url = format!("{}/kafka/raft/heartbeat", peer.url);

    #[derive(Serialize)]
    struct HeartbeatRequest {
        leader_id: String,
        term: u64,
    }

    #[derive(Deserialize)]
    struct HeartbeatResponse {
        success: bool,
        term: u64,
    }

    let request = HeartbeatRequest {
        leader_id: leader_id.to_string(),
        term,
    };

    let _response = http_client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to send heartbeat")?
        .json::<HeartbeatResponse>()
        .await
        .context("Failed to parse heartbeat response")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_initialization() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec![
                "http://coordinator-1:8080".to_string(),
                "http://coordinator-2:8080".to_string(),
                "http://coordinator-3:8080".to_string(),
            ],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(coordinator.id, "coordinator-1");
        assert_eq!(coordinator.state().await, RaftState::Follower);
        assert_eq!(coordinator.term().await, 0);
        assert_eq!(coordinator.peers.read().await.len(), 2); // Excludes self
    }

    #[tokio::test]
    async fn test_term_increment_on_election() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(coordinator.term().await, 0);

        // Start election (will fail without peers, but term should increment)
        let _ = coordinator.start_election().await;

        assert_eq!(coordinator.term().await, 1);
    }

    #[tokio::test]
    async fn test_single_coordinator_becomes_leader() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Single coordinator should win election
        coordinator.start_election().await.unwrap();

        assert_eq!(coordinator.state().await, RaftState::Leader);
    }

    #[tokio::test]
    async fn test_log_append() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        coordinator
            .append_log_entry(ReplayEntryType::LeaderElected {
                leader_id: "coordinator-1".to_string(),
            })
            .await
            .unwrap();

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 1);

        match &log[0].entry_type {
            ReplayEntryType::LeaderElected { leader_id } => {
                assert_eq!(leader_id, "coordinator-1");
            }
            _ => panic!("Unexpected entry type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_log_entries() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Append multiple entries
        coordinator
            .append_log_entry(ReplayEntryType::LeaderElected {
                leader_id: "coordinator-1".to_string(),
            })
            .await
            .unwrap();

        coordinator
            .append_log_entry(ReplayEntryType::RecoveryStarted {
                leader_id: "coordinator-1".to_string(),
                total_events: 1000,
            })
            .await
            .unwrap();

        coordinator
            .append_log_entry(ReplayEntryType::RecoveryProgress {
                replayed_events: 500,
                failed_events: 5,
            })
            .await
            .unwrap();

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 3);

        // Verify order is maintained
        match &log[0].entry_type {
            ReplayEntryType::LeaderElected { .. } => {}
            _ => panic!("Expected LeaderElected as first entry"),
        }

        match &log[1].entry_type {
            ReplayEntryType::RecoveryStarted { total_events, .. } => {
                assert_eq!(*total_events, 1000);
            }
            _ => panic!("Expected RecoveryStarted as second entry"),
        }

        match &log[2].entry_type {
            ReplayEntryType::RecoveryProgress {
                replayed_events,
                failed_events,
            } => {
                assert_eq!(*replayed_events, 500);
                assert_eq!(*failed_events, 5);
            }
            _ => panic!("Expected RecoveryProgress as third entry"),
        }
    }

    #[tokio::test]
    async fn test_state_transitions() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Initial state: Follower
        assert_eq!(coordinator.state().await, RaftState::Follower);
        assert!(!coordinator.is_leader().await);

        // After election: Leader (single coordinator always wins)
        coordinator.start_election().await.unwrap();
        assert_eq!(coordinator.state().await, RaftState::Leader);
        assert!(coordinator.is_leader().await);
    }

    #[tokio::test]
    async fn test_term_monotonically_increases() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        let initial_term = coordinator.term().await;
        assert_eq!(initial_term, 0);

        // First election
        let _ = coordinator.start_election().await;
        let term1 = coordinator.term().await;
        assert_eq!(term1, 1);

        // Second election (simulate leader crash and re-election)
        {
            let mut state = coordinator.state.write().await;
            *state = RaftState::Follower;
        }
        let _ = coordinator.start_election().await;
        let term2 = coordinator.term().await;
        assert_eq!(term2, 2);

        // Terms must never decrease
        assert!(term2 > term1);
        assert!(term1 > initial_term);
    }

    #[tokio::test]
    async fn test_peer_filtering_excludes_self() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-2",
            vec![
                "http://coordinator-1:8080".to_string(),
                "http://coordinator-2:8080".to_string(),
                "http://coordinator-3:8080".to_string(),
            ],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        let peers = coordinator.peers.read().await;

        // Should only have 2 peers (coordinator-1 and coordinator-3)
        assert_eq!(peers.len(), 2);

        // Should not include self (coordinator-2)
        assert!(!peers.contains_key("coordinator-2"));
        assert!(peers.contains_key("coordinator-1") || peers.contains_key("coordinator-3"));
    }

    #[tokio::test]
    async fn test_empty_peer_list() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        let peers = coordinator.peers.read().await;
        assert_eq!(peers.len(), 0);

        // Single coordinator should always win election (no peers to ask)
        coordinator.start_election().await.unwrap();
        assert_eq!(coordinator.state().await, RaftState::Leader);
    }

    #[tokio::test]
    async fn test_recovery_report_summary_conversion() {
        let report = RecoveryReport {
            total_events: 1000,
            replayed_events: 950,
            failed_events: 50,
            failures: vec!["Error 1".to_string(), "Error 2".to_string()],
            duration: Duration::from_secs(30),
            batches_processed: 10,
            retry_attempts: 5,
        };

        let summary = RecoveryReportSummary::from(&report);

        assert_eq!(summary.total_events, 1000);
        assert_eq!(summary.replayed_events, 950);
        assert_eq!(summary.failed_events, 50);
        assert_eq!(summary.duration_secs, 30.0);
        assert_eq!(summary.success_rate, 0.95);
    }

    #[tokio::test]
    async fn test_log_entry_timestamps_are_ordered() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        coordinator
            .append_log_entry(ReplayEntryType::LeaderElected {
                leader_id: "coordinator-1".to_string(),
            })
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        coordinator
            .append_log_entry(ReplayEntryType::RecoveryStarted {
                leader_id: "coordinator-1".to_string(),
                total_events: 100,
            })
            .await
            .unwrap();

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 2);

        // Second entry timestamp should be after first
        assert!(log[1].timestamp > log[0].timestamp);
    }

    #[tokio::test]
    async fn test_log_entries_have_unique_ids() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        for i in 0..10 {
            coordinator
                .append_log_entry(ReplayEntryType::RecoveryProgress {
                    replayed_events: i * 100,
                    failed_events: i,
                })
                .await
                .unwrap();
        }

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 10);

        // All IDs should be unique
        let mut ids = std::collections::HashSet::new();
        for entry in log {
            assert!(ids.insert(entry.id), "Duplicate ID found: {}", entry.id);
        }
    }

    #[tokio::test]
    async fn test_raft_config_defaults() {
        let config = RaftConfig::default();

        assert_eq!(config.election_timeout, Duration::from_millis(300));
        assert_eq!(config.heartbeat_interval, Duration::from_millis(50));
        assert_eq!(config.max_election_retries, 5);
        assert_eq!(config.request_timeout, Duration::from_secs(2));

        // Heartbeat interval must be < election timeout
        assert!(config.heartbeat_interval < config.election_timeout);
    }

    // Resilience Tests

    #[tokio::test]
    async fn test_concurrent_log_appends() {
        let coordinator = Arc::new(
            DistributedReplayCoordinator::new(
                "coordinator-1",
                vec!["http://coordinator-1:8080".to_string()],
                ReplayConfig::default(),
            )
            .await
            .unwrap(),
        );

        // Spawn 10 concurrent tasks appending to log
        let mut handles = vec![];
        for i in 0..10 {
            let coord = coordinator.clone();
            let handle = tokio::spawn(async move {
                coord
                    .append_log_entry(ReplayEntryType::RecoveryProgress {
                        replayed_events: i * 100,
                        failed_events: i,
                    })
                    .await
                    .unwrap();
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 10);
    }

    #[tokio::test]
    async fn test_concurrent_state_reads() {
        let coordinator = Arc::new(
            DistributedReplayCoordinator::new(
                "coordinator-1",
                vec!["http://coordinator-1:8080".to_string()],
                ReplayConfig::default(),
            )
            .await
            .unwrap(),
        );

        coordinator.start_election().await.unwrap();

        // Spawn 100 concurrent reads
        let mut handles = vec![];
        for _ in 0..100 {
            let coord = coordinator.clone();
            let handle = tokio::spawn(async move {
                let state = coord.state().await;
                let term = coord.term().await;
                let is_leader = coord.is_leader().await;

                // All reads should be consistent
                assert_eq!(state, RaftState::Leader);
                assert!(is_leader);
                assert!(term > 0);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_term_conflict_resolution() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Start with term 0
        assert_eq!(coordinator.term().await, 0);

        // First election -> term 1
        coordinator.start_election().await.unwrap();
        assert_eq!(coordinator.term().await, 1);

        // Simulate receiving a message from peer with higher term
        {
            let mut term = coordinator.term.write().await;
            *term = 5; // Peer has term 5
        }

        // Our term should now be 5
        assert_eq!(coordinator.term().await, 5);

        // Next election should increment from 5 -> 6
        {
            let mut state = coordinator.state.write().await;
            *state = RaftState::Follower;
        }
        coordinator.start_election().await.unwrap();
        assert_eq!(coordinator.term().await, 6);
    }

    // Scalability Tests

    #[tokio::test]
    async fn test_large_number_of_log_entries() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Append 1000 log entries
        for i in 0..1000 {
            coordinator
                .append_log_entry(ReplayEntryType::RecoveryProgress {
                    replayed_events: i,
                    failed_events: 0,
                })
                .await
                .unwrap();
        }

        let log = coordinator.get_log().await;
        assert_eq!(log.len(), 1000);

        // Verify all entries are present and ordered
        for (i, entry) in log.iter().enumerate() {
            match &entry.entry_type {
                ReplayEntryType::RecoveryProgress {
                    replayed_events, ..
                } => {
                    assert_eq!(*replayed_events, i);
                }
                _ => panic!("Unexpected entry type"),
            }
        }
    }

    #[tokio::test]
    async fn test_many_coordinators() {
        // Create 10 coordinators
        let mut coordinators = vec![];

        let urls: Vec<String> = (0..10)
            .map(|i| format!("http://coordinator-{}:8080", i))
            .collect();

        for i in 0..10 {
            let coord = DistributedReplayCoordinator::new(
                &format!("coordinator-{}", i),
                urls.clone(),
                ReplayConfig::default(),
            )
            .await
            .unwrap();

            coordinators.push(coord);
        }

        // Each coordinator should have 9 peers (excluding self)
        for coordinator in &coordinators {
            let peers = coordinator.peers.read().await;
            assert_eq!(peers.len(), 9);
        }

        // Each coordinator should be in Follower state initially
        for coordinator in &coordinators {
            assert_eq!(coordinator.state().await, RaftState::Follower);
            assert_eq!(coordinator.term().await, 0);
        }
    }

    #[tokio::test]
    async fn test_high_frequency_state_transitions() {
        let coordinator = DistributedReplayCoordinator::new(
            "coordinator-1",
            vec!["http://coordinator-1:8080".to_string()],
            ReplayConfig::default(),
        )
        .await
        .unwrap();

        // Rapidly transition between states
        for _ in 0..100 {
            // Follower -> Leader
            coordinator.start_election().await.unwrap();
            assert_eq!(coordinator.state().await, RaftState::Leader);

            // Leader -> Follower (simulate losing leadership)
            {
                let mut state = coordinator.state.write().await;
                *state = RaftState::Follower;
            }
            assert_eq!(coordinator.state().await, RaftState::Follower);
        }
    }

    #[tokio::test]
    async fn test_recovery_report_success_rate_edge_cases() {
        // Test with 100% success
        let report = RecoveryReport {
            total_events: 1000,
            replayed_events: 1000,
            failed_events: 0,
            failures: vec![],
            duration: Duration::from_secs(10),
            batches_processed: 10,
            retry_attempts: 0,
        };
        assert_eq!(report.success_rate(), 1.0);
        assert!(report.is_successful());

        // Test with 0% success
        let report = RecoveryReport {
            total_events: 1000,
            replayed_events: 0,
            failed_events: 1000,
            failures: vec!["Error".to_string()],
            duration: Duration::from_secs(10),
            batches_processed: 10,
            retry_attempts: 5,
        };
        assert_eq!(report.success_rate(), 0.0);
        assert!(!report.is_successful());

        // Test with no events (edge case)
        let report = RecoveryReport {
            total_events: 0,
            replayed_events: 0,
            failed_events: 0,
            failures: vec![],
            duration: Duration::from_secs(0),
            batches_processed: 0,
            retry_attempts: 0,
        };
        assert_eq!(report.success_rate(), 1.0); // Default to 100% when no events
        assert!(report.is_successful());
    }
}
