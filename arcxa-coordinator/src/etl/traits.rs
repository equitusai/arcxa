//! Core ETL Traits
//!
//! This module defines the fundamental trait boundaries for the ETL system.
//! These traits enable composition of format readers, transformers, and destinations
//! into flexible data pipelines.

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

// Re-export for convenience
pub use crate::etl::errors::EtlError;

/// A single data record with optional schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRecord {
    /// Record values as JSON
    pub data: Value,

    /// Optional schema metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<RecordSchema>,

    /// Source location (line number, byte offset, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,

    /// Record metadata (timestamps, versions, etc.)
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

/// Source location information for debugging and lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub line: Option<u64>,
    pub byte_offset: Option<u64>,
    pub partition: Option<String>,
}

/// Schema information for a record or dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    pub metadata: HashMap<String, Value>,
}

/// Schema for a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub description: Option<String>,
    pub metadata: HashMap<String, Value>,
}

/// Supported data types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    BigInt,
    Float,
    Double,
    Decimal {
        precision: u8,
        scale: u8,
    },
    Boolean,
    Date,
    DateTime,
    Time,
    Binary,
    Json,
    Array(Box<DataType>),
    Map {
        key: Box<DataType>,
        value: Box<DataType>,
    },
}

// ============================================================================
// Format Reader Trait
// ============================================================================

/// Trait for reading data from various formats
///
/// Implementations should handle format-specific parsing and provide
/// a uniform stream of DataRecords.
#[async_trait]
pub trait FormatReader: Send + Sync {
    /// Read records as an async stream
    async fn read_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<DataRecord>> + Send>>, EtlError>;

    /// Infer schema from the data source
    async fn infer_schema(&self) -> Result<RecordSchema, EtlError>;

    /// Get format-specific statistics
    async fn get_stats(&self) -> Result<FormatStats, EtlError>;

    /// Validate the format and return any issues
    async fn validate(&self) -> Result<ValidationReport, EtlError>;

    /// Get reader capabilities
    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities::default()
    }
}

/// Format-specific statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatStats {
    pub total_records: Option<u64>,
    pub total_bytes: Option<u64>,
    pub format_name: String,
    pub compression: Option<String>,
    pub metadata: HashMap<String, Value>,
}

/// Format reader capabilities
#[derive(Debug, Clone, Default)]
pub struct FormatCapabilities {
    pub supports_schema_inference: bool,
    pub supports_partitioning: bool,
    pub supports_pushdown_filters: bool,
    pub supports_projection: bool,
    pub is_streaming: bool,
}

/// Validation report for format checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
}

// ============================================================================
// Data Extractor Trait
// ============================================================================

/// Trait for extracting data from sources (databases, APIs, etc.)
///
/// Unlike FormatReader, extractors handle connection management and
/// query execution rather than file parsing.
#[async_trait]
pub trait DataExtractor: Send + Sync {
    /// Extract records as a stream
    async fn extract_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<DataRecord>> + Send>>, EtlError>;

    /// Get extraction metadata
    async fn get_metadata(&self) -> Result<ExtractionMetadata, EtlError>;

    /// Test the connection/availability
    async fn test_connection(&self) -> Result<(), EtlError>;
}

/// Metadata about an extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionMetadata {
    pub source_type: String,
    pub source_name: String,
    pub estimated_records: Option<u64>,
    pub extraction_timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, Value>,
}

// ============================================================================
// Data Destination Trait
// ============================================================================

/// Configuration for data loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    pub mode: LoadMode,
    pub batch_size: usize,
    pub key_fields: Vec<String>,
    pub parallelism: usize,
    pub error_tolerance: ErrorTolerance,
    pub checkpoint_interval: Option<Duration>,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            mode: LoadMode::Insert,
            batch_size: 1000,
            key_fields: Vec::new(),
            parallelism: 1,
            error_tolerance: ErrorTolerance::default(),
            checkpoint_interval: None,
        }
    }
}

/// How to handle existing data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadMode {
    Insert,  // Fail on duplicates
    Upsert,  // Update on conflict
    Replace, // Truncate then insert
    Append,  // Always append
    Merge,   // Merge based on keys
}

impl std::fmt::Display for LoadMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadMode::Insert => write!(f, "INSERT"),
            LoadMode::Upsert => write!(f, "UPSERT"),
            LoadMode::Replace => write!(f, "REPLACE"),
            LoadMode::Append => write!(f, "APPEND"),
            LoadMode::Merge => write!(f, "MERGE"),
        }
    }
}

/// Error tolerance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTolerance {
    pub max_errors: usize,
    pub skip_on_error: bool,
    pub error_percentage_threshold: Option<f32>,
}

impl Default for ErrorTolerance {
    fn default() -> Self {
        Self {
            max_errors: 0,
            skip_on_error: false,
            error_percentage_threshold: None,
        }
    }
}

/// Trait for data destinations
#[async_trait]
pub trait DataDestination: Send + Sync {
    /// Prepare the destination (create tables, validate schema, etc.)
    async fn prepare(&mut self, schema: &RecordSchema, config: &LoadConfig)
        -> Result<(), EtlError>;

    /// Load a stream of records
    async fn load_stream(
        &mut self,
        records: Pin<Box<dyn Stream<Item = Result<DataRecord>> + Send>>,
        config: &LoadConfig,
    ) -> Result<LoadStats, EtlError>;

    /// Finalize the load (commit, create indexes, etc.)
    async fn finalize(&mut self) -> Result<(), EtlError>;

