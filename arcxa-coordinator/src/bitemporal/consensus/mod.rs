//! Raft-based distributed consensus for transaction ordering.
//!
//! This module provides distributed consensus using the Raft algorithm,
//! replacing the single-node AtomicU64 transaction counter with a
//! production-grade distributed transaction ID allocation system.
//!
//! ## Feature Flag
//!
//! This module is gated behind the `raft-consensus` feature flag for
//! backward compatibility. When disabled, the system falls back to
//! local atomic counter in `transaction_manager.rs`.
//!
//! ## Architecture
//!
//! ```text
//! TransactionManager
//!     ↓
//! RaftManager (if feature enabled)
//!     ↓
//! Raft Library (tikv/raft-rs)
//!     ↓
//! RaftStorage (RocksDB)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::bitemporal::consensus::RaftManager;
//! use std::collections::HashMap;
//!
//! let peers = HashMap::new();  // Single-node cluster for testing
//! let manager = RaftManager::new(1, peers, "/tmp/raft").unwrap();
//! ```
//!
//! ## Implementation Status
//!
//! - [x] Week 1: Basic structure and types
//! - [ ] Week 2: Proposal submission
//! - [ ] Week 3: Raft tick loop
//! - [ ] Week 4: Multi-node testing

#[cfg(feature = "raft-consensus")]
pub mod codec;
#[cfg(feature = "raft-consensus")]
pub mod config;
#[cfg(feature = "raft-consensus")]
pub mod proposal;
#[cfg(feature = "raft-consensus")]
pub mod raft_manager;
#[cfg(feature = "raft-consensus")]
pub mod storage;

#[cfg(feature = "raft-consensus")]
pub use config::RaftConfig;
#[cfg(feature = "raft-consensus")]
pub use proposal::{IsolationLevel, TransactionProposal};
#[cfg(feature = "raft-consensus")]
pub use raft_manager::RaftManager;

#[cfg(test)]
mod tests;
