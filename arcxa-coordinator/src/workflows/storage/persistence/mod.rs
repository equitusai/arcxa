//! Persistence Layer
//!
//! Provides abstraction and implementations for workflow execution persistence.
//!
//! # Architecture
//!
//! This module follows clean architecture principles with trait-based abstractions:
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  Application Layer                  │
//! │  (Workflow API, Executors)          │
//! └──────────────┬──────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────┐
//! │  Storage Abstraction (Traits)       │
//! │  - ExecutionStoreBackend            │
//! │  - CheckpointStore                  │
//! └──────────────┬──────────────────────┘
//!                │
//!      ┌─────────┴─────────┐
//!      ▼                   ▼
//! ┌──────────┐      ┌──────────────┐
//! │ In-Memory│      │   RocksDB    │
//! │ Backend  │      │   Backend    │
//! └──────────┘      └──────────────┘
//! ```
//!
//! # Usage
//!
//! ## Using the in-memory backend
//!
//! ```ignore
//! use graphica_coordinator::workflows::storage::ExecutionStore;
//!
//! let store = ExecutionStore::new();
//! store.save(execution).await?;
//! ```
//!
//! ## Using the RocksDB backend
//!
//! ```ignore
//! use graphica_coordinator::workflows::storage::persistence::RocksDbBackend;
//!
//! let backend = RocksDbBackend::open("/path/to/db")?;
//! let store = ExecutionStore::with_backend(backend);
//! store.save(execution).await?;
//! ```
//!
//! # Migration
//!
//! To migrate from in-memory to RocksDB without downtime:
//!
//! ```ignore
//! use graphica_coordinator::workflows::storage::persistence::MigrationAdapter;
//!
//! let old_store = ExecutionStore::new();
//! let new_backend = RocksDbBackend::open("/path/to/db")?;
//! let migration = MigrationAdapter::new(old_store, new_backend);
//!
//! // Phase 1: Dual-write (write to both, read from old)
//! migration.set_mode(MigrationMode::DualWrite).await?;
//!
//! // Phase 2: Verify consistency
//! migration.verify_consistency().await?;
//!
//! // Phase 3: Cutover (read from new)
//! migration.set_mode(MigrationMode::Cutover).await?;
//! ```

mod approval_backend;
pub mod error;
mod in_memory_backend;
pub mod rocksdb_backend;
pub mod rocksdb_config;
pub mod traits;

// Re-export commonly used types
pub use approval_backend::InMemoryApprovalBackend;
pub use error::{PersistenceError, Result};
pub use in_memory_backend::InMemoryBackend;
pub use rocksdb_backend::RocksDbBackend;
pub use rocksdb_config::RocksDbConfig;
pub use traits::{ApprovalStoreBackend, Checkpoint, CheckpointStore, ExecutionStoreBackend};
