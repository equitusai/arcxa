//! # Structured Data Source Abstraction
//!
//! Unified interface for reading structured data from any source:
//! - CSV files (from file library)
//! - Database tables (PostgreSQL, DB2, Oracle)
//! - Parquet files
//! - Streaming sources (Kafka, external APIs)
//!
//! ## Architecture Principle
//!
//! **All non-streaming data MUST come from the file library first.**
//! This ensures:
//! - Lineage tracking
//! - Metadata consistency
//! - Access control
//! - Schema caching
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::data_source::{StructuredDataSource, FileLibrarySource};
//!
//! // Create source from file library (enforced pattern)
//! let source = FileLibrarySource::new(file_library, "file_abc123").await?;
//!
//! // Get schema (abstracted across all source types)
//! let schema = source.schema().await?;
//!
//! // Read records (abstracted across all source types)
//! let mut records = source.records().await?;
//! while let Some(record) = records.next().await? {
//!     // Process record
//! }
//! ```

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Core Abstractions
// ============================================================================

/// Source type discriminator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceType {
    /// CSV file from file library
    FileLibraryCsv { file_id: String },

    /// Database table
    Database {
        source_id: String,
        table_name: String,
        schema: Option<String>,
    },

    /// Parquet file from file library
    FileLibraryParquet { file_id: String },

    /// Streaming source (external, not from file library)
    StreamingExternal {
        stream_uri: String,
        format: StreamFormat,
    },
}

/// Streaming format for external sources
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StreamFormat {
    Csv,
    Json,
    Avro,
    Protobuf,
}

/// Universal schema representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSchema {
    /// Source type and location
    pub source_type: SourceType,

    /// Field definitions
    pub fields: Vec<FieldDefinition>,

    /// Estimated row count (if available)
    pub estimated_rows: Option<u64>,

    /// Metadata about the source
    pub metadata: HashMap<String, String>,
}

/// Field definition in source schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name
    pub name: String,

    /// Data type
    pub data_type: UniversalDataType,

    /// Whether field can be null
    pub nullable: bool,

    /// Sample values (for profiling)
    pub sample_values: Vec<String>,

    /// Field position (0-indexed)
    pub position: usize,
}

/// Universal data type (maps across CSV, SQL, Parquet)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UniversalDataType {
    String,
    Integer,
    BigInt,
    Float,
    Double,
    Decimal { precision: u32, scale: u32 },
    Boolean,
    Date,
    Timestamp,
    Time,
    Binary,
    Json,
    Unknown,
}

/// Record from any structured source
#[derive(Debug, Clone)]
pub struct SourceRecord {
    /// Field values (by field name)
    pub fields: HashMap<String, SourceValue>,

    /// Record position (for lineage tracking)
    pub position: u64,
}

/// Value from any structured source
#[derive(Debug, Clone)]
pub enum SourceValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

impl SourceValue {
    /// Convert to string representation
    pub fn to_string(&self) -> String {
        match self {
            SourceValue::String(s) => s.clone(),
            SourceValue::Integer(i) => i.to_string(),
            SourceValue::Float(f) => f.to_string(),
            SourceValue::Boolean(b) => b.to_string(),
            SourceValue::Null => String::new(),
        }
    }
}

// ============================================================================
// Trait: Structured Data Source
// ============================================================================

/// Unified interface for reading structured data
///
/// ## Implementation Requirements
///
/// All implementations MUST:
/// - Provide schema before reading records
/// - Track record positions for lineage
/// - Handle errors gracefully
/// - Be thread-safe (Send + Sync)
#[async_trait]
pub trait StructuredDataSource: Send + Sync {
    /// Get the source schema
    ///
    /// This should be fast (cached if possible) as it may be called multiple times.
    async fn schema(&self) -> Result<SourceSchema>;

    /// Create a record stream for reading data
    ///
    /// This returns a new stream each time called (supports multiple concurrent reads).
    async fn records(&self) -> Result<Box<dyn RecordStream>>;

    /// Get source type
    fn source_type(&self) -> SourceType;

    /// Get human-readable description
    fn description(&self) -> String;
}

/// Stream of records from a data source
///
/// Async iterator pattern for reading records one-by-one.
#[async_trait]
pub trait RecordStream: Send {
    /// Get next record (None = end of stream)
    async fn next(&mut self) -> Result<Option<SourceRecord>>;

    /// Get current position in stream
    fn position(&self) -> u64;

