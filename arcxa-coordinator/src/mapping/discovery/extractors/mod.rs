//! # Schema Extractors
//!
//! Data source-specific implementations for schema discovery.

pub mod csv;
pub mod databricks;
pub mod db2;
pub mod odbc;
pub mod oracle;
pub mod oracle_pool;
pub mod postgresql;
pub mod saphana;
pub mod saphana_pool;
mod shared;
pub mod traits;

pub use csv::CsvExtractor;
pub use databricks::DatabricksExtractor;
pub use db2::DB2Extractor;
pub use oracle::OracleExtractor;
pub use oracle_pool::OdbcOracleConnection;
pub use postgresql::PostgreSQLExtractor;
pub use saphana::SAPHANAExtractor;
pub use saphana_pool::OdbcSAPHANAConnection;
pub use traits::{ExtractorRegistry, SchemaExtractor};

// Placeholder for future extractor implementations
// These will be added in Phase 3+
//
// pub mod snowflake;
// pub mod s3_parquet;
