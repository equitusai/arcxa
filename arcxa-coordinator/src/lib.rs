//! Graphica Coordinator Library
//!
//! Main application with temporal indexes, API layer, and query coordinator.
//!
//! ## Architecture
//!
//! This crate contains:
//! - Query coordinator (scatter-gather across shards)
//! - REST + gRPC APIs (Axum, Tonic)
//! - Temporal indexes (RocksDB)
//! - WAL (Write-Ahead Log with RocksDB)
//! - Bitemporal storage
//! - Connection pool to shards
//!
//! ## Key Dependencies
//!
//! - `graphica-core`: Shared domain types and proto
//! - `rocksdb`: Temporal indexes (librocksdb-sys v0.16.0)
//! - NO `oxigraph`: RDF storage is in shard processes

#![recursion_limit = "600"]
#![type_length_limit = "10000000"]

pub mod api;
pub mod app_context;
pub mod bitemporal;
pub mod catalog_impl;
pub mod catalog_to_dataset;
pub mod checkpointing; // Checkpoint persistence for ETL operations
pub mod common;
pub mod config;
pub mod etl;
pub mod gdpr; // GDPR compliance (Article 17: Right to Erasure)
pub mod governance;
pub mod ha; // High-availability coordinator module
pub mod mapping;
pub mod observability;
pub mod security; // Security validation and job ID protection
pub mod storage;
pub mod workflows;

#[cfg(test)]
pub mod test_helpers;

pub use graphica_core as core;

// Re-export commonly used types
pub use app_context::AppContext;
pub use graphica_core::distributed::{HashRange, ShardId, ShardMetadata, ShardServiceClient};