    /// Close the stream and release resources
    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// Implementation: File Library CSV Source
// ============================================================================

/// CSV source from file library (enforces architecture principle)
pub struct FileLibraryCsvSource {
    file_id: String,
    file_path: String,
    schema: Arc<SourceSchema>,
}

impl FileLibraryCsvSource {
    /// Create source from file library
    ///
    /// ## Architecture Enforcement
    ///
    /// This is the ONLY way to create a CSV source. Direct path access is not allowed.
    ///
    /// ## Arguments
    ///
    /// - `file_library`: Reference to file library storage (trait object)
    /// - `file_id`: File ID in the library
    ///
    /// ## Errors
    ///
    /// - File not found in library
    /// - File is not a CSV
    /// - Schema detection failed
    pub async fn new(
        file_library: &dyn crate::api::file_library::storage_trait::FileLibraryStore,
        file_id: &str,
    ) -> Result<Self> {
        use crate::api::file_library::scanner::FileScanner;
        use crate::api::file_library::types::ScanFileRequest;

        // Get file from library
        let file = file_library
            .get_file(file_id)
            .context("Failed to get file from library")?
            .ok_or_else(|| anyhow::anyhow!("File not found in library: {}", file_id))?;

        // Validate file type (infer from file path extension)
        let file_ext = file
            .file_path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        if file_ext != "csv" && file_ext != "tsv" && file_ext != "txt" {
            anyhow::bail!(
                "File is not a CSV/TSV (extension: {}): {}",
                file_ext,
                file_id
            );
        }

        let file_path = file.file_path.clone();

        // Scan file for schema (use cached schema if available)
        let schema = if let Some(cached_schema) = file.schema {
            // Use cached schema from file library
            Self::convert_cached_schema(file_id, cached_schema)?
        } else {
            // Scan file for schema
            let scanner = FileScanner::new();
            let scan_result = scanner.scan_file(
                &file_path,
                ScanFileRequest {
                    delimiter: None,
                    encoding: None,
                    has_header: None,
                    sample_rows: Some(1000),
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )?;

            Self::convert_scanned_schema(file_id, scan_result)?
        };

        Ok(Self {
            file_id: file_id.to_string(),
            file_path,
            schema: Arc::new(schema),
        })
    }

    /// Create source from file library with explicit path (internal use)
    ///
    /// **INTERNAL USE ONLY** - Used when file library reference is already validated.
    pub(crate) fn from_validated_path(
        file_id: String,
        file_path: String,
        schema: SourceSchema,
    ) -> Self {
        Self {
            file_id,
            file_path,
            schema: Arc::new(schema),
        }
    }

    /// Convert cached schema from file library
    fn convert_cached_schema(
        file_id: &str,
        cached: crate::api::file_library::types::FileSchema,
    ) -> Result<SourceSchema> {
        let fields = cached
            .fields
            .into_iter()
            .enumerate()
            .map(|(idx, field)| FieldDefinition {
                name: field.name,
                data_type: Self::map_field_type(&field.field_type),
                nullable: field.nullable,
                sample_values: field.sample_values,
                position: idx,
            })
            .collect();

        Ok(SourceSchema {
            source_type: SourceType::FileLibraryCsv {
                file_id: file_id.to_string(),
            },
            fields,
            estimated_rows: Some(cached.total_rows),
            metadata: HashMap::new(),
        })
    }

    /// Convert scanned schema
    fn convert_scanned_schema(
        file_id: &str,
        scan: crate::api::file_library::types::ScanResult,
    ) -> Result<SourceSchema> {
        let fields = scan
            .detected_fields
            .into_iter()
            .enumerate()
            .map(|(idx, field)| FieldDefinition {
                name: field.name,
                data_type: Self::map_field_type(&field.field_type),
                nullable: field.nullable,
                sample_values: field.sample_values,
                position: idx,
            })
            .collect();

        Ok(SourceSchema {
            source_type: SourceType::FileLibraryCsv {
                file_id: file_id.to_string(),
            },
            fields,
            estimated_rows: scan.total_rows,
            metadata: HashMap::new(),
        })
    }

    /// Map file library field type to universal data type
    fn map_field_type(ft: &crate::api::file_library::types::FieldType) -> UniversalDataType {
        use crate::api::file_library::types::FieldType;
        match ft {
            FieldType::String => UniversalDataType::String,
            FieldType::Integer => UniversalDataType::Integer,
            FieldType::Float => UniversalDataType::Float,
            FieldType::Boolean => UniversalDataType::Boolean,
            FieldType::Timestamp => UniversalDataType::Timestamp,
            FieldType::Date => UniversalDataType::Date,
        }
    }
}

#[async_trait]
impl StructuredDataSource for FileLibraryCsvSource {
    async fn schema(&self) -> Result<SourceSchema> {
        Ok((*self.schema).clone())
    }

    async fn records(&self) -> Result<Box<dyn RecordStream>> {
        Ok(Box::new(
            CsvRecordStream::new(&self.file_path, self.schema.clone()).await?,
        ))
    }

    fn source_type(&self) -> SourceType {
        SourceType::FileLibraryCsv {
            file_id: self.file_id.clone(),
        }
    }

