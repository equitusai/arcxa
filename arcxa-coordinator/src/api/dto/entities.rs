//! Entity DTOs
//!
//! Request and response types for entity queries and attribute management.

use serde::{Deserialize, Serialize};

// =============================================================================
// Dataset DTOs
// =============================================================================

#[derive(Serialize)]
pub struct DatasetListResponse {
    pub datasets: Vec<DatasetSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Serialize)]
pub struct DatasetSummary {
    pub id: String,
    pub name: String,
    pub dataset_type: String,
    pub asset_kind: String,
    pub record_count: i64,
    pub quality_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub last_ingested_at: Option<String>,

    // Lineage links
    pub source_datasource_id: Option<String>,
    pub workflow_execution_id: Option<String>,
}

#[derive(Serialize)]
pub struct DatasetResponse {
    pub id: String,
    pub name: String,
    pub dataset_type: String,
    pub asset_kind: String,
    pub record_count: i64,
    pub schema: Vec<DatasetColumnDto>,
    pub quality_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub last_ingested_at: Option<String>,

    // Lineage metadata
    pub lineage: DatasetLineage,
}

#[derive(Serialize)]
pub struct DatasetColumnDto {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub distinct_count: Option<i64>,
    pub null_percentage: Option<f64>,
}

#[derive(Serialize)]
pub struct DatasetLineage {
    pub source_datasource_id: Option<String>,
    pub workflow_execution_id: Option<String>,
    pub workflow_name: Option<String>,
    pub executed_at: Option<String>,
}

#[derive(Deserialize)]
pub struct DatasetListQuery {
    pub dataset_type: Option<String>,
    pub dataset_scope: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

// =============================================================================
// Entity Attribute DTOs
// =============================================================================

#[derive(Serialize)]
pub struct EntityAttributesResponse {
    pub entity_id: String,
    pub attributes: Vec<DerivedAttribute>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct DerivedAttribute {
    pub name: String,
    pub value: String,
    pub confidence: f64,
    pub model_id: Option<String>,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct EntityResponse {
    pub entity_id: String,
    pub entity_type: Option<String>,
    pub properties: std::collections::HashMap<String, serde_json::Value>,
    pub derived_attributes: Vec<DerivedAttribute>,

    // Fusion metadata (for frontend team - BACKEND_SPEC_2025_10_12)
    pub source_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_date: Option<String>,
}

// =============================================================================
// Entity List DTOs
// =============================================================================

#[derive(Serialize)]
pub struct EntityListResponse {
    pub entities: Vec<EntitySummary>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct EntitySummary {
    pub id: String,
    pub entity_type: Option<String>,
    pub domain: Option<String>,
    pub attribute_count: usize,
    pub avg_confidence: f64,
    pub status: String,
    pub created_at: String,

    // Fusion metadata
    pub source_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_date: Option<String>,
}

// =============================================================================
// Attribute Timeseries DTOs
// =============================================================================

#[derive(Serialize)]
pub struct AttributeTimeseriesResponse {
    pub entity_id: String,
    pub attribute_name: String,
    pub datapoints: Vec<AttributeDatapoint>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct AttributeDatapoint {
    pub timestamp: String,
    pub value: String,
    pub confidence: f64,
    pub model_id: Option<String>,
}
