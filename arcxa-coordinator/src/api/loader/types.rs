//! Loader API Data Transfer Objects
//!
//! Request and response types for the ETL loader REST API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use utoipa::ToSchema;

// ============================================================================
// Job Management DTOs
// ============================================================================

/// Create loader job request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateLoaderJobRequest {
    /// Job name/identifier
    pub name: String,

    /// Source file from file library (enforces architecture)
    ///
    /// **Architecture Change (2024)**: This field now requires a file library ID
    /// instead of a direct path. This ensures all data goes through the file library.
    ///
    /// **Migration Path**: Upload file to file library first via `POST /api/v1/file-library/files`,
    /// then use the returned file_id here.
    pub source_file_id: String,

    /// DEPRECATED: Direct file path (legacy support only)
    ///
    /// **WARNING**: This field bypasses the file library architecture and should not be used.
    /// It will be removed in a future version. Use `source_file_id` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[deprecated(
        since = "2024.0.0",
        note = "Use source_file_id with file library instead"
    )]
    #[schema(value_type = Option<String>)]
    pub source_file_path: Option<PathBuf>,

    /// Target database configuration
    pub target_config: TargetDatabaseConfig,

    /// Column mappings (source column → target column)
    pub column_mappings: Vec<ColumnMappingDto>,

    /// Transformation expressions
    pub transformations: Option<Vec<TransformationDto>>,

    /// Loader configuration
    pub loader_config: Option<LoaderConfigDto>,

    /// Checkpoint configuration
    pub checkpoint_config: Option<CheckpointConfigDto>,

    /// DLQ configuration
    pub dlq_config: Option<DlqConfigDto>,
}

/// Target database configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TargetDatabaseConfig {
    /// Database type
    pub db_type: DatabaseType,

    /// Database host
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Target table name
    pub table: String,

    /// Username
    pub username: String,

    /// Password (should use secrets management in production)
    pub password: String,

    /// Additional connection options
    pub options: Option<HashMap<String, String>>,
}

/// Database type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    DB2,
    PostgreSQL,
}

/// Column mapping DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnMappingDto {
    /// Source column name or index
    pub source: String,

    /// Target column name
    pub target: String,

    /// Whether NULL values are allowed
    #[serde(default = "default_true")]
    pub nullable: bool,

    /// Default value if source is NULL
    pub default_value: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Transformation DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransformationDto {
    /// Target column to apply transformation to
    pub target_column: String,

    /// Transformation expression (e.g., "UPPER(TRIM({value}))")
    pub expression: String,
}

/// Loader configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoaderConfigDto {
    /// Batch size for processing
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Use DB2 LOAD utility (faster) vs INSERT
    #[serde(default = "default_true")]
    pub use_load_utility: bool,

    /// LOAD utility buffer size (KB)
    #[serde(default = "default_load_buffer_size")]
    pub load_buffer_kb: usize,

    /// LOAD utility parallelism
    #[serde(default = "default_load_parallelism")]
    pub load_parallelism: usize,

    /// Automatically create target table if it doesn't exist
    /// Uses DDL generation from CSV schema and executes via DDL API
    #[serde(default)]
    pub auto_create_table: bool,

    /// SHACL shape URI for DDL generation (optional)
    /// If not provided, will infer schema from CSV headers
    pub shacl_uri: Option<String>,
}

fn default_batch_size() -> usize {
    1000
}

fn default_max_connections() -> usize {
    4
}

fn default_load_buffer_size() -> usize {
    4096
}

fn default_load_parallelism() -> usize {
    4
}

/// Checkpoint configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckpointConfigDto {
    /// Checkpoint interval (rows)
    #[serde(default = "default_checkpoint_interval")]
    pub interval_rows: u64,

    /// Maximum errors before aborting
    #[serde(default = "default_max_errors")]
    pub max_errors: usize,

    /// Maximum retries for transient errors
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
}

fn default_checkpoint_interval() -> u64 {
    10000
}

fn default_max_errors() -> usize {
    100
}

fn default_max_retries() -> usize {
    3
}

/// DLQ configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DlqConfigDto {
    /// DLQ output format
    #[serde(default)]
    pub format: DlqFormatDto,

    /// Organize by error category
    #[serde(default = "default_true")]
    pub organize_by_category: bool,

    /// Organize by date
    #[serde(default = "default_true")]
    pub organize_by_date: bool,
}

/// DLQ format DTO
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DlqFormatDto {
    Csv,
    Json,
    JsonLines,
}

impl Default for DlqFormatDto {
    fn default() -> Self {
        DlqFormatDto::JsonLines
    }
}

/// Create loader job response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateLoaderJobResponse {
    /// Job ID
    pub job_id: String,

    /// Job status
    pub status: JobStatus,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Message
    pub message: String,
}

/// Loader job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// ============================================================================
// Job Query/Status DTOs
// ============================================================================

/// Get job status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobStatusResponse {
    /// Job ID
    pub job_id: String,

    /// Job name
    pub name: String,

    /// Current status
    pub status: JobStatus,

    /// Progress information
    pub progress: JobProgressDto,

    /// Checkpoint information
    pub checkpoint: Option<CheckpointStatusDto>,

    /// DLQ statistics
    pub dlq_stats: Option<DlqStatsDto>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Started timestamp
    pub started_at: Option<DateTime<Utc>>,

    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message (if failed)
    pub error_message: Option<String>,
}

