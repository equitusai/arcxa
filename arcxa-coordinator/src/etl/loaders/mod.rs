//! ETL Data Loaders
//!
//! Professional data loading implementations for various targets.

pub mod database;
pub mod rdf;

// Re-export commonly used types
pub use database::{DatabaseLoader, DatabaseLoaderFactory, LoadMode, PostgreSQLLoader};
// DB2Loader removed - use workflows/engine/transformers/db2_load.rs instead
