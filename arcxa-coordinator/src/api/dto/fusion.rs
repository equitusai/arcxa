//! Fusion DTOs
//!
//! Request and response types for entity fusion and resolution operations.

use serde::{Deserialize, Serialize};

// =============================================================================
// Fusion Proposal DTOs
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ProposeFusionRequest {
    pub dataset: String,
    pub rule: String,
    pub min_confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ProposeFusionResponse {
    pub candidates: Vec<FusionCandidate>,
    pub total_count: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct FusionCandidate {
    pub candidate_id: String,
    pub entities: Vec<serde_json::Map<String, serde_json::Value>>,
    pub match_rule: String,
    pub match_value: String,
    pub confidence: f64,
    pub proposed_at: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct FusionCandidateQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct FusionCandidateListResponse {
    pub candidates: Vec<FusionCandidate>,
    pub total_count: usize,
}

// =============================================================================
// Fusion Review DTOs
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ReviewCandidateRequest {
    pub reviewer: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewCandidateResponse {
    pub candidate_id: String,
    pub status: String,
    pub reviewed_by: String,
    pub reviewed_at: String,
}

// =============================================================================
// Fusion Resolution DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct FusionResolveRequest {
    pub entities: Vec<serde_json::Map<String, serde_json::Value>>,
    pub rule: String,
    pub confidence: Option<f64>,
}

#[derive(Serialize)]
pub struct FusionResolveResponse {
    pub fusion_id: String,
    pub merged_entity_id: String,
    pub source_entity_ids: Vec<String>,
    pub rule: String,
    pub confidence: f64,
    pub created_at: String,
}

// =============================================================================
// Fusion Reversal DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct ReverseFusionRequest {
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ReverseFusionResponse {
    pub fusion_id: String,
    pub reversed: bool,
    pub reversed_at: String,
    pub reason: Option<String>,
}
