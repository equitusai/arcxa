//! # Schema Extractor Trait
//!
//! Core abstraction for extracting schemas from different data sources.

use anyhow::Result;
use async_trait::async_trait;

use super::super::types::*;
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;

/// Schema extractor interface
///
/// Each data source type implements this trait to provide:
/// - Metadata extraction (tables, columns, types, constraints)
/// - Sample value extraction (stratified sampling for profiling)
/// - Statistics extraction (cardinality, indexes, distributions)
///
/// ## Implementation Notes
///
/// - Use connection pooling where possible
/// - Implement efficient sampling (TABLESAMPLE, SAMPLE, etc.)
/// - Handle large tables gracefully (sampling, not full scans)
/// - Cache system catalog queries when appropriate
#[async_trait]
pub trait SchemaExtractor: Send + Sync {
    /// Extract schema metadata from data source
    ///
    /// Queries system catalogs (INFORMATION_SCHEMA, pg_catalog, etc.) to discover:
    /// - Table names and types (base table, view, materialized view)
    /// - Column names, data types, nullability, defaults
    /// - Primary keys, foreign keys, unique constraints
    /// - Table statistics (row counts, size estimates)
    ///
    /// ## Performance
    ///
    /// Should complete in <100ms for typical schemas (<100 tables).
    /// Uses system catalogs which are heavily cached by databases.
    ///
    /// ## Parameters
    ///
    /// - `source`: Data source configuration
    /// - `credentials`: Connection credentials
    /// - `schema_filter`: Optional schema name filter (e.g., "public")
    /// - `table_filter`: Optional table name filter (e.g., "customers")
    async fn extract_metadata(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata>;

    /// Extract sample values from a table
    ///
    /// Executes stratified sampling queries to retrieve representative data:
    /// - PostgreSQL: `TABLESAMPLE BERNOULLI (10) LIMIT n`
    /// - Snowflake: `SAMPLE (1000 ROWS)`
    /// - Oracle: `SAMPLE (10)`
    /// - Parquet/CSV: Read first N rows
    ///
    /// ## Performance
    ///
    /// Should complete in <500ms for typical tables.
    /// Uses database-specific sampling to avoid full table scans.
    ///
    /// ## Parameters
    ///
    /// - `source`: Data source configuration
    /// - `credentials`: Connection credentials
    /// - `table_name`: Table to sample
    /// - `sample_size`: Number of rows to retrieve (default: 1000)
    async fn extract_samples(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        sample_size: usize,
    ) -> Result<Vec<SampleRow>>;

    /// Extract column statistics from system catalogs
    ///
    /// Retrieves pre-computed statistics from:
    /// - PostgreSQL: `pg_stats`
    /// - Oracle: `DBA_TAB_COLUMNS`, `DBA_TAB_COL_STATISTICS`
    /// - Snowflake: `INFORMATION_SCHEMA.COLUMNS`
    ///
    /// ## Performance
    ///
    /// Should complete in <50ms per column.
    /// Uses pre-computed statistics from system tables.
    ///
    /// ## Parameters
    ///
    /// - `source`: Data source configuration
    /// - `credentials`: Connection credentials
    /// - `table_name`: Table name
    /// - `column_name`: Column name
    async fn extract_statistics(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        column_name: &str,
    ) -> Result<ColumnStats>;

    /// Get extractor name (for logging and debugging)
    fn name(&self) -> &'static str;

    /// Check if this extractor supports the given source type
    fn supports_source(&self, source_type: &str) -> bool {
        source_type == self.name()
    }
}

/// Registry for schema extractors
///
/// Manages collection of extractors and routes requests to appropriate implementation.
pub struct ExtractorRegistry {
    extractors: std::collections::HashMap<String, Box<dyn SchemaExtractor>>,
}

impl ExtractorRegistry {
    /// Create a new extractor registry
    pub fn new() -> Self {
        Self {
            extractors: std::collections::HashMap::new(),
        }
    }

    /// Register an extractor for a source type
    pub fn register<E: SchemaExtractor + 'static>(&mut self, source_type: String, extractor: E) {
        self.extractors
            .insert(source_type.to_lowercase(), Box::new(extractor));
    }

    /// Get extractor for a source type
    pub fn get(&self, source_type: &str) -> Option<&dyn SchemaExtractor> {
        self.extractors
            .get(&source_type.to_lowercase())
            .map(|e| e.as_ref())
    }

    /// List all registered extractors
    pub fn list_extractors(&self) -> Vec<&str> {
        self.extractors.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
