//! Checkpointing Module
//!
//! Production-grade checkpoint persistence for ETL operations with hybrid storage.
//!
//! ## Architecture
//!
//! Uses a hybrid storage approach:
//! - **RocksDB**: Hot checkpoint data (current offsets, recent checkpoints) for fast lookups
//! - **RDF Store**: Checkpoint metadata, history, lineage links for queryability
//! - **File System**: DLQ row data (JSON/CSV/Parquet files)
//!
//! ## Features
//!
//! - Fast checkpoint status retrieval from RocksDB
//! - Historical checkpoint queries via SPARQL
//! - Automatic checkpoint compression and archival
//! - Integration with DLQ for error tracking
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::checkpointing::CheckpointPersistence;
//!
//! let persistence = CheckpointPersistence::new(rocksdb, rdf_store);
//!
//! // Save checkpoint
//! persistence.save_checkpoint(&job_id, &checkpoint).await?;
//!
//! // Get status
//! let status = persistence.get_checkpoint_status(&job_id).await?;
//! ```

pub mod persistence;

pub use persistence::{Checkpoint, CheckpointPersistence};
