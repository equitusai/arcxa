//! Profiling Types
//!
//! Data structures for dataset profiling results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

/// Configuration for source profiling
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProfileConfig {
    /// Maximum number of rows to sample (None = full scan)
    pub sample_size: Option<usize>,

    /// Whether to detect semantic types (email, phone, SSN, etc.)
    pub detect_semantic_types: bool,

    /// Whether to infer foreign key relationships
    pub infer_relationships: bool,

    /// Minimum cardinality ratio to consider a column as candidate key
    pub candidate_key_threshold: f64,

    /// Whether to generate pattern examples for string columns
    pub generate_pattern_examples: bool,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            sample_size: Some(10_000), // Sample first 10K rows by default
            detect_semantic_types: true,
            infer_relationships: false,    // Expensive, opt-in
            candidate_key_threshold: 0.95, // 95% unique values
            generate_pattern_examples: true,
        }
    }
}

/// Result of profiling a dataset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProfileResult {
    /// Dataset identifier (file path or URI)
    pub dataset_id: String,

    /// Original source location
    pub source_location: String,

    /// File format (csv, parquet, json)
    pub format: String,

    /// File size in bytes
    pub file_size_bytes: u64,

    /// Total number of rows (if known)
    pub total_rows: Option<u64>,

    /// Number of rows actually profiled
    pub rows_profiled: u64,

    /// Number of columns
    pub column_count: usize,

    /// Per-column profile information
    pub columns: Vec<ColumnProfile>,

    /// Candidate primary key columns (high cardinality, low nulls)
    pub candidate_keys: Vec<String>,

    /// Profiling timestamp
    pub profiled_at: DateTime<Utc>,

    /// Profiling duration (seconds)
    pub duration_seconds: f64,
}

/// Profile information for a single column
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnProfile {
    /// Column name
    pub name: String,

    /// Column index (0-based)
    pub index: usize,

    /// Inferred data type
    pub data_type: DataType,

    /// Semantic type (if detected)
    pub semantic_type: Option<SemanticType>,

    /// Number of null/empty values
    pub null_count: u64,

    /// Percentage of null values (0.0 - 1.0)
    pub null_percentage: f64,

    /// Number of distinct values (HyperLogLog estimate)
    pub distinct_count: u64,

    /// Cardinality ratio (distinct / total)
    pub cardinality: f64,

    /// Minimum value (for numeric/date types)
    pub min_value: Option<String>,

    /// Maximum value (for numeric/date types)
    pub max_value: Option<String>,

    /// Mean value (for numeric types)
    pub mean: Option<f64>,

    /// Median value (for numeric types)
    pub median: Option<f64>,

    /// Standard deviation (for numeric types)
    pub std_dev: Option<f64>,

    /// Minimum string length (for string types)
    pub min_length: Option<usize>,

    /// Maximum string length (for string types)
    pub max_length: Option<usize>,

    /// Average string length (for string types)
    pub avg_length: Option<f64>,

    /// Example pattern (for string types)
    pub pattern_example: Option<String>,

    /// Regex pattern detected (for string types)
    pub pattern_regex: Option<String>,

    /// Top 10 most frequent values
    pub top_values: Vec<ValueFrequency>,
}

/// Inferred data type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    DateTime,
    Time,
    Json,
    Unknown,
}

impl DataType {
    /// Convert to XSD datatype URI
    pub fn to_xsd_uri(&self) -> &'static str {
        match self {
            DataType::String => "http://www.w3.org/2001/XMLSchema#string",
            DataType::Integer => "http://www.w3.org/2001/XMLSchema#integer",
            DataType::Float => "http://www.w3.org/2001/XMLSchema#decimal",
            DataType::Boolean => "http://www.w3.org/2001/XMLSchema#boolean",
            DataType::Date => "http://www.w3.org/2001/XMLSchema#date",
            DataType::DateTime => "http://www.w3.org/2001/XMLSchema#dateTime",
            DataType::Time => "http://www.w3.org/2001/XMLSchema#time",
            DataType::Json => "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON",
            DataType::Unknown => "http://www.w3.org/2001/XMLSchema#string",
        }
    }
}

/// Semantic type detected through pattern analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticType {
    Email,
    Phone,
    Ssn,
    CreditCard,
    Zipcode,
    IpAddress,
    Url,
    Uuid,
    CountryCode,
    CurrencyCode,
    Custom,
}

/// Value frequency for top-N analysis
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValueFrequency {
    pub value: String,
    pub count: u64,
    pub percentage: f64,
}

/// Dataset URI reference
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetUri {
    /// Full URI (e.g., "http://graphica.io/dataset/customers_csv")
    pub uri: String,

    /// Local identifier (e.g., "customers_csv")
    pub local_id: String,

    /// Named graph where profile is stored
    pub graph: String,
}

impl DatasetUri {
    /// Create new dataset URI from file path
    pub fn from_path(path: &Path) -> Self {
        let local_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .replace(".", "_")
            .replace(" ", "_")
            .to_lowercase();

        Self {
            uri: format!("http://graphica.io/dataset/{}", local_id),
            local_id,
            graph: "http://graphica.io/profiles".to_string(),
        }
    }
}