/// Job progress DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobProgressDto {
    /// Current row number
    pub current_row: u64,

    /// Total rows (if known)
    pub total_rows: Option<u64>,

    /// Rows processed successfully
    pub rows_processed: u64,

    /// Rows failed
    pub rows_failed: u64,

    /// Rows skipped
    pub rows_skipped: u64,

    /// Progress percentage (0-100)
    pub progress_percent: f64,

    /// Estimated time remaining (seconds)
    pub estimated_time_remaining: Option<f64>,

    /// Rows per second
    pub rows_per_second: f64,
}

/// Checkpoint status DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckpointStatusDto {
    /// Current row
    pub current_row: u64,

    /// File offset
    pub file_offset: u64,

    /// Last checkpoint timestamp
    pub last_checkpoint: DateTime<Utc>,

    /// Checkpoint state
    pub state: String,

    /// Error summary
    pub error_summary: ErrorSummaryDto,
}

/// Error summary DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorSummaryDto {
    /// Total errors
    pub total_errors: usize,

    /// Errors by category
    pub errors_by_category: HashMap<String, usize>,

    /// Recent errors (last 10)
    pub recent_errors: Vec<ErrorRecordDto>,
}

/// Error record DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorRecordDto {
    /// Row number
    pub row_number: u64,

    /// Error category
    pub category: String,

    /// Error message
    pub message: String,

    /// Retry count
    pub retry_count: usize,

    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// DLQ statistics DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DlqStatsDto {
    /// Total failed rows in DLQ
    pub total_rows: u64,

    /// Rows by error category
    pub rows_by_category: HashMap<String, u64>,

    /// First error timestamp
    pub first_error: Option<DateTime<Utc>>,

    /// Last error timestamp
    pub last_error: Option<DateTime<Utc>>,

    /// DLQ file paths
    pub dlq_files: Vec<String>,
}

/// List jobs response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListJobsResponse {
    /// Jobs
    pub jobs: Vec<JobSummaryDto>,

    /// Total count
    pub total_count: usize,
}

/// Job summary DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobSummaryDto {
    /// Job ID
    pub job_id: String,

    /// Job name
    pub name: String,

    /// Status
    pub status: JobStatus,

    /// Rows processed
    pub rows_processed: u64,

    /// Rows failed
    pub rows_failed: u64,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Job Control DTOs
// ============================================================================

/// Resume job request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResumeJobRequest {
    /// Whether to force resume even if not in failed state
    #[serde(default)]
    pub force: bool,
}

/// Resume job response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResumeJobResponse {
    /// Job ID
    pub job_id: String,

    /// Status after resume
    pub status: JobStatus,

    /// Message
    pub message: String,

    /// Resume point
    pub resume_from_row: u64,
}

/// Cancel job response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CancelJobResponse {
    /// Job ID
    pub job_id: String,

    /// Status after cancel
    pub status: JobStatus,

    /// Message
    pub message: String,
}

// ============================================================================
// DLQ Query DTOs
// ============================================================================

/// Get DLQ rows request (query parameters)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetDlqRowsQuery {
    /// Error category filter
    pub category: Option<String>,

    /// Limit number of rows
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Get DLQ rows response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetDlqRowsResponse {
    /// Failed rows
    pub rows: Vec<FailedRowDto>,

    /// Total count
    pub total_count: usize,

    /// Returned count
    pub returned_count: usize,
}

/// Failed row DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FailedRowDto {
    /// Row number in source file
    pub row_number: u64,

    /// Original row data
    pub row_data: Vec<String>,

    /// Error category
    pub error_category: String,

    /// Error message
    pub error_message: String,

    /// Retry count
    pub retry_count: usize,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Reprocess DLQ rows request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReprocessDlqRequest {
    /// Category filter (reprocess only this category)
    pub category: Option<String>,

    /// Row numbers to reprocess (if specific rows)
    pub row_numbers: Option<Vec<u64>>,

    /// Maximum rows to reprocess
    #[serde(default = "default_reprocess_limit")]
    pub limit: usize,
}

fn default_reprocess_limit() -> usize {
    1000
}

/// Reprocess DLQ rows response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReprocessDlqResponse {
    /// Job ID
    pub job_id: String,

    /// Rows attempted
    pub rows_attempted: usize,

    /// Rows succeeded
    pub rows_succeeded: usize,

    /// Rows still failing
    pub rows_still_failing: usize,

    /// Message
    pub message: String,
}

// ============================================================================
// Health/Stats DTOs
// ============================================================================

/// Loader health response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoaderHealthResponse {
    /// Health status
    pub status: HealthStatus,

    /// Active jobs count
    pub active_jobs: usize,

    /// Pending jobs count
    pub pending_jobs: usize,

    /// Failed jobs count (last 24h)
    pub failed_jobs_24h: usize,

    /// Total rows processed (last 24h)
    pub rows_processed_24h: u64,

    /// Average throughput (rows/sec, last 24h)
    pub avg_throughput: f64,

    /// Components health
    pub components: HashMap<String, HealthStatus>,
}

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}
