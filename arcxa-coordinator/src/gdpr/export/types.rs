//! GDPR Data Export Types
//!
//! Implements data portability per GDPR Article 20.
//! This module provides types for requesting, tracking, and delivering user data exports.
//!
//! ## Architecture
//!
//! The export system leverages existing Graphica infrastructure:
//! - **RocksDB**: Export job state storage (hot tier)
//! - **Lineage System**: Discover all data associated with a user
//! - **RDF Store**: Query governance metadata and data classifications
//! - **File Library**: Store generated export files
//! - **Audit System**: Track all export operations for compliance
//!
//! ## Workflow
//!
//! 1. User requests export via API
//! 2. Export job created in RocksDB
//! 3. Data discovery phase:
//!    - Query lineage store for user's data footprint
//!    - Query RDF store for data classifications and retention policies
//!    - Query row/column lineage for granular data tracking
//! 4. Data aggregation phase:
//!    - Collect data from all discovered sources
//!    - Apply GDPR filters (exclude legal hold data, etc.)
//! 5. Format conversion phase:
//!    - Convert to requested format (JSON/CSV/XML/PDF)
//! 6. Storage phase:
//!    - Store in File Library with expiry
//!    - Generate secure download URL
//! 7. Notification phase:
//!    - Update job status
//!    - Send notification (future: email/webhook)

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

// ============================================================================
// Export Request Types
// ============================================================================

/// Data export request submitted by user or admin
///
/// This represents the initial request for data portability under GDPR Article 20.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportRequest {
    /// User ID whose data to export
    pub user_id: String,

    /// Requested export format
    pub format: ExportFormat,

    /// Data categories to include (empty = all)
    #[serde(default)]
    pub categories: Vec<DataCategory>,

    /// Include derived/inferred data (ML predictions, analytics)
    #[serde(default)]
    pub include_derived: bool,

    /// Include system metadata (timestamps, IDs, versions)
    #[serde(default)]
    pub include_metadata: bool,

    /// Include audit trail (who accessed/modified data)
    #[serde(default)]
    pub include_audit_trail: bool,

    /// Time range filter (optional)
    pub time_range: Option<TimeRange>,

    /// Additional filters (dataset names, systems, etc.)
    #[serde(default)]
    pub filters: HashMap<String, String>,
}

/// Export format options
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// JSON format (structured, machine-readable)
    Json,

    /// CSV format (tabular, spreadsheet-compatible)
    Csv,

    /// XML format (structured, legacy systems)
    Xml,

    /// PDF format (human-readable, archival)
    Pdf,
}

impl ExportFormat {
    pub fn extension(&self) -> &str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Xml => "xml",
            ExportFormat::Pdf => "pdf",
        }
    }

    pub fn mime_type(&self) -> &str {
        match self {
            ExportFormat::Json => "application/json",
            ExportFormat::Csv => "text/csv",
            ExportFormat::Xml => "application/xml",
            ExportFormat::Pdf => "application/pdf",
        }
    }
}

/// Data categories for GDPR classification
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DataCategory {
    /// Personal identifiable information (name, email, phone)
    PersonalIdentifiers,

    /// Contact information (address, phone, email)
    ContactInfo,

    /// Financial data (transactions, payments, billing)
    Financial,

    /// Health information (medical records, fitness data)
    Health,

    /// Location data (GPS, IP addresses, check-ins)
    Location,

    /// Behavioral data (browsing, clicks, usage patterns)
    Behavioral,

    /// Demographic data (age, gender, ethnicity)
    Demographic,

    /// Professional data (employment, education, skills)
    Professional,

    /// Social data (connections, interactions, posts)
    Social,

    /// Technical data (device info, cookies, logs)
    Technical,

    /// Custom category
    Custom(String),
}

/// Time range filter for export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeRange {
    /// Start time (inclusive)
    pub start: DateTime<Utc>,

    /// End time (inclusive)
    pub end: DateTime<Utc>,
}

// ============================================================================
// Export Job Types
// ============================================================================

/// Export job represents an ongoing or completed export operation
///
/// Jobs are stored in RocksDB for fast lookup and status tracking.
/// Large jobs may take minutes to hours, so async processing is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    /// Unique job ID
    pub id: Uuid,

    /// User ID whose data is being exported
    pub user_id: String,

    /// User who requested the export (may be admin)
    pub requested_by: String,

    /// Original request
    pub request: ExportRequest,

    /// Current job status
    pub status: ExportStatus,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Completion timestamp (when status became Ready/Failed)
    pub completed_at: Option<DateTime<Utc>>,

    /// Progress information
    pub progress: ExportProgress,

    /// Result (when completed)
    pub result: Option<ExportResult>,

    /// Error information (when failed)
    pub error: Option<ExportError>,

    /// Expiry time for download (typically 48 hours after completion)
    pub expires_at: Option<DateTime<Utc>>,
}

