//! Lineage DTOs
//!
//! Request and response types for lineage tracking and impact analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// Lineage Query Requests
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct LineageQueryRequest {
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub dataset: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ModelImpactQuery {
    pub version: String,
}

// =============================================================================
// Time-Travel & Impact Analysis Query Parameters
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct AsOfQuery {
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ForwardImpactQuery {
    pub source: String, // Format: "system:path"
    pub as_of: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct BackwardAnalysisQuery {
    pub record_id: String,
    pub as_of: Option<DateTime<Utc>>,
}

// =============================================================================
// Lineage Responses
// =============================================================================

#[derive(Serialize)]
pub struct LineageResponse {
    pub record_id: String,
    pub events: Vec<serde_json::Value>,
    pub total_count: usize,
}

#[derive(Serialize)]
pub struct WriteLineageResponse {
    pub success: bool,
    pub count: usize,
}

#[derive(Serialize)]
pub struct ModelImpactResponse {
    pub model_id: String,
    pub version: String,
    pub affected_records: u64,
    pub datasets: Vec<String>,
    pub events: Vec<serde_json::Value>,
}

#[derive(Serialize)]
pub struct EntityLineageResponse {
    pub entity_id: String,
    pub lineage_graph: Vec<serde_json::Value>,
    pub format: String,
}

#[derive(Serialize)]
pub struct ModelLineageGraph {
    pub model_id: String,
    pub impacted_records: Vec<String>,
    pub lineage_depth: usize,
}
