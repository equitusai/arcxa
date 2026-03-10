//! # Discovery Type Definitions
//!
//! Core types for intelligent schema discovery.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Discovered schema for a data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSchema {
    /// Source ID
    pub source_id: String,

    /// Schema/database name
    pub schema_name: String,

    /// Discovered tables
    pub tables: Vec<DiscoveredTable>,

    /// Relationships between discovered tables
    #[serde(default)]
    pub relationships: Vec<DiscoveredRelationship>,

    /// Timestamp when discovered (Unix epoch seconds)
    pub discovered_at: i64,
}

impl DiscoveredSchema {
    /// Check if schema is expired based on TTL
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        (now - self.discovered_at) > ttl_secs as i64
    }
}

/// Discovered table with intelligent type inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTable {
    /// Table name
    pub name: String,

    /// Discovered columns with inferred types
    pub columns: Vec<DiscoveredColumn>,

    /// Estimated row count (from system catalogs)
    pub row_count: Option<u64>,
}

/// Discovered relationship between tables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredRelationship {
    /// Optional relationship/constraint name
    pub name: Option<String>,

    /// Source (child) table name
    pub source_table: String,

    /// Source column names
    pub source_columns: Vec<String>,

    /// Target (parent) table name
    pub target_table: String,

    /// Target column names
    pub target_columns: Vec<String>,
}

/// Discovered column with semantic type inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredColumn {
    /// Column name
    pub name: String,

    /// SQL data type
    pub data_type: String,

    /// Nullable flag
    pub nullable: bool,

    /// Primary key flag
    pub primary_key: bool,

    /// Inferred semantic type (email, phone, name, etc.)
    pub semantic_type: Option<String>,

    /// Confidence score for semantic type (0.0 - 1.0)
    pub confidence: f64,

    /// Detected patterns (email regex, phone regex, etc.)
    pub patterns: Vec<DetectedPattern>,

    /// Column statistics
    pub statistics: ColumnStatistics,

    /// Sample values (up to 10)
    pub sample_values: Vec<String>,
}

/// Detected pattern in column values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Pattern type (email, phone, ssn, uuid, etc.)
    pub pattern_type: String,

    /// Confidence/match rate (0.0 - 1.0)
    pub match_rate: f64,

    /// Example value matching pattern
    pub example: Option<String>,
}

/// Column statistics from profiling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStatistics {
    /// Number of distinct values
    pub distinct_count: i64,

    /// Null fraction (0.0 - 1.0)
    pub null_fraction: f64,

    /// Sample count used for statistics
    pub sample_count: usize,

    /// Most common values (if available)
    pub most_common_values: Option<Vec<String>>,

    /// Average length (for string types)
    pub avg_length: Option<f64>,

    /// Min/max values (for numeric types)
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}

impl Default for ColumnStatistics {
    fn default() -> Self {
        Self {
            distinct_count: 0,
            null_fraction: 0.0,
            sample_count: 0,
            most_common_values: None,
            avg_length: None,
            min_value: None,
            max_value: None,
        }
    }
}

/// Schema metadata extracted from system catalogs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Schema name
    pub schema_name: String,

    /// Tables in schema
    pub tables: Vec<TableMetadata>,

    /// Relationship metadata discovered from system catalogs
    #[serde(default)]
    pub relationships: Vec<TableRelationshipMetadata>,
}

/// Table metadata from system catalogs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Table name
    pub name: String,

    /// Columns in table
    pub columns: Vec<ColumnMetadata>,

    /// Estimated row count (from statistics)
    pub estimated_rows: Option<u64>,
}

/// Relationship metadata extracted from system catalogs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRelationshipMetadata {
    /// Optional relationship/constraint name
    pub name: Option<String>,

    /// Source table name
    pub source_table: String,

    /// Source columns
    pub source_columns: Vec<String>,

    /// Target table name
    pub target_table: String,

    /// Target columns
    pub target_columns: Vec<String>,
}

/// Column metadata from INFORMATION_SCHEMA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMetadata {
    /// Column name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Nullable flag
    pub nullable: bool,

    /// Default value
    pub default_value: Option<String>,

    /// Primary key flag
    pub primary_key: bool,
}

/// Sample row from table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRow {
    /// Column name -> value mapping
    pub values: HashMap<String, String>,
}

/// Column statistics from system tables
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ColumnStats {
    /// Number of distinct values
    pub distinct_count: i64,

    /// Fraction of null values
    pub null_fraction: f64,

    /// Most common values (comma-separated)
    pub most_common_values: Option<String>,
}

/// Discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Schema filter (e.g., "public")
    pub schema_filter: Option<String>,

    /// Table filter (e.g., "customers")
    pub table_filter: Option<String>,

    /// Sample size for value extraction
    pub sample_size: usize,

    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
}

impl DiscoveryConfig {
    /// Generate cache key
    pub fn cache_key(&self) -> String {
        format!(
            "schema:{}_table:{}_sample:{}",
            self.schema_filter.as_deref().unwrap_or("*"),
            self.table_filter.as_deref().unwrap_or("*"),
            self.sample_size
        )
    }
}

/// Type inference result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResult {
    /// Inferred semantic type
    pub semantic_type: Option<String>,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Detected patterns
    pub detected_patterns: Vec<DetectedPattern>,

    /// Column statistics
    pub statistics: ColumnStatistics,
}
