//! WAL (Write-Ahead Log) DTOs
//!
//! Request and response types for WAL operations, status, and replay.

use chrono::{DateTime, Utc};
use serde::Serialize;

// =============================================================================
// WAL Status DTOs
// =============================================================================

#[derive(Serialize)]
pub struct WalStatusResponse {
    pub healthy: bool,
    pub total_entries: usize,
    pub uncommitted_entries: usize,
    pub wal_enabled: bool,
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

// =============================================================================
// WAL Operations DTOs
// =============================================================================

#[derive(Serialize)]
pub struct WalOperation {
    pub op_id: String,
    pub timestamp: DateTime<Utc>,
    pub operation_type: String,
    pub committed: bool,
}

#[derive(Serialize)]
pub struct WalOperationsResponse {
    pub operations: Vec<WalOperation>,
    pub total_count: usize,
    pub uncommitted_count: usize,
}

// =============================================================================
// WAL Replay DTOs
// =============================================================================

#[derive(Serialize)]
pub struct WalReplayResponse {
    pub success: bool,
    pub replayed_count: usize,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}
