//! Profiling API Types
//!
//! Request and response DTOs for profiling endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request to profile a dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDatasetRequest {
    /// Source file path or URI (CSV, Parquet)
    pub source: String,

    /// File format (csv, parquet)
    #[serde(default = "default_format")]
    pub format: String,

    /// Maximum number of rows to sample (None = full scan)
    pub sample_size: Option<usize>,

    /// Whether to detect semantic types (email, phone, etc.)
    #[serde(default = "default_true")]
    pub detect_semantic_types: bool,

    /// Whether to infer relationships (FK candidates)
    #[serde(default)]
    pub infer_relationships: bool,

    /// Store profile in RDF store
    #[serde(default = "default_true")]
    pub store_in_rdf: bool,
}

fn default_format() -> String {
    "csv".to_string()
}

fn default_true() -> bool {
    true
}

/// Response from profiling operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDatasetResponse {
    /// Dataset URI (RDF identifier)
    pub dataset_uri: String,

    /// Local dataset ID
    pub dataset_id: String,

    /// Number of rows profiled
    pub rows_profiled: u64,

    /// Number of columns
    pub column_count: usize,

    /// File size in bytes
    pub file_size_bytes: u64,

    /// Profiling duration (seconds)
    pub duration_seconds: f64,

    /// Candidate primary keys
    pub candidate_keys: Vec<String>,

    /// Profiling timestamp
    pub profiled_at: DateTime<Utc>,

    /// RDF graph URI where profile is stored
    pub graph_uri: Option<String>,

    /// Link to full profile
    pub profile_link: String,
}

/// Response containing full profile details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProfileResponse {
    /// Dataset URI
    pub dataset_uri: String,

    /// Dataset ID
    pub dataset_id: String,

    /// Source location
    pub source_location: String,

    /// File format
    pub format: String,

    /// File size
    pub file_size_bytes: u64,

    /// Total rows (if known)
    pub total_rows: Option<u64>,

    /// Rows profiled
    pub rows_profiled: u64,

    /// Column count
    pub column_count: usize,

    /// Per-column profiles
    pub columns: Vec<ColumnProfileDto>,

    /// Candidate keys
    pub candidate_keys: Vec<String>,

    /// Profiled at
    pub profiled_at: DateTime<Utc>,

    /// Duration
    pub duration_seconds: f64,

    /// RDF representation (Turtle format)
    pub rdf_turtle: Option<String>,
}

/// Column profile DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnProfileDto {
    pub name: String,
    pub index: usize,
    pub data_type: String,
    pub semantic_type: Option<String>,
    pub null_count: u64,
    pub null_percentage: f64,
    pub distinct_count: u64,
    pub cardinality: f64,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub mean: Option<f64>,
    pub median: Option<f64>,
    pub std_dev: Option<f64>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub avg_length: Option<f64>,
    pub pattern_example: Option<String>,
    pub top_values: Vec<ValueFrequencyDto>,
}

/// Value frequency DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueFrequencyDto {
    pub value: String,
    pub count: u64,
    pub percentage: f64,
}

/// List profiles response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProfilesResponse {
    pub profiles: Vec<ProfileSummaryDto>,
    pub total_count: usize,
}

/// Profile summary DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummaryDto {
    pub dataset_uri: String,
    pub dataset_id: String,
    pub source_location: String,
    pub format: String,
    pub rows_profiled: u64,
    pub column_count: usize,
    pub profiled_at: DateTime<Utc>,
}
