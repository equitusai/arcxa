//! Model DTOs
//!
//! Request and response types for ML model registration, predictions, and orchestration.

use serde::{Deserialize, Serialize};

// =============================================================================
// Model Registration DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct RegisterModelRequest {
    pub model_id: String,
    pub version: String,
    pub model_type: String,
}

#[derive(Serialize)]
pub struct ModelResponse {
    pub id: String,
    pub model_id: String,
    pub version: String,
}

// =============================================================================
// Model Predictions DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct RecordPredictionsRequest {
    pub predictions: Vec<Prediction>,
}

#[derive(Deserialize)]
pub struct Prediction {
    pub entity_id: String,
    pub attribute: String,
    pub value: String,
    pub confidence: f64,
}

#[derive(Serialize)]
pub struct PredictionsResponse {
    pub recorded: usize,
    pub model_id: String,
}

// =============================================================================
// Orchestration Model DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct RegisterOrchestrationModelRequest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub endpoint: ModelEndpointDto,
    pub framework: String,
    pub input_schema: Vec<FeatureSchemaDto>,
    pub output_schema: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEndpointDto {
    pub protocol: String,
    pub url: String,
    pub timeout_ms: u64,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSchemaDto {
    pub name: String,
    pub data_type: String,
    pub required: bool,
}

#[derive(Serialize)]
pub struct ModelSummaryResponseDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: String,
}