    /// Rollback changes on error
    async fn rollback(&mut self) -> Result<(), EtlError>;

    /// Get destination capabilities
    fn capabilities(&self) -> DestinationCapabilities;
}

/// Capabilities of a destination
#[derive(Debug, Clone)]
pub struct DestinationCapabilities {
    pub supports_transactions: bool,
    pub supports_bulk_load: bool,
    pub supports_upsert: bool,
    pub supports_merge: bool,
    pub supports_streaming: bool,
    pub max_batch_size: Option<usize>,
    pub preferred_batch_size: usize,
}

impl Default for DestinationCapabilities {
    fn default() -> Self {
        Self {
            supports_transactions: false,
            supports_bulk_load: false,
            supports_upsert: false,
            supports_merge: false,
            supports_streaming: false,
            max_batch_size: None,
            preferred_batch_size: 1000,
        }
    }
}

/// Statistics from a load operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadStats {
    pub records_read: u64,
    pub records_loaded: u64,
    pub records_updated: u64,
    pub records_failed: u64,
    pub bytes_processed: u64,
    pub duration_ms: u64,
    pub destination_specific: HashMap<String, Value>,
}

impl LoadStats {
    pub fn new() -> Self {
        Self {
            records_read: 0,
            records_loaded: 0,
            records_updated: 0,
            records_failed: 0,
            bytes_processed: 0,
            duration_ms: 0,
            destination_specific: HashMap::new(),
        }
    }
}

// ============================================================================
// Transformer Trait
// ============================================================================

/// Trait for data transformations
#[async_trait]
pub trait Transformer: Send + Sync {
    /// Transform a single record
    async fn transform(&self, record: DataRecord) -> Result<TransformResult, EtlError>;

    /// Get output schema after transformation
    fn output_schema(&self, input: &RecordSchema) -> Result<RecordSchema, EtlError>;

    /// Validate transformation configuration
    fn validate(&self) -> Result<(), EtlError>;

    /// Get transformer metadata
    fn metadata(&self) -> TransformerMetadata {
        TransformerMetadata::default()
    }
}

/// Result of a transformation
#[derive(Debug)]
pub enum TransformResult {
    /// Record passed through (possibly modified)
    Record(DataRecord),
    /// Record split into multiple
    Multiple(Vec<DataRecord>),
    /// Record filtered out
    Filtered { reason: String },
    /// Transformation error (may be recoverable)
    Error(EtlError),
}

/// Metadata about a transformer
#[derive(Debug, Clone, Default)]
pub struct TransformerMetadata {
    pub name: String,
    pub version: String,
    pub preserves_order: bool,
    pub is_stateful: bool,
    pub estimated_overhead_percent: Option<f32>,
}

// ============================================================================
// Pipeline Executor Trait
// ============================================================================

/// Trait for pipeline execution strategies
#[async_trait]
pub trait PipelineExecutor: Send + Sync {
    /// Execute the pipeline with given components
    async fn execute(
        &mut self,
        source: Box<dyn FormatReader>,
        transformers: Vec<Box<dyn Transformer>>,
        destination: Box<dyn DataDestination>,
        config: &PipelineConfig,
    ) -> Result<PipelineStats, EtlError>;

    /// Cancel a running pipeline
    async fn cancel(&mut self) -> Result<(), EtlError>;

    /// Get current execution status
    fn status(&self) -> PipelineStatus;
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub description: Option<String>,
    pub parallelism: usize,
    pub buffer_size: usize,
    pub checkpoint_interval: Option<Duration>,
    pub metrics_enabled: bool,
    pub tracing_enabled: bool,
    pub load_config: LoadConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            name: "unnamed_pipeline".to_string(),
            description: None,
            parallelism: 1,
            buffer_size: 1000,
            checkpoint_interval: None,
            metrics_enabled: true,
            tracing_enabled: false,
            load_config: LoadConfig::default(),
        }
    }
}

/// Pipeline execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatus {
    NotStarted,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Pipeline execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStats {
    pub status: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: u64,
    pub records_read: u64,
    pub records_transformed: u64,
    pub records_loaded: u64,
    pub records_failed: u64,
    pub bytes_processed: u64,
    pub errors: Vec<String>,
    pub checkpoints_created: u32,
    pub stage_stats: Vec<StageStats>,
}

/// Statistics for a pipeline stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageStats {
    pub stage_name: String,
    pub stage_type: String,
    pub records_in: u64,
    pub records_out: u64,
    pub duration_ms: u64,
    pub errors: u32,
}

// ============================================================================
// Factory Traits
// ============================================================================

/// Factory for creating format readers
pub trait FormatReaderFactory: Send + Sync {
    /// Create a format reader from configuration
    fn create(&self, config: &Value) -> Result<Box<dyn FormatReader>, EtlError>;

    /// List supported formats
    fn supported_formats(&self) -> Vec<String>;
}

/// Factory for creating data destinations
pub trait DataDestinationFactory: Send + Sync {
    /// Create a destination from configuration
    fn create(&self, config: &Value) -> Result<Box<dyn DataDestination>, EtlError>;

    /// List supported destination types
    fn supported_types(&self) -> Vec<String>;
}

/// Factory for creating transformers
pub trait TransformerFactory: Send + Sync {
    /// Create a transformer from configuration
    fn create(&self, config: &Value) -> Result<Box<dyn Transformer>, EtlError>;

    /// List supported transformer types
    fn supported_types(&self) -> Vec<String>;
}
