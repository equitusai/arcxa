//! R2RML Executor Module
//!
//! Executes R2RML mappings against CSV/Parquet data to generate RDF triples.

pub mod csv_executor;

pub use csv_executor::R2rmlExecutor;