impl ExportJob {
    /// Create a new export job from a request
    pub fn new(user_id: String, requested_by: String, request: ExportRequest) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            requested_by,
            request,
            status: ExportStatus::Pending,
            created_at: now,
            updated_at: now,
            completed_at: None,
            progress: ExportProgress::default(),
            result: None,
            error: None,
            expires_at: None,
        }
    }

    /// Mark job as processing
    pub fn start_processing(&mut self) {
        self.status = ExportStatus::Processing;
        self.updated_at = Utc::now();
    }

    /// Update progress
    pub fn update_progress(&mut self, phase: ExportPhase, percent: u8, message: Option<String>) {
        self.progress.current_phase = Some(phase);
        self.progress.percent_complete = percent;
        if let Some(msg) = message {
            self.progress.status_message = Some(msg);
        }
        self.updated_at = Utc::now();
    }

    /// Mark job as completed successfully
    pub fn complete(&mut self, result: ExportResult, expiry_hours: i64) {
        let now = Utc::now();
        self.status = ExportStatus::Ready;
        self.completed_at = Some(now);
        self.expires_at = Some(now + Duration::hours(expiry_hours));
        self.result = Some(result);
        self.progress.percent_complete = 100;
        self.updated_at = now;
    }

    /// Mark job as failed
    pub fn fail(&mut self, error: ExportError) {
        let now = Utc::now();
        self.status = ExportStatus::Failed;
        self.completed_at = Some(now);
        self.error = Some(error);
        self.updated_at = now;
    }

    /// Check if job has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
}

/// Export job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    /// Job created, waiting to be processed
    Pending,

    /// Currently processing
    Processing,

    /// Export ready for download
    Ready,

    /// Job failed with error
    Failed,

    /// Export expired (past download window)
    Expired,

    /// Job cancelled by user/admin
    Cancelled,
}

/// Export processing phases
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportPhase {
    /// Discovering data locations
    Discovery,

    /// Collecting data from sources
    Collection,

    /// Converting to requested format
    Conversion,

    /// Storing export file
    Storage,

    /// Generating download URL
    Finalization,
}

/// Export progress tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportProgress {
    /// Current processing phase
    pub current_phase: Option<ExportPhase>,

    /// Percent complete (0-100)
    pub percent_complete: u8,

    /// Human-readable status message
    pub status_message: Option<String>,

    /// Statistics
    pub stats: ExportStats,
}

/// Export statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportStats {
    /// Number of data sources scanned
    pub sources_scanned: usize,

    /// Number of records found
    pub records_found: usize,

    /// Number of records included in export
    pub records_exported: usize,

    /// Number of records excluded (legal hold, retention, etc.)
    pub records_excluded: usize,

    /// Size of export file in bytes
    pub file_size_bytes: usize,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

// ============================================================================
// Export Result Types
// ============================================================================

/// Export result when job completes successfully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// Path to export file in File Library
    pub file_path: String,

    /// Secure download URL (time-limited)
    pub download_url: String,

    /// File size in bytes
    pub file_size_bytes: usize,

    /// SHA-256 checksum for integrity verification
    pub checksum: String,

    /// Export metadata
    pub metadata: ExportMetadata,
}

/// Export metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// Format of the export
    pub format: ExportFormat,

    /// Number of records included
    pub record_count: usize,

    /// Data categories included
    pub categories: Vec<DataCategory>,

    /// Data sources included
    pub sources: Vec<DataSource>,

    /// Time range covered
    pub time_range: Option<TimeRange>,

    /// Generation timestamp
    pub generated_at: DateTime<Utc>,
}

/// Data source information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataSource {
    /// Source system name
    pub system: String,

    /// Dataset/table name
    pub dataset: String,

    /// Number of records from this source
    pub record_count: usize,

    /// Data categories in this source
    pub categories: Vec<DataCategory>,
}

/// Export error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportError {
    /// Error code
    pub code: ExportErrorCode,

    /// Human-readable error message
    pub message: String,

    /// Technical details (for debugging)
    pub details: Option<String>,

    /// Timestamp of error
    pub occurred_at: DateTime<Utc>,
}

/// Export error codes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportErrorCode {
    /// User not found in system
    UserNotFound,

    /// No data found for user
    NoDataFound,

    /// Data source unavailable
    SourceUnavailable,

    /// Format conversion failed
    ConversionFailed,

    /// File storage failed
    StorageFailed,

    /// Export too large (exceeds limits)
    ExportTooLarge,

    /// Internal system error
    InternalError,

    /// Permission denied
    PermissionDenied,

    /// Request timeout
    Timeout,
}

// ============================================================================
// API Response Types
// ============================================================================

