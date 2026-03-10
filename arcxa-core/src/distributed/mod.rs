//! Distributed System Core Types
//!
//! Shared types and proto definitions for the distributed RDF shard system.
//!
//! This module contains:
//! - gRPC proto definitions (generated code)
//! - Shared types (ShardId, HashRange, ShardMetadata)
//! - No storage implementations (those are in shard/coordinator)

pub mod connection_pool;
pub mod coordinator;
pub mod coordinator_client;
pub mod proto;
pub mod shard_client;
pub mod shard_identity;
pub mod types;

// Re-export commonly used proto types
pub use proto::shard_service::{
    shard_service_client::ShardServiceClient,
    shard_service_server::{ShardService, ShardServiceServer},
    *,
};

// Re-export distributed types
pub use types::{HashRange, ShardId, ShardMetadata};
