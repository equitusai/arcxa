//! Enhanced Data Source Connector V2
//!
//! Extends the base DataSourceConnector trait with:
//! - Unified schema profiling
//! - Streaming data access
//! - Cross-source data export
//! - Integration with UnifiedSchema model

use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

use super::connector::{ConnectorResult, Credentials, DataSourceConnector};
use super::types::DataSource;
use crate::errors::GraphicaError;
use crate::schema::{DataProfiler, ProfileConfig, SampleRow, UnifiedSchema};

/// Result type for V2 connector operations
pub type ConnectorV2Result<T> = Result<T, GraphicaError>;

/// Streaming row batch type
pub type RowBatch = Vec<SampleRow>;

/// Boxed stream of row batches
pub type DataStream = Pin<Box<dyn Stream<Item = ConnectorV2Result<RowBatch>> + Send>>;

/// Enhanced connector interface with unified profiling and streaming
///
/// This trait extends DataSourceConnector with capabilities for:
/// - Unified schema profiling across all source types
/// - Efficient streaming for large datasets
/// - Cross-source data export in multiple formats
#[async_trait]
pub trait DataSourceConnectorV2: DataSourceConnector {
    /// Get profiler for this datasource type
    ///
    /// Returns a DataProfiler implementation that can profile tables/files
    /// and produce UnifiedSchema with field-level statistics.
    fn get_profiler(&self) -> Box<dyn DataProfiler>;

    /// Get unified schema for a specific table/collection
    ///
    /// This is a convenience method that combines connection + profiling
    /// to return UnifiedSchema directly. Uses the connector's profiler
    /// with default configuration.
    ///
    /// # Arguments
    /// * `source` - Data source configuration
    /// * `credentials` - Authentication credentials
    /// * `table_name` - Name of table/collection to profile
    /// * `config` - Profiling configuration
    async fn get_unified_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        config: ProfileConfig,
    ) -> ConnectorV2Result<UnifiedSchema>;

    /// Stream data in batches for efficient processing
    ///
    /// Returns a stream of row batches for incremental processing
    /// of large datasets without loading everything into memory.
    ///
    /// # Arguments
    /// * `source` - Data source configuration
    /// * `credentials` - Authentication credentials
    /// * `table_or_query` - Table name or SQL query
    /// * `batch_size` - Number of rows per batch
    ///
    /// # Returns
    /// Stream of row batches (Vec<HashMap<String, Value>>)
    async fn stream_data(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_or_query: &str,
        batch_size: usize,
    ) -> ConnectorV2Result<DataStream>;

    /// Export data to specific format
    ///
    /// Supports cross-source data movement by exporting table data
    /// in common interchange formats (CSV, JSON, Parquet, Arrow).
    ///
    /// # Arguments
    /// * `source` - Data source configuration
    /// * `credentials` - Authentication credentials
    /// * `table_name` - Table to export
    /// * `format` - Target export format
    /// * `config` - Export configuration options
    async fn export_to_format(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        format: ExportFormat,
        config: ExportConfig,
    ) -> ConnectorV2Result<Vec<u8>>;

    /// Get sample data for preview
    ///
    /// Returns limited number of rows for UI preview or validation.
    /// This is similar to the profiler's get_sample_data but integrated
    /// with connector's authentication and connection pooling.
    async fn get_sample_rows(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        limit: usize,
    ) -> ConnectorV2Result<Vec<SampleRow>> {
        // Default implementation: use stream_data with small batch
        let mut stream = self
            .stream_data(source, credentials, table_name, limit)
            .await?;

        // Get first batch only
        use futures::StreamExt;
        if let Some(batch_result) = stream.next().await {
            batch_result
        } else {
            Ok(vec![])
        }
    }

    /// Estimate row count for a table
    ///
    /// Returns approximate row count without full table scan.
    /// Uses database statistics when available.
    async fn estimate_row_count(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
    ) -> ConnectorV2Result<Option<u64>> {
        // Default implementation: no estimate available
        let _ = (source, credentials, table_name);
        Ok(None)
    }

    /// Check if connector supports V2 streaming
    fn supports_streaming(&self) -> bool {
        self.capabilities().streaming
    }
}

/// Export format options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportFormat {
    /// Comma-separated values
    Csv,
    /// Line-delimited JSON
    JsonLines,
    /// JSON array
    JsonArray,
    /// Apache Parquet (columnar)
    Parquet,
    /// Apache Arrow IPC
    Arrow,
}

/// Configuration for data export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Maximum number of rows to export (None = all)
    pub max_rows: Option<usize>,

    /// Include column headers (for CSV/JSON)
    pub include_headers: bool,

    /// Compression format
    pub compression: Option<CompressionFormat>,

    /// CSV-specific options
    pub csv_options: Option<CsvExportOptions>,

    /// Parquet-specific options
    pub parquet_options: Option<ParquetExportOptions>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            max_rows: None,
            include_headers: true,
            compression: None,
            csv_options: None,
            parquet_options: None,
        }
    }
}

/// Compression format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionFormat {
    Gzip,
    Zstd,
    Snappy,
    Lz4,
}

/// CSV export options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvExportOptions {
    /// Delimiter character (default: ',')
    pub delimiter: u8,
    /// Quote character (default: '"')
    pub quote: u8,
    /// Escape character
    pub escape: Option<u8>,
    /// Line terminator (default: "\n")
    pub line_terminator: String,
}

impl Default for CsvExportOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: b'"',
            escape: None,
            line_terminator: "\n".to_string(),
        }
    }
}

/// Parquet export options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParquetExportOptions {
    /// Row group size
    pub row_group_size: usize,
    /// Enable statistics
    pub enable_statistics: bool,
    /// Compression codec for columns
    pub compression: Option<String>,
}

impl Default for ParquetExportOptions {
    fn default() -> Self {
        Self {
            row_group_size: 50_000,
            enable_statistics: true,
            compression: Some("snappy".to_string()),
        }
    }
}

/// Adapter to help existing connectors implement V2
///
/// Provides default implementations that delegate to the base connector
/// where possible. Connectors can override specific methods for better
/// performance.
#[async_trait]
pub trait DataSourceConnectorV2Adapter: DataSourceConnectorV2 {
    /// Helper: Convert old SchemaDefinition to UnifiedSchema
    ///
    /// Implementations should override this to properly map their
    /// schema types to UnifiedSchema.
    fn schema_definition_to_unified(
        &self,
        source: &DataSource,
        schema_def: super::api_types::SchemaDefinition,
    ) -> ConnectorV2Result<UnifiedSchema>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format() {
        let format = ExportFormat::Csv;
        assert_eq!(format, ExportFormat::Csv);

        let json = serde_json::to_string(&format).unwrap();
        assert!(json.contains("Csv"));
    }

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert!(config.include_headers);
        assert!(config.max_rows.is_none());
        assert!(config.compression.is_none());
    }

    #[test]
    fn test_csv_options_default() {
        let opts = CsvExportOptions::default();
        assert_eq!(opts.delimiter, b',');
        assert_eq!(opts.quote, b'"');
        assert_eq!(opts.line_terminator, "\n");
    }

    #[test]
    fn test_parquet_options_default() {
        let opts = ParquetExportOptions::default();
        assert_eq!(opts.row_group_size, 50_000);
        assert!(opts.enable_statistics);
        assert_eq!(opts.compression, Some("snappy".to_string()));
    }

    #[test]
    fn test_compression_format() {
        let compression = CompressionFormat::Gzip;
        assert_eq!(compression, CompressionFormat::Gzip);

        let json = serde_json::to_string(&compression).unwrap();
        assert!(json.contains("Gzip"));
    }
}
