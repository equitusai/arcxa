//! Data Destination Implementations
//!
//! This module provides concrete implementations of the DataDestination trait
//! for various target systems (databases, file systems, message queues, etc.).
//!
//! ## Available Destinations
//!
//! - **DB2**: IBM DB2 database destination with connection pooling, transactions,
//!   and support for INSERT, UPSERT, and REPLACE modes.
//!
//! ## Design Principles
//!
//! 1. **Connection Reuse**: All destinations support connection pooling
//! 2. **Transaction Safety**: ACID guarantees for batch operations
//! 3. **Error Handling**: Comprehensive error context and retry logic
//! 4. **Performance**: Batch writes optimized for each destination type
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::etl::destinations::Db2Destination;
//! use graphica_coordinator::etl::traits::{DataDestination, LoadConfig, LoadMode};
//!
//! let pool = create_db2_pool(pool_config).await?;
//! let mut destination = Db2Destination::new(pool, "CUSTOMERS".to_string());
//!
//! // Prepare destination (create table, start transaction)
//! destination.prepare(&schema, &load_config).await?;
//!
//! // Load data stream
//! let stats = destination.load_stream(record_stream, &load_config).await?;
//!
//! // Finalize (commit, create indexes)
//! destination.finalize().await?;
//! ```

pub mod db2;

// Re-export destination types
pub use db2::Db2Destination;
