//! Column-Level Lineage API Handlers
//!
//! REST API handlers for querying column-level lineage tracking.

use super::types::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use graphica_core::core::lineage::column_level::{
    ColumnImpactAnalysis, ColumnLineageEvent, ColumnLineageGraph, ColumnRef,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

/// API error response
#[derive(Debug)]
pub enum ColumnLineageApiError {
    NotFound(String),
    QueryFailed(String),
    InvalidInput(String),
    InternalError(String),
}

impl IntoResponse for ColumnLineageApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ColumnLineageApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ColumnLineageApiError::QueryFailed(msg) => (StatusCode::BAD_REQUEST, msg),
            ColumnLineageApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            ColumnLineageApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

/// Get column-level lineage
///
/// Query: GET /api/v1/lineage/column/:table/:column
///
/// Returns all transformations that produce this column.
#[utoipa::path(
    get,
    path = "/api/v1/lineage/column/{table}/{column}",
    params(
        ("table" = String, Path, description = "Table name"),
        ("column" = String, Path, description = "Column name"),
        ("datasource_id" = Option<String>, Query, description = "Datasource ID filter"),
        ("schema" = Option<String>, Query, description = "Schema name filter"),
    ),
    responses(
        (status = 200, description = "Column lineage found", body = Vec<ColumnLineageEvent>),
        (status = 404, description = "No lineage found for this column"),
        (status = 500, description = "Column lineage store not available"),
    ),
    tag = "Column-Level Lineage"
)]
pub async fn get_column_lineage(
    State(state): State<Arc<ApiState>>,
    Path((table, column)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ColumnLineageEvent>>, ColumnLineageApiError> {
    info!("Querying column lineage for: {}.{}", table, column);

    let column_lineage_store = state.column_lineage_store.as_ref().ok_or_else(|| {
        ColumnLineageApiError::InternalError("Column lineage store not available".to_string())
    })?;

    // Build ColumnRef from path params and query params
    let datasource_id = params
        .get("datasource_id")
        .cloned()
        .unwrap_or_else(|| "default".to_string());

    let data_type = params
        .get("data_type")
        .cloned()
        .unwrap_or_else(|| "VARCHAR".to_string());

    let mut column_ref = ColumnRef::new(datasource_id, table, column, data_type);

    if let Some(schema) = params.get("schema") {
        column_ref = column_ref.with_schema(schema.clone());
    }

    // Query lineage
    let events = column_lineage_store
        .get_column_lineage(&column_ref)
        .await
        .map_err(|e| ColumnLineageApiError::QueryFailed(format!("Query failed: {}", e)))?;

    if events.is_empty() {
        return Err(ColumnLineageApiError::NotFound(format!(
            "No lineage found for column: {}.{}",
            column_ref.table_name, column_ref.column_name
        )));
    }

    Ok(Json(events))
}

/// Get column lineage graph (upstream dependencies)
///
/// Query: GET /api/v1/lineage/column/:table/:column/graph
#[utoipa::path(
    get,
    path = "/api/v1/lineage/column/{table}/{column}/graph",
    params(
        ("table" = String, Path, description = "Table name"),
        ("column" = String, Path, description = "Column name"),
        ("datasource_id" = Option<String>, Query, description = "Datasource ID"),
        ("schema" = Option<String>, Query, description = "Schema name"),
        ("max_depth" = Option<usize>, Query, description = "Maximum traversal depth (default: 10)"),
    ),
    responses(
        (status = 200, description = "Column lineage graph", body = ColumnLineageGraph),
        (status = 404, description = "No lineage graph found"),
        (status = 500, description = "Graph query failed"),
    ),
    tag = "Column-Level Lineage"
)]
pub async fn get_column_graph(
    State(state): State<Arc<ApiState>>,
    Path((table, column)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ColumnLineageGraph>, ColumnLineageApiError> {
    info!("Tracing column graph for: {}.{}", table, column);

    let column_lineage_store = state.column_lineage_store.as_ref().ok_or_else(|| {
        ColumnLineageApiError::InternalError("Column lineage store not available".to_string())
    })?;

    // Build ColumnRef
    let datasource_id = params
        .get("datasource_id")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let data_type = params
        .get("data_type")
        .cloned()
        .unwrap_or_else(|| "VARCHAR".to_string());

    let mut column_ref = ColumnRef::new(datasource_id, table, column, data_type);

    if let Some(schema) = params.get("schema") {
        column_ref = column_ref.with_schema(schema.clone());
    }

    let max_depth = params
        .get("max_depth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    // Trace graph
    let graph = column_lineage_store
        .trace_column_graph(&column_ref, max_depth)
        .await
        .map_err(|e| ColumnLineageApiError::QueryFailed(format!("Graph query failed: {}", e)))?;

    Ok(Json(graph))
}

/// Analyze column impact (downstream effects)
///
/// Query: POST /api/v1/lineage/column/impact-analysis
#[utoipa::path(
    post,
    path = "/api/v1/lineage/column/impact-analysis",
    request_body = ColumnRef,
    responses(
        (status = 200, description = "Impact analysis complete", body = ColumnImpactAnalysis),
        (status = 500, description = "Impact analysis failed"),
    ),
    tag = "Column-Level Lineage"
)]
pub async fn analyze_column_impact(
    State(state): State<Arc<ApiState>>,
    Json(column_ref): Json<ColumnRef>,
) -> Result<Json<ColumnImpactAnalysis>, ColumnLineageApiError> {
    info!(
        "Analyzing impact for column: {}",
        column_ref.fully_qualified_name()
    );

    let column_lineage_store = state.column_lineage_store.as_ref().ok_or_else(|| {
        ColumnLineageApiError::InternalError("Column lineage store not available".to_string())
    })?;

    // Analyze impact
    let impact = column_lineage_store
        .analyze_column_impact(&column_ref)
        .await
        .map_err(|e| {
            ColumnLineageApiError::QueryFailed(format!("Impact analysis failed: {}", e))
        })?;

    info!(
        "Impact analysis complete: {} affected columns, {} affected pipelines",
        impact.affected_columns.len(),
        impact.affected_pipelines.len()
    );

    Ok(Json(impact))
}

/// Get all derived columns from a source column
///
/// Query: GET /api/v1/lineage/column/:table/:column/derived
#[utoipa::path(
    get,
    path = "/api/v1/lineage/column/{table}/{column}/derived",
    params(
        ("table" = String, Path, description = "Table name"),
        ("column" = String, Path, description = "Column name"),
        ("datasource_id" = Option<String>, Query, description = "Datasource ID"),
        ("schema" = Option<String>, Query, description = "Schema name"),
    ),
    responses(
        (status = 200, description = "Derived columns found", body = Vec<ColumnRef>),
        (status = 404, description = "No derived columns found"),
    ),
    tag = "Column-Level Lineage"
)]
pub async fn get_derived_columns(
    State(state): State<Arc<ApiState>>,
    Path((table, column)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ColumnRef>>, ColumnLineageApiError> {
    info!("Finding derived columns for: {}.{}", table, column);

    let column_lineage_store = state.column_lineage_store.as_ref().ok_or_else(|| {
        ColumnLineageApiError::InternalError("Column lineage store not available".to_string())
    })?;

    // Build ColumnRef
    let datasource_id = params
        .get("datasource_id")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let data_type = params
        .get("data_type")
        .cloned()
        .unwrap_or_else(|| "VARCHAR".to_string());

    let mut column_ref = ColumnRef::new(datasource_id, table, column, data_type);

    if let Some(schema) = params.get("schema") {
        column_ref = column_ref.with_schema(schema.clone());
    }

    // Get derived columns
    let derived = column_lineage_store
        .get_derived_columns(&column_ref)
        .await
        .map_err(|e| ColumnLineageApiError::QueryFailed(format!("Query failed: {}", e)))?;

    if derived.is_empty() {
        return Err(ColumnLineageApiError::NotFound(format!(
            "No derived columns found for: {}.{}",
            column_ref.table_name, column_ref.column_name
        )));
    }

    Ok(Json(derived))
}