/// Response when export is requested
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportRequestResponse {
    /// Job ID for tracking
    pub job_id: Uuid,

    /// Current status
    pub status: ExportStatus,

    /// Message to user
    pub message: String,

    /// Estimated completion time (if available)
    pub estimated_completion: Option<DateTime<Utc>>,
}

/// Response when querying export status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportStatusResponse {
    /// Job ID
    pub job_id: Uuid,

    /// Current status
    pub status: ExportStatus,

    /// Progress information
    pub progress: Option<ExportProgressInfo>,

    /// Download URL (when ready)
    pub download_url: Option<String>,

    /// Expiry time (when ready)
    pub expires_at: Option<DateTime<Utc>>,

    /// Error information (when failed)
    pub error: Option<ExportErrorInfo>,
}

/// Progress information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportProgressInfo {
    /// Current phase
    pub phase: Option<ExportPhase>,

    /// Percent complete (0-100)
    pub percent_complete: u8,

    /// Status message
    pub message: Option<String>,
}

/// Error information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportErrorInfo {
    /// Error code
    pub code: ExportErrorCode,

    /// Error message
    pub message: String,
}

// ============================================================================
// Storage Keys (for RocksDB)
// ============================================================================

/// Key prefixes for RocksDB storage
pub mod storage_keys {
    use uuid::Uuid;

    /// Prefix for export jobs: "export_job:{job_id}"
    pub fn job(job_id: &Uuid) -> String {
        format!("export_job:{}", job_id)
    }

    /// Prefix for user's export jobs index: "user_exports:{user_id}:{timestamp}"
    pub fn user_index(user_id: &str, timestamp: i64) -> String {
        format!("user_exports:{}:{}", user_id, timestamp)
    }

    /// Prefix for status index: "exports_by_status:{status}:{job_id}"
    pub fn status_index(status: &str, job_id: &Uuid) -> String {
        format!("exports_by_status:{}:{}", status, job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_extensions() {
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Xml.extension(), "xml");
        assert_eq!(ExportFormat::Pdf.extension(), "pdf");
    }

    #[test]
    fn test_export_format_mime_types() {
        assert_eq!(ExportFormat::Json.mime_type(), "application/json");
        assert_eq!(ExportFormat::Csv.mime_type(), "text/csv");
        assert_eq!(ExportFormat::Xml.mime_type(), "application/xml");
        assert_eq!(ExportFormat::Pdf.mime_type(), "application/pdf");
    }

    #[test]
    fn test_export_job_lifecycle() {
        let request = ExportRequest {
            user_id: "user123".to_string(),
            format: ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: false,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let mut job = ExportJob::new("user123".to_string(), "user123".to_string(), request);

        assert_eq!(job.status, ExportStatus::Pending);

        job.start_processing();
        assert_eq!(job.status, ExportStatus::Processing);

        job.update_progress(
            ExportPhase::Discovery,
            25,
            Some("Scanning data sources".to_string()),
        );
        assert_eq!(job.progress.percent_complete, 25);

        let result = ExportResult {
            file_path: "/exports/user123.json".to_string(),
            download_url: "https://example.com/download/abc123".to_string(),
            file_size_bytes: 1024,
            checksum: "abc123".to_string(),
            metadata: ExportMetadata {
                format: ExportFormat::Json,
                record_count: 100,
                categories: vec![],
                sources: vec![],
                time_range: None,
                generated_at: Utc::now(),
            },
        };

        job.complete(result, 48);
        assert_eq!(job.status, ExportStatus::Ready);
        assert_eq!(job.progress.percent_complete, 100);
        assert!(job.completed_at.is_some());
        assert!(job.expires_at.is_some());
    }

    #[test]
    fn test_job_expiry() {
        let request = ExportRequest {
            user_id: "user123".to_string(),
            format: ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: false,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let mut job = ExportJob::new("user123".to_string(), "user123".to_string(), request);

        // Set expiry to past
        job.expires_at = Some(Utc::now() - Duration::hours(1));
        assert!(job.is_expired());

        // Set expiry to future
        job.expires_at = Some(Utc::now() + Duration::hours(1));
        assert!(!job.is_expired());

        // No expiry set
        job.expires_at = None;
        assert!(!job.is_expired());
    }

    #[test]
    fn test_storage_keys() {
        let job_id = Uuid::new_v4();
        let user_id = "user123";
        let timestamp = 1234567890;

        assert_eq!(storage_keys::job(&job_id), format!("export_job:{}", job_id));
        assert_eq!(
            storage_keys::user_index(user_id, timestamp),
            "user_exports:user123:1234567890"
        );
        assert_eq!(
            storage_keys::status_index("pending", &job_id),
            format!("exports_by_status:pending:{}", job_id)
        );
    }
}
