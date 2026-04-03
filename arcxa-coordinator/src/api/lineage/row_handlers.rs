//! Row-Level Lineage API Handlers
//!
//! REST API handlers for querying row-level lineage tracking.

use super::types::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use graphica_core::core::lineage::row_level::{
    JobStatistics, RowId, RowJourney,
};
#[cfg(feature = "test-endpoints")]
use graphica_core::core::lineage::row_level::RowLineageEvent;
use serde_json::json;
use std::sync::Arc;
use tracing::info;

/// API error response
#[derive(Debug)]
pub enum RowLineageApiError {
    NotFound(String),
    QueryFailed(String),
    InvalidInput(String),
    InternalError(String),
}

impl IntoResponse for RowLineageApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            RowLineageApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            RowLineageApiError::QueryFailed(msg) => (StatusCode::BAD_REQUEST, msg),
            RowLineageApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            RowLineageApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

/// Get row-level lineage for a specific row
///
/// Query: GET /api/v1/lineage/row/:row_key
///
/// Examples:
/// - CSV: /api/v1/lineage/row/csv:customers.csv:12345
/// - DB: /api/v1/lineage/row/db2:customers:customer_id=C123
/// - Kafka: /api/v1/lineage/row/kafka:orders:p5:o987654
#[utoipa::path(
    get,
    path = "/api/v1/lineage/row/{row_key}",
    params(
        ("row_key" = String, Path, description = "Row identifier in format: type:path:id (e.g., csv:file.csv:123)"),
    ),
    responses(
        (status = 200, description = "Row lineage found", body = RowLineageResponse),
        (status = 400, description = "Invalid row key format"),
        (status = 404, description = "No lineage found for this row"),
        (status = 500, description = "Row lineage store not available"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn get_row_lineage(
    State(state): State<Arc<ApiState>>,
    Path(row_key): Path<String>,
) -> Result<Json<RowLineageResponse>, RowLineageApiError> {
    info!("Querying row lineage for: {}", row_key);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    // Parse row key into RowId
    let row_id = parse_row_key(&row_key)
        .map_err(|e| RowLineageApiError::InvalidInput(format!("Invalid row key: {}", e)))?;

    // Debug: log the parsed row_id and its key representation
    let lookup_key = row_id.to_key();
    info!("Parsed row_id: {:?}, lookup_key: {}", row_id, lookup_key);

    // Query lineage
    let events = row_lineage_store
        .get_row_lineage(&row_id)
        .await
        .map_err(|e| RowLineageApiError::QueryFailed(format!("Query failed: {}", e)))?;

    if events.is_empty() {
        return Err(RowLineageApiError::NotFound(format!(
            "No lineage found for row: {}",
            row_key
        )));
    }

    let total_count = events.len();
    Ok(Json(RowLineageResponse {
        row_key,
        events,
        total_count,
    }))
}

/// Search indexed row keys for row-journey autocomplete.
///
/// Query: GET /api/v1/lineage/rows/search?q=...&limit=...
#[utoipa::path(
    get,
    path = "/api/v1/lineage/rows/search",
    params(
        ("q" = String, Query, description = "Partial row key or datasource/table prefix"),
        ("limit" = Option<usize>, Query, description = "Maximum number of matches to return"),
    ),
    responses(
        (status = 200, description = "Matching row keys found", body = RowKeySearchResponse),
        (status = 400, description = "Invalid search query"),
        (status = 500, description = "Row lineage store not available"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn search_row_keys(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<RowKeySearchQuery>,
) -> Result<Json<RowKeySearchResponse>, RowLineageApiError> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err(RowLineageApiError::InvalidInput(
            "Search query must not be empty".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(10).clamp(1, 25);
    info!("Searching row lineage keys for query='{}' limit={}", query, limit);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    let matches = row_lineage_store
        .search_row_keys(query, limit)
        .await
        .map_err(|e| RowLineageApiError::QueryFailed(format!("Row key search failed: {}", e)))?;

    let total_count = matches.len();
    Ok(Json(RowKeySearchResponse {
        query: query.to_string(),
        matches: matches
            .into_iter()
            .map(|row_id| RowKeySearchMatch {
                row_key: row_id.to_key(),
                source_type: row_id.source_type.to_string(),
                source_id: row_id.source_id,
            })
            .collect(),
        total_count,
    }))
}

/// Get row journey (complete trace from source to destination)
///
/// Query: GET /api/v1/lineage/row/:row_key/journey
#[utoipa::path(
    get,
    path = "/api/v1/lineage/row/{row_key}/journey",
    params(
        ("row_key" = String, Path, description = "Row identifier in format: type:path:id"),
    ),
    responses(
        (status = 200, description = "Row journey found", body = RowJourney),
        (status = 400, description = "Invalid row key format"),
        (status = 500, description = "Journey query failed"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn get_row_journey(
    State(state): State<Arc<ApiState>>,
    Path(row_key): Path<String>,
) -> Result<Json<RowJourney>, RowLineageApiError> {
    info!("Tracing row journey for: {}", row_key);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    let row_id = parse_row_key(&row_key)
        .map_err(|e| RowLineageApiError::InvalidInput(format!("Invalid row key: {}", e)))?;

    let journey = row_lineage_store
        .trace_row_journey(&row_id)
        .await
        .map_err(|e| RowLineageApiError::QueryFailed(format!("Journey query failed: {}", e)))?;

    Ok(Json(journey))
}

/// Get lineage for all rows in a batch
///
/// Query: GET /api/v1/lineage/batch/:batch_id
#[utoipa::path(
    get,
    path = "/api/v1/lineage/batch/{batch_id}",
    params(
        ("batch_id" = String, Path, description = "Batch identifier"),
    ),
    responses(
        (status = 200, description = "Batch lineage found", body = BatchLineageResponse),
        (status = 404, description = "No lineage found for this batch"),
        (status = 500, description = "Batch query failed"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn get_batch_lineage(
    State(state): State<Arc<ApiState>>,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchLineageResponse>, RowLineageApiError> {
    info!("Querying batch lineage for: {}", batch_id);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    let events = row_lineage_store
        .get_batch_lineage(&batch_id)
        .await
        .map_err(|e| RowLineageApiError::QueryFailed(format!("Batch query failed: {}", e)))?;

    if events.is_empty() {
        return Err(RowLineageApiError::NotFound(format!(
            "No lineage found for batch: {}",
            batch_id
        )));
    }

    let total_rows = events.len();
    Ok(Json(BatchLineageResponse {
        batch_id,
        events,
        total_rows,
    }))
}

/// Get job statistics
///
/// Query: GET /api/v1/lineage/job/:job_id/stats
#[utoipa::path(
    get,
    path = "/api/v1/lineage/job/{job_id}/stats",
    params(
        ("job_id" = String, Path, description = "Job identifier"),
    ),
    responses(
        (status = 200, description = "Job statistics retrieved", body = JobStatistics),
        (status = 500, description = "Stats query failed"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn get_job_stats(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<JobStatistics>, RowLineageApiError> {
    info!("Querying job statistics for: {}", job_id);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    let stats = row_lineage_store
        .get_job_stats(&job_id)
        .await
        .map_err(|e| RowLineageApiError::QueryFailed(format!("Stats query failed: {}", e)))?;

    Ok(Json(stats))
}

/// Get filtered rows for a job
///
/// Query: GET /api/v1/lineage/job/:job_id/filtered?start_time=...&end_time=...
#[utoipa::path(
    get,
    path = "/api/v1/lineage/job/{job_id}/filtered",
    params(
        ("job_id" = String, Path, description = "Job identifier"),
        ("start_time" = DateTime<Utc>, Query, description = "Start time filter (RFC3339 format)"),
        ("end_time" = DateTime<Utc>, Query, description = "End time filter (RFC3339 format)"),
    ),
    responses(
        (status = 200, description = "Filtered rows retrieved", body = FilteredRowsResponse),
        (status = 500, description = "Filtered rows query failed"),
    ),
    tag = "Row-Level Lineage"
)]
pub async fn get_filtered_rows(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<String>,
    Query(params): Query<FilteredRowsQuery>,
) -> Result<Json<FilteredRowsResponse>, RowLineageApiError> {
    info!("Querying filtered rows for job: {}", job_id);

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    let filtered = row_lineage_store
        .get_filtered_rows(&job_id, params.start_time, params.end_time)
        .await
        .map_err(|e| {
            RowLineageApiError::QueryFailed(format!("Filtered rows query failed: {}", e))
        })?;

    let total_count = filtered.len();
    Ok(Json(FilteredRowsResponse {
        job_id,
        filtered_rows: filtered
            .into_iter()
            .map(|(row_id, reason)| FilteredRow {
                row_key: row_id.to_key(),
                reason,
            })
            .collect(),
        total_count,
    }))
}

/// Write a single row lineage event (TEST/DEV ONLY)
///
/// ⚠️ WARNING: This endpoint is for TESTING purposes only!
///
/// In production, lineage events should be written internally by ETL pipelines
/// using the RowLevelLineageSink trait, NOT via HTTP POST.
///
/// This endpoint is:
/// - COMPILE-TIME GATED: Only available with `--features test-endpoints`
/// - RUNTIME GATED: Only enabled if ENABLE_TEST_LINEAGE_API=true
/// - NEVER for production use (creates SECURITY RISK)
/// - Only for integration testing and development
///
/// POST /api/v1/lineage/row/test
///
/// Request body: RowLineageEvent JSON
#[cfg(feature = "test-endpoints")]
pub async fn write_row_lineage_event_test(
    State(state): State<Arc<ApiState>>,
    Json(event): Json<RowLineageEvent>,
) -> Result<Json<serde_json::Value>, RowLineageApiError> {
    // Check if test endpoint is enabled
    if !is_test_lineage_api_enabled() {
        return Err(RowLineageApiError::InternalError(
            "Test lineage API is disabled. Set ENABLE_TEST_LINEAGE_API=true to enable (NOT FOR PRODUCTION)".to_string()
        ));
    }

    tracing::warn!(
        "TEST ENDPOINT: Writing row lineage event for row: {} (this endpoint should not be used in production!)",
        event.row_id.to_key()
    );

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    // Write the event
    row_lineage_store
        .write_row(event.clone())
        .await
        .map_err(|e| RowLineageApiError::InternalError(format!("Failed to write event: {}", e)))?;

    Ok(Json(json!({
        "status": "success",
        "row_id": event.row_id.to_key(),
        "message": "Row lineage event written successfully (TEST ENDPOINT)",
        "warning": "This is a test-only endpoint. Production systems should write lineage internally via RowLevelLineageSink."
    })))
}

/// Write a batch of row lineage events (TEST/DEV ONLY)
///
/// ⚠️ WARNING: This endpoint is for TESTING purposes only!
///
/// This endpoint is:
/// - COMPILE-TIME GATED: Only available with `--features test-endpoints`
/// - RUNTIME GATED: Only enabled if ENABLE_TEST_LINEAGE_API=true
/// - NEVER for production use (creates SECURITY RISK)
///
/// POST /api/v1/lineage/rows/batch/test
///
/// Request body: Array of RowLineageEvent JSON
#[cfg(feature = "test-endpoints")]
pub async fn write_row_lineage_batch_test(
    State(state): State<Arc<ApiState>>,
    Json(events): Json<Vec<RowLineageEvent>>,
) -> Result<Json<serde_json::Value>, RowLineageApiError> {
    // Check if test endpoint is enabled
    if !is_test_lineage_api_enabled() {
        return Err(RowLineageApiError::InternalError(
            "Test lineage API is disabled. Set ENABLE_TEST_LINEAGE_API=true to enable (NOT FOR PRODUCTION)".to_string()
        ));
    }

    tracing::warn!(
        "TEST ENDPOINT: Writing batch of {} row lineage events (this endpoint should not be used in production!)",
        events.len()
    );

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    if events.is_empty() {
        return Err(RowLineageApiError::InvalidInput(
            "Batch must contain at least one event".to_string(),
        ));
    }

    // Write the batch
    row_lineage_store
        .write_rows_batch(events.clone())
        .await
        .map_err(|e| RowLineageApiError::InternalError(format!("Failed to write batch: {}", e)))?;

    Ok(Json(json!({
        "status": "success",
        "events_written": events.len(),
        "message": format!("Successfully wrote {} row lineage events (TEST ENDPOINT)", events.len()),
        "warning": "This is a test-only endpoint. Production systems should write lineage internally via RowLevelLineageSink."
    })))
}

/// Flush buffered lineage events to storage (TEST/DEV ONLY)
///
/// ⚠️ WARNING: This endpoint is for TESTING purposes only!
///
/// Forces all buffered row lineage events to be flushed to RocksDB.
/// This is necessary because the row lineage store uses write buffering
/// (default buffer size: 1000 events) for performance.
///
/// This endpoint is:
/// - COMPILE-TIME GATED: Only available with `--features test-endpoints`
/// - RUNTIME GATED: Only enabled if ENABLE_TEST_LINEAGE_API=true
/// - NEVER for production use (creates SECURITY RISK)
///
/// POST /api/v1/lineage/flush/test
#[cfg(feature = "test-endpoints")]
pub async fn flush_lineage_buffer_test(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, RowLineageApiError> {
    // Check if test endpoint is enabled
    if !is_test_lineage_api_enabled() {
        return Err(RowLineageApiError::InternalError(
            "Test lineage API is disabled. Set ENABLE_TEST_LINEAGE_API=true to enable (NOT FOR PRODUCTION)".to_string()
        ));
    }

    tracing::warn!(
        "TEST ENDPOINT: Flushing row lineage buffer (this endpoint should not be used in production!)"
    );

    let row_lineage_store = state.row_lineage_store.as_ref().ok_or_else(|| {
        RowLineageApiError::InternalError("Row lineage store not available".to_string())
    })?;

    // Call the flush_buffer method from the trait
    row_lineage_store
        .flush_buffer()
        .await
        .map_err(|e| RowLineageApiError::InternalError(format!("Failed to flush buffer: {}", e)))?;

    Ok(Json(json!({
        "status": "success",
        "message": "Row lineage buffer flushed successfully (TEST ENDPOINT)",
        "warning": "This is a test-only endpoint used to force flush of buffered events."
    })))
}

/// Check if test lineage API is enabled (runtime check)
///
/// This provides an additional layer of protection beyond compile-time feature gating.
/// Even if compiled with --features test-endpoints, the endpoint won't work unless
/// ENABLE_TEST_LINEAGE_API environment variable is explicitly set to true.
#[cfg(feature = "test-endpoints")]
fn is_test_lineage_api_enabled() -> bool {
    std::env::var("ENABLE_TEST_LINEAGE_API")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false)
}

/// Parse row key string into RowId
///
/// Handles formats like:
/// - `csv:/tmp/healthcare_patients.csv:2` (CSV with absolute path)
/// - `db2:customers:customer_id=C123` (database)
/// - `kafka:orders:p5:o987654` (Kafka)
/// - `s3:bucket/key.parquet:r45678` (S3)
pub fn parse_row_key(key: &str) -> anyhow::Result<RowId> {
    RowId::from_key(key)
}

#[cfg(test)]
#[path = "row_handlers_tests.rs"]
mod tests;
