//! Dataset Import DTOs
//!
//! Request and response types for dataset import operations with automatic lineage tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Import Request/Response DTOs
// =============================================================================

/// Metadata for dataset import (sent as JSON in multipart form)
#[derive(Debug, Deserialize)]
pub struct ImportMetadata {
    /// Dataset name
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional schema definition (if not provided, will be auto-detected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaDefinition>,
}

// =============================================================================
// Datasource Import DTOs
// =============================================================================

/// Request to import data from a connected datasource
#[derive(Debug, Deserialize)]
pub struct DatasourceImportRequest {
    /// Datasource ID (from catalog)
    pub source_id: String,

    /// Table/collection name to import
    pub table: String,

    /// Optional name for the materialized dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional WHERE clause for filtering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,

    /// Optional columns to select (default: all columns)
    #[serde(default)]
    pub columns: Vec<String>,

    /// Limit rows (for sampling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Enable data quality profiling (default: false)
    #[serde(default)]
    pub profile: bool,

    /// Return immediately for async processing (default: false for < 10k rows)
    #[serde(default)]
    pub async_mode: bool,

    /// Incremental import settings (for future use)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<IncrementalImportConfig>,
}

/// Batch datasource import request
#[derive(Debug, Deserialize)]
pub struct BatchDatasourceImportRequest {
    /// Datasource ID (from catalog)
    pub source_id: String,

    /// Tables to import
    pub tables: Vec<BatchTableImport>,

    /// Common tags for all imports
    #[serde(default)]
    pub tags: Vec<String>,

    /// Enable profiling for all tables (default: false)
    #[serde(default)]
    pub profile: bool,
}

/// Single table in batch import
#[derive(Debug, Deserialize)]
pub struct BatchTableImport {
    /// Table name
    pub table: String,

    /// Optional dataset name (defaults to table name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional WHERE clause
    #[serde(skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,

    /// Optional column selection
    #[serde(default)]
    pub columns: Vec<String>,

    /// Optional limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Batch import response
#[derive(Debug, serde::Serialize)]
pub struct BatchImportResponse {
    /// Batch import ID
    pub batch_id: String,

    /// Individual import IDs
    pub import_ids: Vec<String>,

    /// Status
    pub status: String,

    /// Started at
    pub started_at: String,
}

/// Incremental import configuration
#[derive(Debug, Deserialize, Clone)]
pub struct IncrementalImportConfig {
    /// Enabled flag
    pub enabled: bool,

    /// Column to track (e.g., "updated_at", "id")
    pub tracking_column: String,

    /// Last imported value (for resume)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_value: Option<serde_json::Value>,
}

/// Schema definition for import
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SchemaDefinition {
    /// Primary key column name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_key: Option<String>,

    /// Column definitions
    pub columns: Vec<ColumnDefinition>,
}

/// Column definition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// Response after successful import
#[derive(Debug, Serialize)]
pub struct ImportDatasetResponse {
    /// Generated dataset ID
    pub dataset_id: String,

    /// Dataset name
    pub name: String,

    /// Import status
    pub status: ImportStatus,

    /// Number of records imported
    pub record_count: u64,

    /// File size in bytes
    pub file_size_bytes: u64,

    /// Detected/provided schema
    pub schema: SchemaDefinition,

    /// Lineage metadata
    pub lineage: ImportLineage,

    /// Storage information
    pub storage: StorageInfo,
}

/// Import status enum
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImportStatus {
    Pending,
    Processing,
    Imported,
    CompletedWithErrors,
    Failed,
}

/// Lineage metadata for import
#[derive(Debug, Serialize, Clone)]
pub struct ImportLineage {
    /// Import method (always "file_upload" for this endpoint)
    pub import_method: String,

    /// Original source filename
    pub source_file: String,

    /// User who performed the import
    pub imported_by: String,

    /// Import timestamp (ISO 8601)
    pub imported_at: String,

    /// Import ID for tracking
    pub import_id: String,
}

/// Storage information
#[derive(Debug, Serialize, Clone)]
pub struct StorageInfo {
    /// Storage format (always "parquet" for efficiency)
    pub format: String,

    /// Path to stored data
    pub path: String,

    /// Whether data is compressed
    pub compressed: bool,
}

// =============================================================================
// Import Status DTOs
// =============================================================================

/// Get import status response
#[derive(Debug, Serialize)]
pub struct ImportStatusResponse {
    /// Import ID
    pub import_id: String,

    /// Current status
    pub status: ImportStatus,

    /// Progress percentage (0-100)
    pub progress: u8,

    /// Dataset ID (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,

    /// Start timestamp
    pub started_at: String,

    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    /// Records successfully processed
    pub records_processed: u64,

    /// Records that failed validation
    pub records_failed: u64,

    /// Error messages (if any)
    #[serde(default)]
    pub errors: Vec<ImportError>,
}

/// Import error detail
#[derive(Debug, Serialize, Clone)]
pub struct ImportError {
    /// Row number (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<u64>,

    /// Column name (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,

    /// Error message
    pub message: String,

    /// Error code
    pub code: String,
}

// =============================================================================
// List Imports DTOs
// =============================================================================

/// List imports query parameters
#[derive(Debug, Deserialize)]
pub struct ListImportsQuery {
    /// Filter by status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ImportStatus>,

    /// Page number (0-indexed)
    #[serde(default)]
    pub page: usize,

    /// Items per page
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

fn default_page_size() -> usize {
    20
}

/// List imports response
#[derive(Debug, Serialize)]
pub struct ListImportsResponse {
    pub imports: Vec<ImportSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

/// Import summary for list view
#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub import_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_name: Option<String>,

    pub status: ImportStatus,
    pub record_count: u64,
    pub imported_by: String,
    pub imported_at: String,
}

// =============================================================================
// Error Response
// =============================================================================

/// Import error response
#[derive(Debug, Serialize)]
pub struct ImportErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_metadata_deserialize() {
        let json = r#"{
            "name": "Test Dataset",
            "description": "Test import",
            "tags": ["test", "demo"],
            "schema": {
                "primary_key": "id",
                "columns": [
                    {"name": "id", "data_type": "INTEGER", "nullable": false}
                ]
            }
        }"#;

        let metadata: ImportMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.name, "Test Dataset");
        assert_eq!(metadata.tags.len(), 2);
        assert!(metadata.schema.is_some());
    }

    #[test]
    fn test_import_status_serialization() {
        assert_eq!(
            serde_json::to_string(&ImportStatus::Imported).unwrap(),
            "\"imported\""
        );
        assert_eq!(
            serde_json::to_string(&ImportStatus::CompletedWithErrors).unwrap(),
            "\"completedwitherrors\""
        );
    }
}
