//! ETL (Extract, Transform, Load) Execution Framework
//!
//! Professional-grade ETL executors for workflow orchestration.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌──────────────┐
//! │   Sources   │────▶│ Transformers │────▶│   Loaders    │
//! └─────────────┘     └──────────────┘     └──────────────┘
//!      ↓                     ↓                     ↓
//!   CSV, DB             Field Ops           PostgreSQL,
//!   Extract                                 DB2, RDF
//! ```
//!
//! ## Design Principles
//!
//! 1. **Fully Async**: All I/O operations use async/await
//! 2. **Connection Pooling**: Shared connection pools via context
//! 3. **Database Abstraction**: Common traits with database-specific implementations
//! 4. **Error Handling**: Comprehensive error context and recovery
//! 5. **Observability**: Structured logging and metrics
//!
//! ## Module Organization
//!
//! - `sources/`: Data extraction (CSV, databases)
//! - `transformers/`: Data transformation (field operations)
//! - `loaders/`: Data loading (databases, RDF)
//! - `context`: Shared execution context and connection pools

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

// New ETL abstractions (from redesign)
pub mod adapters; // Adapter layer for workflow integration
pub mod destinations;
pub mod errors;
pub mod readers; // Format readers (CSV, JSON, Parquet, etc.)
pub mod traits; // Data destinations (DB2, PostgreSQL, etc.)

// New module structure
pub mod context;
pub mod loaders;
pub mod orchestration;
pub mod sources;
pub mod transformers;

// Legacy module exports (for backward compatibility)
// TODO: Remove these in v3.0.0 after 2-release deprecation cycle
#[deprecated(
    since = "2.1.0",
    note = "Use sources::csv module directly instead of csv_source"
)]
pub mod csv_source {
    pub use super::sources::csv::*;
}

#[deprecated(
    since = "2.1.0",
    note = "Use sources::database module directly instead of db_extract"
)]
pub mod db_extract {
    pub use super::sources::database::*;
}

#[deprecated(
    since = "2.1.0",
    note = "Use transformers::field module directly instead of field_transformer"
)]
pub mod field_transformer {
    pub use super::transformers::field::*;
}

#[deprecated(
    since = "2.1.0",
    note = "Use loaders::rdf module directly instead of rdf_loader"
)]
pub mod rdf_loader {
    pub use super::loaders::rdf::*;
}

// Re-export commonly used types
pub use context::EtlContext;
pub use sources::{CsvSourceExecutor, DbExtractExecutor};
pub use transformers::FieldTransformerExecutor;

// Deprecated: DbLoaderExecutor has been removed
// For backward compatibility, we provide a type alias to PostgreSQLLoader
// TODO: Remove in v3.0.0 after 2-release deprecation cycle
#[deprecated(
    since = "2.1.0",
    note = "DbLoaderExecutor has been removed. Use loaders::database::PostgreSQLLoader for trait-based loading instead. \
            See /docs/migration/LOADER_MIGRATION.md for migration guide."
)]
pub type DbLoaderExecutor = loaders::database::PostgreSQLLoader;
pub use loaders::rdf::RdfLoaderExecutor;

// Re-export database loaders
pub use loaders::database::{DatabaseLoader, DatabaseLoaderFactory};

// NOTE: Naming Conflict Warning (v2.1.0)
// =======================================
// There are TWO classes named `PostgreSQLLoader` in different modules:
//
// 1. **ETL Module** (this one):
//    - Path: `graphica::etl::loaders::database::PostgreSQLLoader`
//    - Pattern: Trait-based (implements `DatabaseLoader`)
//    - Use for: New ETL pipelines, trait-based abstractions
//
// 2. **Mapping Module**:
//    - Path: `graphica::mapping::loader::postgres::PostgreSQLLoader`
//    - Pattern: Feature-rich (checkpointing, DLQ, lineage)
//    - Use for: Production workflows needing advanced features
//
// To avoid ambiguity, use fully qualified paths:
//   use graphica::etl::loaders::database::PostgreSQLLoader as EtlPostgresLoader;
//   use graphica::mapping::loader::postgres::PostgreSQLLoader as MappingPostgresLoader;
//
// See docs/architecture/LOADER_INVENTORY.md for complete guidance.
// This naming conflict will be resolved in v2.2.0 (mapping version will be renamed).
pub use loaders::database::PostgreSQLLoader;

// DB2Loader removed - use workflows/engine/transformers/db2_load.rs instead

