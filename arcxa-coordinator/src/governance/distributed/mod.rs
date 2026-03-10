//! Distributed RDF Storage Layer
//!
//! This module provides horizontal scaling capabilities for Graphica's RDF storage:
//!
//! - **Shard Metadata**: Types for representing shards, hash ranges, and cluster topology
//! - **Shard Registry**: Persistent storage and management of cluster configuration
//! - **Query Router**: (Future) Distributed query execution across shards
//! - **Shard Manager**: (Future) Dynamic shard provisioning and scaling
//! - **Replication**: (Future) Raft-based replication for fault tolerance
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Client Query                              │
//! └────────────────────┬────────────────────────────────────────┘
//!                      │
//!          ┌───────────▼───────────┐
//!          │   Query Router        │
//!          │   (Hash-based)        │
//!          └───────────┬───────────┘
//!                      │
//!        ┌─────────────┼─────────────┐
//!        │             │             │
//!   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
//!   │ Shard 0 │   │ Shard 1 │   │ Shard 2 │
//!   │ (0-33%) │   │ (33-66%)│   │ (66-100%)│
//!   └────┬────┘   └────┬────┘   └────┬────┘
//!        │             │             │
//!   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
//!   │ Replica │   │ Replica │   │ Replica │
//!   │ (Raft)  │   │ (Raft)  │   │ (Raft)  │
//!   └─────────┘   └─────────┘   └─────────┘
//! ```
//!
//! ## Hash-Based Sharding
//!
//! Triples are distributed across shards using consistent hashing:
//!
//! ```ignore
//! use std::collections::hash_map::DefaultHasher;
//! use std::hash::{Hash, Hasher};
//!
//! fn calculate_hash(subject: &str, predicate: &str, object: &str) -> u64 {
//!     let mut hasher = DefaultHasher::new();
//!     subject.hash(&mut hasher);
//!     predicate.hash(&mut hasher);
//!     object.hash(&mut hasher);
//!     hasher.finish()
//! }
//! ```
//!
//! ## Usage Example
//!
//! ```ignore
//! use graphica::governance::distributed::{ShardRegistry, ShardMetadata, HashRange, ShardId};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create shard registry
//! let registry = ShardRegistry::new("./data/shards", 3, 60)?;
//!
//! // Register shards
//! let shard1 = ShardMetadata::new(
//!     ShardId(0),
//!     HashRange::new(0, u64::MAX / 3),
//!     "shard-0.cluster.local:9090".to_string(),
//!     vec!["shard-0-replica-1:9090".to_string()],
//! );
//!
//! registry.register_shard(shard1)?;
//!
//! // Find shard for triple
//! let hash = 123456789u64;
//! let shard = registry.find_shard_for_hash(hash)?;
//!
//! // Query topology
//! let topology = registry.get_topology()?;
//! println!("Cluster has {} shards", topology.total_shards);
//! # Ok(())
//! # }
//! ```

pub mod auto_registration;
pub mod coordinator_proto;
pub mod coordinator_service_impl;
pub mod shard_metadata;
pub mod shard_registry;

// Re-export key types
pub use auto_registration::{AutoRegistrationHandler, RegistrationConfig};
pub use coordinator_proto::{CoordinatorService, CoordinatorServiceServer, ShardCapabilities};
pub use coordinator_service_impl::{CoordinatorServiceConfig, CoordinatorServiceImpl};
pub use shard_metadata::{
    ClusterTopology, HashRange, ReplicationConfig, ShardId, ShardMetadata, ShardStatus,
};
pub use shard_registry::ShardRegistry;
