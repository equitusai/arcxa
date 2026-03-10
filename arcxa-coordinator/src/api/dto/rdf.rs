//! RDF DTOs
//!
//! Request and response types for RDF store operations, auto-save stats, and manual saves.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// RDF Auto-Save Stats DTOs
// =============================================================================

#[derive(Serialize, ToSchema)]
pub struct RdfAutoSaveStatsResponse {
    /// Unix timestamp of last save
    pub last_save_time: u64,
    /// Total number of auto-saves performed
    pub auto_save_count: u64,
    /// Number of failed auto-save attempts
    pub auto_save_failures: u64,
    /// Seconds since last successful save
    pub seconds_since_last_save: Option<u64>,
    /// Human-readable last save time
    pub last_save_formatted: Option<String>,
    /// Health status (true if recent save)
    pub healthy: bool,
    /// Status message
    pub message: String,
    /// Response timestamp
    pub timestamp: DateTime<Utc>,
}

// =============================================================================
// RDF Save DTOs
// =============================================================================

#[derive(Serialize, ToSchema)]
pub struct RdfSaveResponse {
    /// Save operation success status
    pub success: bool,
    /// Number of RDF quads saved to disk
    pub quads_saved: usize,
    /// Save operation duration in milliseconds
    pub duration_ms: u128,
    /// Result message
    pub message: String,
    /// Response timestamp
    pub timestamp: DateTime<Utc>,
}
