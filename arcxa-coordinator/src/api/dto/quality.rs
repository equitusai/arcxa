//! Quality DTOs
//!
//! Request and response types for data quality management.

use serde::{Deserialize, Serialize};

// =============================================================================
// Quality Query Requests
// =============================================================================

#[derive(Deserialize)]
pub struct ScorecardQuery {
    pub start: String,
    pub end: String,
}

#[derive(Deserialize)]
pub struct ViolationQuery {
    pub dataset: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub rule_type: String,
    pub expression: String,
}

#[derive(Deserialize)]
pub struct LoadRuleRequest {
    pub wasm_bytes: String, // Base64 encoded WASM
}

// =============================================================================
// Quality Responses
// =============================================================================

#[derive(Serialize)]
pub struct ScorecardResponse {
    pub dataset: String,
    pub overall_score: f64,
    pub period_start: String,
    pub period_end: String,
    pub dimension_scores: std::collections::HashMap<String, f64>,
}

#[derive(Serialize)]
pub struct ViolationListResponse {
    pub violations: Vec<serde_json::Value>,
    pub total: u64,
    pub page: u32,
}

#[derive(Serialize)]
pub struct RuleResponse {
    pub id: String,
    pub name: String,
    pub created: bool,
}
