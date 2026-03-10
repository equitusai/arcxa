//! SPARQL and RDF Handler Functions
//!
//! HTTP handlers for SPARQL query execution and RDF store management.

use crate::api::dto::*;
use crate::api::validators::sparql::{is_query_too_complex, validate_sparql_query};
use crate::api::ApiState;
use axum::{extract::State, Json};
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct RdfStats {
    pub total_triples: usize,
    pub store_type: String,
    pub materialization_enabled: bool,
}

/// Execute SPARQL query or INSERT DATA operation
pub async fn sparql_query(
    State(state): State<Arc<ApiState>>,
    Json(query): Json<SparqlQuery>,
) -> Result<Json<SparqlResults>, ApiError> {
    let sparql_upper = query.sparql.trim().to_uppercase();

    // Check if this is an INSERT DATA operation
    if sparql_upper.contains("INSERT DATA") {
        tracing::info!("Executing SPARQL INSERT DATA");

        // Use RDF store update executor for INSERT DATA
        if let Some(ref rdf_store) = state.rdf_store {
            use crate::governance::rdf_store::RdfStore;

            let start = std::time::Instant::now();
            let result = rdf_store.as_ref().update(&query.sparql);

            let duration = start.elapsed().as_secs_f64();

            // Record metrics
            if let Some(ref metrics) = state.metrics_registry {
                match &result {
                    Ok(_) => metrics.rdf.record_query("insert", duration),
                    Err(_) => {
                        metrics.rdf.record_query("insert", duration);
                        metrics.rdf.record_parse_error("update_failed");
                    }
                }
            }

            result.map_err(|e| ApiError::internal(format!("INSERT DATA failed: {}", e)))?;

            // Return empty results for INSERT DATA
            return Ok(Json(SparqlResults { results: vec![] }));
        }

        return Err(ApiError::internal(
            "No RDF update backend available".to_string(),
        ));
    }

    // Regular query path
    tracing::info!("Executing SPARQL query");

    // Validate query before execution
    if let Err(validation_error) = validate_sparql_query(&query.sparql) {
        return Err(ApiError::bad_request(format!(
            "SPARQL validation failed: {}",
            validation_error
        )));
    }

    // Check query complexity
    if is_query_too_complex(&query.sparql) {
        return Err(ApiError::bad_request(
            "Query too complex. Please add LIMIT clause or reduce complexity.".to_string(),
        ));
    }

    // Try RDF store first (direct access)
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Execute with timeout wrapper
        let start_time = std::time::Instant::now();
        match rdf_store.as_ref().query(&query.sparql) {
            Ok(results) => {
                let elapsed = start_time.elapsed();
                let duration_secs = elapsed.as_secs_f64();

                tracing::info!(
                    "SPARQL query successful: {} results in {:?}",
                    results.len(),
                    elapsed
                );

                // Record successful query metrics
                if let Some(ref metrics) = state.metrics_registry {
                    metrics.rdf.record_query("select", duration_secs);
                    metrics
                        .rdf
                        .record_triple_operation("read", results.len() as u64);
                }

                // Enforce result size limit
                const MAX_RESULTS: usize = 10_000;
                if results.len() > MAX_RESULTS {
                    return Err(ApiError::bad_request(format!(
                        "Query returned {} results, exceeding limit of {}. Please add LIMIT clause.",
                        results.len(),
                        MAX_RESULTS
                    )));
                }

                return Ok(Json(SparqlResults { results }));
            }
            Err(e) => {
                let duration_secs = start_time.elapsed().as_secs_f64();

                // Record failed query metrics
                if let Some(ref metrics) = state.metrics_registry {
                    metrics.rdf.record_query("select", duration_secs);
                    metrics.rdf.record_parse_error("query_failed");
                }

                tracing::warn!("RDF store query failed, trying governance brain: {}", e);
            }
        }
    }

    // Fallback to governance brain
    if let Some(ref brain) = state.governance_brain {
        let start = std::time::Instant::now();
        let result = brain.query(&query.sparql);
        let duration = start.elapsed().as_secs_f64();

        // Record metrics for governance brain fallback
        if let Some(ref metrics) = state.metrics_registry {
            match &result {
                Ok(results) => {
                    metrics.rdf.record_query("select_fallback", duration);
                    metrics
                        .rdf
                        .record_triple_operation("read", results.len() as u64);
                }
                Err(_) => {
                    metrics.rdf.record_query("select_fallback", duration);
                    metrics.rdf.record_parse_error("fallback_failed");
                }
            }
        }

        let results =
            result.map_err(|e| ApiError::bad_request(format!("SPARQL query failed: {}", e)))?;

        return Ok(Json(SparqlResults { results }));
    }

    Err(ApiError::internal(
        "No RDF query backend available".to_string(),
    ))
}

