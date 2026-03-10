//! High Availability Coordinator Module
//!
//! This module implements the high-availability coordinator architecture
//! using Raft consensus for leader election and state replication.
//!
//! ## Feature Flag
//!
//! This module requires the `raft-consensus` feature flag to be enabled.

// Raft-based coordinator requires the raft-consensus feature
#[cfg(feature = "raft-consensus")]
pub mod raft_coordinator;
#[cfg(feature = "raft-consensus")]
pub mod state_machine;

// These modules don't require Raft
pub mod consistent_hash;
pub mod discovery;
pub mod health_monitor;
pub mod rebalancer;

pub use consistent_hash::{ConsistentHashRouter, VirtualNode};
pub use discovery::{DiscoveryMethod, ServiceDiscovery};
pub use health_monitor::{HealthMonitor, HealthMonitorConfig, ShardHealth};
#[cfg(feature = "raft-consensus")]
pub use raft_coordinator::{CoordinatorMessage, CoordinatorRole, NetworkCallback, RaftCoordinator};
pub use rebalancer::{MigrationPlan, MigrationState, RebalancerConfig, ShardRebalancer};
#[cfg(feature = "raft-consensus")]
pub use state_machine::{CoordinatorStateMachine, StateCommand};
