//! Schema Evolution API Handlers
//!
//! REST API handlers for querying schema evolution tracking and drift analysis.

use super::types::*;
use crate::api::ApiState;
use crate::storage::schema_evolution_store::SchemaEvolutionStore;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use graphica_core::core::lineage::schema_evolution::{
    MigrationImpactAnalysis, SchemaChangeEvent, SchemaDriftAnalysis, SchemaVersion,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

/// API error response
#[derive(Debug)]
pub enum SchemaEvolutionApiError {
    NotFound(String),
    QueryFailed(String),
    InvalidInput(String),
    InternalError(String),
}

impl IntoResponse for SchemaEvolutionApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            SchemaEvolutionApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            SchemaEvolutionApiError::QueryFailed(msg) => (StatusCode::BAD_REQUEST, msg),
            SchemaEvolutionApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            SchemaEvolutionApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

/// Record a schema change event
///
/// POST /api/v1/lineage/schema/change
///
/// Records a schema change event for tracking and drift analysis.
#[utoipa::path(
    post,
    path = "/api/v1/lineage/schema/change",
    request_body = SchemaChangeEvent,
    responses(
        (status = 200, description = "Schema change recorded successfully"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn record_schema_change(
    State(state): State<Arc<ApiState>>,
    Json(event): Json<SchemaChangeEvent>,
) -> Result<Json<serde_json::Value>, SchemaEvolutionApiError> {
    info!(
        "Recording schema change: {:?} for table {}",
        event.change_type, event.table_name
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    schema_evolution_store
        .record_schema_change(event.clone())
        .map_err(|e| {
            error!("Failed to record schema change: {}", e);
            SchemaEvolutionApiError::InternalError(format!("Failed to record schema change: {}", e))
        })?;

    Ok(Json(json!({
        "status": "success",
        "event_id": event.id,
    })))
}

/// Get schema change history for a datasource
///
/// GET /api/v1/lineage/schema/datasource/:datasource_id/changes
///
/// Returns all schema change events for a specific datasource.
#[utoipa::path(
    get,
    path = "/api/v1/lineage/schema/datasource/{datasource_id}/changes",
    params(
        ("datasource_id" = String, Path, description = "Datasource ID"),
        ("breaking_only" = Option<bool>, Query, description = "Return only breaking changes"),
    ),
    responses(
        (status = 200, description = "Schema changes found", body = Vec<SchemaChangeEvent>),
        (status = 404, description = "No schema changes found for this datasource"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn get_datasource_schema_changes(
    State(state): State<Arc<ApiState>>,
    Path(datasource_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SchemaChangeEvent>>, SchemaEvolutionApiError> {
    info!("Querying schema changes for datasource: {}", datasource_id);

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    let breaking_only = params
        .get("breaking_only")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    let events = if breaking_only {
        schema_evolution_store
            .get_breaking_changes(&datasource_id)
            .map_err(|e| {
                error!("Failed to query breaking changes: {}", e);
                SchemaEvolutionApiError::QueryFailed(format!(
                    "Failed to query breaking changes: {}",
                    e
                ))
            })?
    } else {
        schema_evolution_store
            .get_datasource_schema_changes(&datasource_id)
            .map_err(|e| {
                error!("Failed to query schema changes: {}", e);
                SchemaEvolutionApiError::QueryFailed(format!(
                    "Failed to query schema changes: {}",
                    e
                ))
            })?
    };

    if events.is_empty() {
        return Err(SchemaEvolutionApiError::NotFound(format!(
            "No schema changes found for datasource: {}",
            datasource_id
        )));
    }

    Ok(Json(events))
}

/// Get schema change history for a specific table
///
/// GET /api/v1/lineage/schema/datasource/:datasource_id/table/:table_name/changes
///
/// Returns all schema change events for a specific table.
#[utoipa::path(
    get,
    path = "/api/v1/lineage/schema/datasource/{datasource_id}/table/{table_name}/changes",
    params(
        ("datasource_id" = String, Path, description = "Datasource ID"),
        ("table_name" = String, Path, description = "Table name"),
    ),
    responses(
        (status = 200, description = "Table schema changes found", body = Vec<SchemaChangeEvent>),
        (status = 404, description = "No schema changes found for this table"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn get_table_schema_changes(
    State(state): State<Arc<ApiState>>,
    Path((datasource_id, table_name)): Path<(String, String)>,
) -> Result<Json<Vec<SchemaChangeEvent>>, SchemaEvolutionApiError> {
    info!(
        "Querying schema changes for table: {}.{}",
        datasource_id, table_name
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    let events = schema_evolution_store
        .get_table_schema_changes(&datasource_id, &table_name)
        .map_err(|e| {
            error!("Failed to query table schema changes: {}", e);
            SchemaEvolutionApiError::QueryFailed(format!(
                "Failed to query table schema changes: {}",
                e
            ))
        })?;

    if events.is_empty() {
        return Err(SchemaEvolutionApiError::NotFound(format!(
            "No schema changes found for table: {}.{}",
            datasource_id, table_name
        )));
    }

    Ok(Json(events))
}

/// Save a schema version snapshot
///
/// POST /api/v1/lineage/schema/version
///
/// Saves a complete schema snapshot for a datasource at a point in time.
#[utoipa::path(
    post,
    path = "/api/v1/lineage/schema/version",
    request_body = SchemaVersion,
    responses(
        (status = 200, description = "Schema version saved successfully"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn save_schema_version(
    State(state): State<Arc<ApiState>>,
    Json(version): Json<SchemaVersion>,
) -> Result<Json<serde_json::Value>, SchemaEvolutionApiError> {
    info!(
        "Saving schema version: {} for datasource {}",
        version.version_id, version.datasource_id
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    schema_evolution_store
        .save_schema_version(version.clone())
        .map_err(|e| {
            error!("Failed to save schema version: {}", e);
            SchemaEvolutionApiError::InternalError(format!("Failed to save schema version: {}", e))
        })?;

    Ok(Json(json!({
        "status": "success",
        "version_id": version.version_id,
    })))
}

/// Get the latest schema version for a datasource
///
/// GET /api/v1/lineage/schema/datasource/:datasource_id/version/latest
///
/// Returns the most recent schema snapshot for a datasource.
#[utoipa::path(
    get,
    path = "/api/v1/lineage/schema/datasource/{datasource_id}/version/latest",
    params(
        ("datasource_id" = String, Path, description = "Datasource ID"),
    ),
    responses(
        (status = 200, description = "Latest schema version found", body = SchemaVersion),
        (status = 404, description = "No schema versions found for this datasource"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn get_latest_schema_version(
    State(state): State<Arc<ApiState>>,
    Path(datasource_id): Path<String>,
) -> Result<Json<SchemaVersion>, SchemaEvolutionApiError> {
    info!(
        "Querying latest schema version for datasource: {}",
        datasource_id
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    let version = schema_evolution_store
        .get_latest_schema_version(&datasource_id)
        .map_err(|e| {
            error!("Failed to query latest schema version: {}", e);
            SchemaEvolutionApiError::QueryFailed(format!(
                "Failed to query latest schema version: {}",
                e
            ))
        })?
        .ok_or_else(|| {
            SchemaEvolutionApiError::NotFound(format!(
                "No schema versions found for datasource: {}",
                datasource_id
            ))
        })?;

    Ok(Json(version))
}

/// Analyze schema drift between two versions
///
/// GET /api/v1/lineage/schema/drift/:source_version/:target_version
///
/// Compares two schema versions and returns a drift analysis report.
#[utoipa::path(
    get,
    path = "/api/v1/lineage/schema/drift/{source_version}/{target_version}",
    params(
        ("source_version" = String, Path, description = "Source version ID (baseline)"),
        ("target_version" = String, Path, description = "Target version ID (current)"),
    ),
    responses(
        (status = 200, description = "Drift analysis completed", body = SchemaDriftAnalysis),
        (status = 404, description = "One or both versions not found"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn analyze_schema_drift(
    State(state): State<Arc<ApiState>>,
    Path((source_version, target_version)): Path<(String, String)>,
) -> Result<Json<SchemaDriftAnalysis>, SchemaEvolutionApiError> {
    info!(
        "Analyzing schema drift from {} to {}",
        source_version, target_version
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    let analysis = schema_evolution_store
        .analyze_schema_drift(&source_version, &target_version)
        .map_err(|e| {
            error!("Failed to analyze schema drift: {}", e);
            SchemaEvolutionApiError::QueryFailed(format!("Failed to analyze schema drift: {}", e))
        })?;

    Ok(Json(analysis))
}

/// Analyze migration impact for a schema change
///
/// POST /api/v1/lineage/schema/impact
///
/// Analyzes the downstream impact of a proposed or actual schema change.
#[utoipa::path(
    post,
    path = "/api/v1/lineage/schema/impact",
    request_body = SchemaChangeEvent,
    responses(
        (status = 200, description = "Impact analysis completed", body = MigrationImpactAnalysis),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Schema evolution store not available"),
    ),
    tag = "Schema Evolution"
)]
pub async fn analyze_migration_impact(
    State(state): State<Arc<ApiState>>,
    Json(change): Json<SchemaChangeEvent>,
) -> Result<Json<MigrationImpactAnalysis>, SchemaEvolutionApiError> {
    info!(
        "Analyzing migration impact for change: {:?} on table {}",
        change.change_type, change.table_name
    );

    let schema_evolution_store = state.schema_evolution_store.as_ref().ok_or_else(|| {
        SchemaEvolutionApiError::InternalError("Schema evolution store not available".to_string())
    })?;

    let analysis = schema_evolution_store
        .analyze_migration_impact(&change)
        .map_err(|e| {
            error!("Failed to analyze migration impact: {}", e);
            SchemaEvolutionApiError::QueryFailed(format!(
                "Failed to analyze migration impact: {}",
                e
            ))
        })?;

    Ok(Json(analysis))
}
