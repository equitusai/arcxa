//! Temporal DTOs
//!
//! Request and response types for temporal index operations, checkpoints, and time-travel queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// Checkpoint DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct CheckpointRequest {
    pub path: String,
}

#[derive(Serialize)]
pub struct CheckpointResponse {
    pub success: bool,
    pub checkpoint_path: String,
    pub timestamp: DateTime<Utc>,
}

// =============================================================================
// Temporal Summary DTOs
// =============================================================================

#[derive(Serialize)]
pub struct TemporalSummaryResponse {
    // WAL Status
    pub wal_healthy: bool,
    pub wal_uncommitted_ops: usize,

    // Temporal Index Stats
    pub total_versions: usize,
    pub cache_hit_rate: f64,

    // RDF Store Stats
    pub total_triples: usize,

    // Health
    pub overall_healthy: bool,
    pub timestamp: DateTime<Utc>,
}
