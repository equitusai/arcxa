//! Data Export (Data Portability)
//!
//! Implements GDPR Article 20 (Right to data portability).
//!
//! This is a placeholder module that will be fully implemented in Phase 3.

use super::types::DataSubjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Export Format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// JSON format (structured, machine-readable)
    Json,
    /// CSV format (tabular data)
    Csv,
    /// XML format
    Xml,
    /// Human-readable PDF report
    Pdf,
}

/// Export Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Unique identifier for this export request
    pub request_id: String,
    /// The data subject requesting export
    pub data_subject: DataSubjectId,
    /// Desired export format
    pub format: ExportFormat,
    /// When the export was requested
    pub requested_at: DateTime<Utc>,
}

/// Export Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// The original request
    pub request: ExportRequest,
    /// Whether the export succeeded
    pub success: bool,
    /// Path or URL to the exported data
    pub export_location: Option<String>,
    /// Size of the export in bytes
    pub size_bytes: u64,
}

/// Data Export Trait
///
/// Storage backends implement this trait to provide data portability.
/// Full implementation will be added in Phase 3.
pub trait DataExport: Send + Sync {
    // Placeholder - will be implemented in Phase 3
}