/// Get RDF store statistics
pub async fn get_rdf_stats(State(state): State<Arc<ApiState>>) -> Result<Json<RdfStats>, ApiError> {
    let brain = state
        .governance_brain
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF governance brain not initialized".to_string()))?;

    // Clone brain for blocking task to avoid lifetime issues
    let brain_clone = brain.clone();

    // Use spawn_blocking to avoid blocking the async runtime
    // The brain.triple_count() method may call block_on internally,
    // which panics if called from within an async context
    let triple_count = tokio::task::spawn_blocking(move || brain_clone.triple_count())
        .await
        .map_err(|e| ApiError::internal(format!("Task join error: {}", e)))?
        .map_err(|e| ApiError::internal(format!("Failed to get triple count: {}", e)))?;

    Ok(Json(RdfStats {
        total_triples: triple_count,
        store_type: "oxigraph_memory".to_string(),
        materialization_enabled: state.governance_brain.is_some(),
    }))
}

/// Get RDF auto-save statistics with health monitoring
pub async fn get_rdf_auto_save_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<RdfAutoSaveStatsResponse>, ApiError> {
    tracing::info!("Retrieving RDF auto-save statistics");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let stats = rdf_store.get_auto_save_stats();

    // Format last save time as human-readable
    let last_save_formatted = if stats.last_save_time > 0 {
        let datetime = chrono::DateTime::from_timestamp(stats.last_save_time as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string());
        datetime
    } else {
        None
    };

    // Determine health based on time since last save
    // Warning if no save in 10 minutes (600 seconds)
    let healthy = stats
        .seconds_since_last_save
        .map_or(true, |secs| secs < 600);

    let message = match stats.seconds_since_last_save {
        Some(secs) if secs < 60 => "Auto-save healthy - recently saved".to_string(),
        Some(secs) if secs < 600 => format!("Auto-save healthy - last saved {} seconds ago", secs),
        Some(secs) => format!("WARNING: No save in {} seconds ({}min)", secs, secs / 60),
        None => "No saves yet - auto-save may not be configured".to_string(),
    };

    Ok(Json(RdfAutoSaveStatsResponse {
        last_save_time: stats.last_save_time,
        auto_save_count: stats.auto_save_count,
        auto_save_failures: stats.auto_save_failures,
        seconds_since_last_save: stats.seconds_since_last_save,
        last_save_formatted,
        healthy,
        message,
        timestamp: Utc::now(),
    }))
}

/// Manually trigger RDF save to disk
pub async fn trigger_rdf_save(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<RdfSaveResponse>, ApiError> {
    tracing::info!("Manually triggering RDF save to disk");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not initialized".to_string()))?;

    let start = std::time::Instant::now();
    let quads_saved = rdf_store
        .save_to_disk()
        .map_err(|e| ApiError::internal(format!("RDF save failed: {}", e)))?;
    let duration_ms = start.elapsed().as_millis();

    let message = format!(
        "Successfully saved {} quads to disk in {}ms",
        quads_saved, duration_ms
    );

    Ok(Json(RdfSaveResponse {
        success: true,
        quads_saved,
        duration_ms,
        message,
        timestamp: Utc::now(),
    }))
}
