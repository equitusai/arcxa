//! Connector DTOs
//!
//! Request and response types for connector discovery, metadata, and management operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Connector List DTOs
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorListResponse {
    pub connectors: Vec<ConnectorMetadataResponse>,
}

// =============================================================================
// Connector Metadata DTOs
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorMetadataResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source_type: String,
    pub capabilities: ConnectorCapabilitiesResponse,
    pub required_credentials: Vec<CredentialFieldResponse>,
    pub optional_config: Vec<ConfigFieldResponse>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorCapabilitiesResponse {
    pub supports_parameterized_queries: bool,
    pub supports_schema_inference: bool,
    pub supports_query_timeout: bool,
    pub supports_streaming: bool,
    pub supports_transactions: bool,
    pub max_batch_size: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CredentialFieldResponse {
    pub name: String,
    pub description: String,
    pub field_type: String,
    pub required: bool,
    pub sensitive: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigFieldResponse {
    pub name: String,
    pub description: String,
    pub field_type: String,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
}

// =============================================================================
// Connector Statistics DTOs
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorStatisticsResponse {
    pub total_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub by_category: HashMap<String, usize>,
    pub total_usage: usize,
}

// =============================================================================
// Connector Operation DTOs
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectorOperationResponse {
    pub success: bool,
    pub message: String,
}