// NOTE: LoadMode Enum Consolidation (v2.1.0)
// ===========================================
// LoadMode is now the CANONICAL enum from `etl::traits` module.
// Previously, there were 5 different LoadMode enums across the codebase:
//   1. etl::traits::LoadMode (canonical - most complete)
//   2. etl::loaders::database::LoadMode (deprecated - use traits version)
//   3. mapping::loader::postgres_bulk::LoadMode (deprecated - use traits version)
//   4. mapping::loader::db2_load_executor::LoadMode (deprecated - use traits version)
//   5. graphica_core::orchestration::workflow::definition::LoadMode (deprecated)
//
// All duplicate LoadMode enums now re-export from etl::traits.
// See below (line 130) for the canonical LoadMode export.
// See docs/migration/LOADER_MIGRATION.md for migration guide.

// Re-export orchestration components
pub use orchestration::{
    ConflictResolution, EtlLineageTracker, FieldLineage, LineageChain, LoadOrchestrator,
    LoadPipeline, LoadStats, UnifiedMappingCoordinator, UnifiedMappingSession,
};

// Re-export new ETL abstractions
pub use errors::{ErrorAccumulator, ErrorContext, EtlError, EtlResult};
pub use traits::{
    DataDestination,
    DataDestinationFactory,
    DataExtractor,
    // Data types
    DataRecord,
    DataType,
    ErrorTolerance,
    FieldSchema,
    // Core traits
    FormatReader,
    // Factory traits
    FormatReaderFactory,
    // Result types
    FormatStats,
    // Configuration types
    LoadConfig,
    LoadMode,
    LoadStats as LoadStatsNew,
    PipelineConfig,
    PipelineExecutor,
    PipelineStats,
    RecordSchema,
    SourceLocation,
    TransformResult,
    Transformer as EtlTransformer,
    TransformerFactory,
    ValidationReport,
};

// Re-export adapters for workflow integration
pub use adapters::{DataDestinationAdapter, FormatReaderAdapter, PipelineTransformerAdapter};

// Re-export format readers
pub use readers::{CsvOptions, CsvReader};

// Re-export data destinations
pub use destinations::Db2Destination;

/// Core ETL executor trait
///
/// All ETL step executors implement this trait for uniform execution.
#[async_trait]
pub trait EtlExecutor: Send + Sync {
    /// Execute the ETL step with the given input data
    ///
    /// # Arguments
    /// * `input` - Input data (typically JSON array of records)
    ///
    /// # Returns
    /// Output data (typically JSON with records + metadata)
    async fn execute(&self, input: Value) -> Result<Value>;

    /// Get the step type name for logging and metrics
    fn step_type(&self) -> &'static str;

    /// Validate configuration before execution (optional)
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

/// ETL execution result metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionMetadata {
    /// Number of records processed
    pub records_processed: usize,

    /// Number of records successfully loaded
    pub records_success: usize,

    /// Number of records that failed
    pub records_failed: usize,

    /// Execution duration in milliseconds
    pub duration_ms: u64,

    /// Step type
    pub step_type: String,

    /// Additional step-specific metadata
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl ExecutionMetadata {
    pub fn new(step_type: impl Into<String>) -> Self {
        Self {
            records_processed: 0,
            records_success: 0,
            records_failed: 0,
            duration_ms: 0,
            step_type: step_type.into(),
            extra: serde_json::Map::new(),
        }
    }

    pub fn with_processed(mut self, count: usize) -> Self {
        self.records_processed = count;
        self.records_success = count;
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn add_extra(&mut self, key: impl Into<String>, value: Value) {
        self.extra.insert(key.into(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_metadata() {
        let mut metadata = ExecutionMetadata::new("test_step")
            .with_processed(100)
            .with_duration(500);

        metadata.add_extra("test_key", Value::String("test_value".to_string()));

        assert_eq!(metadata.records_processed, 100);
        assert_eq!(metadata.duration_ms, 500);
        assert_eq!(metadata.step_type, "test_step");
        assert!(metadata.extra.contains_key("test_key"));
    }

    #[test]
    fn test_new_etl_error_types_exported() {
        // Validate that new ETL error types are properly exported
        // This is a compile-time test - if it compiles, exports are correct

        // Test error types
        let _error: EtlError = EtlError::Other(anyhow::anyhow!("test"));
        let _result: EtlResult<()> = Ok(());
        let _context = ErrorContext::new("test");
        let _accumulator = ErrorAccumulator::new(10, false);

        // If we got here, error types are accessible
        assert!(true);
    }

    #[test]
    fn test_etl_error_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("test error");
        let etl_err: EtlError = anyhow_err.into();

        match etl_err {
            EtlError::Other(_) => {
                // Correct conversion
                assert!(true);
            }
            _ => panic!("Expected EtlError::Other variant"),
        }
    }

    // Note: Full trait tests will be added after we implement the first FormatReader
    // in Phase 1, Task 1.3 (CsvReader implementation)
}
