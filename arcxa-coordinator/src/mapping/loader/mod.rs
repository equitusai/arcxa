//! Database Loader Module
//!
//! This module provides functionality for bulk loading data from CSV files
//! into target relational databases.
//!
//! ## Supported Databases
//!
//! - PostgreSQL (via `postgres` module)
//! - IBM DB2 (via `db2` module)
//! - Oracle (via `oracle` module)
//!
//! ## Architecture
//!
//! The loader workflow:
//! 1. Read CSV files with streaming reader
//! 2. Apply field transformations
//! 3. Resolve conflicts using ConflictResolver
//! 4. Generate target database DDL
//! 5. Execute bulk load operations (INSERT or DB2 LOAD utility)
//! 6. Track lineage as RDF triples

pub mod checkpoint;
pub mod csv_reader;
pub mod db2;
pub mod db2_connection;
pub mod db2_del_generator;
pub mod db2_load_executor;
pub mod db2_pool; // Async connection pooling for DB2
pub mod dlq;
pub mod dlq_reader; // DLQ row reader with pagination and filtering
pub mod dlq_reprocessor; // DLQ reprocessing with retry logic
pub mod dlq_stats; // DLQ statistics calculator (production-ready)
pub mod error_handler;
pub mod lineage; // W3C PROV lineage capture (Sprint 1.4)
pub mod odbc_db2_connection; // ODBC-based DB2 connection (production)
pub mod oracle; // Oracle database loader
pub mod orchestration; // Job orchestration for background ETL execution
pub mod postgres;
pub mod postgres_bulk; // PostgreSQL bulk loader with COPY support
pub mod transformation; // High-performance transformation engine for field transformations
pub mod transformation_executor;
pub mod transformation_integration;
pub mod unified_session_loader; // Unified session loader for CSV-to-DB workflows // Integration of transformation engine with unified loader

// PostgreSQL loaders (mapping subsystem)
pub use checkpoint::{
    BatchProgress, BatchState, Checkpoint, CheckpointConfig, CheckpointManager, ErrorCategory,
    ErrorRecord, ErrorSummary, LoadState,
};
pub use csv_reader::{CsvError, CsvReaderConfig, CsvStreamReader, ReaderProgress};
pub use db2::DB2Loader;
pub use db2_connection::{
    DB2Config, DB2Connection, DB2ConnectionManager, DB2Error, MockDB2Connection, PooledConnection,
    SqlParam, SqlParamType,
};
pub use db2_del_generator::{
    ColumnMapping, DelFileGenerator, DelFileStats, DelGenerationError, DelGeneratorConfig,
};
pub use db2_load_executor::{
    DB2LoadExecutor, ExceptionRow, LoadExecutorConfig, LoadMode, LoadResult as DB2LoadResult,
};
pub use db2_pool::{
    create_db2_pool, get_pool_stats, DB2Pool, DB2PoolConfig, PoolStats, PoolTimeouts,
    PooledDB2Connection,
};
pub use dlq::{find_dlq_files, DeadLetterQueue, DlqConfig, DlqFormat, DlqStats, FailedRow};
pub use dlq_reader::{DlqReader, DlqRecord, DlqReprocessFilter};
pub use dlq_reprocessor::{DlqReprocessor, ReprocessResult};
pub use dlq_stats::{DlqStatsCalculator, DlqStatsDto};
pub use error_handler::{
    BatchErrorHandler, CircuitBreaker, CircuitBreakerState, ErrorHandler, ErrorHandlerConfig,
};
pub use lineage::RdfLineageSink;
pub use odbc_db2_connection::OdbcDB2Connection;
pub use oracle::OracleLoader;
pub use postgres::{
    LoadResult,
    LoaderConfig,
    MappingPostgresLoader, // Primary export
    PostgreSQLLoader,      // Deprecated alias (re-exported for backward compatibility)
    SourceRow,
};
pub use postgres_bulk::{
    LoadMode as PgLoadMode,    // Deprecated - use PgBulkLoadMode
    MappingPostgresBulkLoader, // Primary export
    PgBulkLoadMode,
    PostgreSQLBulkConfig,
    PostgreSQLBulkLoader, // Deprecated alias (re-exported for backward compatibility)
};
pub use transformation::{DataType, TransformationEngine, Value as TransformValue};
pub use transformation_executor::{TransformFunction, TransformationExecutor, Value};
pub use transformation_integration::UnifiedTransformationProcessor;