    fn description(&self) -> String {
        format!("CSV file from library: {}", self.file_id)
    }
}

// ============================================================================
// CSV Record Stream
// ============================================================================

/// Record stream for CSV files
struct CsvRecordStream {
    reader: crate::mapping::loader::orchestration::async_csv_reader::AsyncCsvReader<
        tokio_util::compat::Compat<tokio::io::BufReader<tokio::fs::File>>,
    >,
    schema: Arc<SourceSchema>,
    position: u64,
    headers: Vec<String>,
}

impl CsvRecordStream {
    async fn new(file_path: &str, schema: Arc<SourceSchema>) -> Result<Self> {
        use crate::mapping::loader::orchestration::async_csv_reader::{
            AsyncCsvReader, AsyncCsvReaderConfig,
        };

        let config = AsyncCsvReaderConfig {
            file_path: std::path::PathBuf::from(file_path),
            delimiter: b',',
            has_header: true,
            buffer_size: 8192,
            ..Default::default()
        };

        let mut reader = AsyncCsvReader::new(config)
            .await
            .context("Failed to open CSV file")?;

        let headers = reader.headers().clone();

        Ok(Self {
            reader,
            schema,
            position: 0,
            headers,
        })
    }
}

#[async_trait]
impl RecordStream for CsvRecordStream {
    async fn next(&mut self) -> Result<Option<SourceRecord>> {
        match self.reader.next_row().await? {
            Some(record) => {
                let mut fields = HashMap::new();

                for (idx, header) in self.headers.iter().enumerate() {
                    let value = record
                        .get(idx)
                        .map(|s| {
                            if s.is_empty() {
                                SourceValue::Null
                            } else {
                                // Try to parse based on schema type
                                let field_type = self
                                    .schema
                                    .fields
                                    .get(idx)
                                    .map(|f| &f.data_type)
                                    .unwrap_or(&UniversalDataType::String);

                                match field_type {
                                    UniversalDataType::Integer | UniversalDataType::BigInt => s
                                        .parse::<i64>()
                                        .map(SourceValue::Integer)
                                        .unwrap_or_else(|_| SourceValue::String(s.to_string())),
                                    UniversalDataType::Float | UniversalDataType::Double => s
                                        .parse::<f64>()
                                        .map(SourceValue::Float)
                                        .unwrap_or_else(|_| SourceValue::String(s.to_string())),
                                    UniversalDataType::Boolean => match s.to_lowercase().as_str() {
                                        "true" | "1" | "yes" => SourceValue::Boolean(true),
                                        "false" | "0" | "no" => SourceValue::Boolean(false),
                                        _ => SourceValue::String(s.to_string()),
                                    },
                                    _ => SourceValue::String(s.to_string()),
                                }
                            }
                        })
                        .unwrap_or(SourceValue::Null);

                    fields.insert(header.clone(), value);
                }

                self.position += 1;

                Ok(Some(SourceRecord {
                    fields,
                    position: self.position,
                }))
            }
            None => Ok(None),
        }
    }

    fn position(&self) -> u64 {
        self.position
    }
}

// ============================================================================
// Implementation: Streaming External Source
// ============================================================================

/// External streaming source (exception to file library rule)
pub struct StreamingExternalSource {
    stream_uri: String,
    format: StreamFormat,
    // TODO: Add actual streaming implementation (Kafka, HTTP, etc.)
}

impl StreamingExternalSource {
    /// Create streaming source
    ///
    /// **Note**: This is an EXCEPTION to the file library rule.
    /// Only use for true external streaming sources (Kafka, APIs, etc.)
    pub fn new(stream_uri: String, format: StreamFormat) -> Self {
        Self { stream_uri, format }
    }
}

#[async_trait]
impl StructuredDataSource for StreamingExternalSource {
    async fn schema(&self) -> Result<SourceSchema> {
        // TODO: Implement schema detection for streaming sources
        anyhow::bail!("Streaming source schema detection not yet implemented")
    }

    async fn records(&self) -> Result<Box<dyn RecordStream>> {
        // TODO: Implement streaming record reader
        anyhow::bail!("Streaming source reading not yet implemented")
    }

    fn source_type(&self) -> SourceType {
        SourceType::StreamingExternal {
            stream_uri: self.stream_uri.clone(),
            format: self.format.clone(),
        }
    }

    fn description(&self) -> String {
        format!("Streaming source: {} ({:?})", self.stream_uri, self.format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_value_to_string() {
        assert_eq!(SourceValue::String("test".to_string()).to_string(), "test");
        assert_eq!(SourceValue::Integer(42).to_string(), "42");
        assert_eq!(SourceValue::Float(3.14).to_string(), "3.14");
        assert_eq!(SourceValue::Boolean(true).to_string(), "true");
        assert_eq!(SourceValue::Null.to_string(), "");
    }

    #[test]
    fn test_source_type_serialization() {
        let source_type = SourceType::FileLibraryCsv {
            file_id: "file_123".to_string(),
        };

        let json = serde_json::to_string(&source_type).unwrap();
        let deserialized: SourceType = serde_json::from_str(&json).unwrap();

        assert_eq!(source_type, deserialized);
    }
}
