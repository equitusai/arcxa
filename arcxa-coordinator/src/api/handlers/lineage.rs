//! Lineage Handler Functions
//!
//! HTTP handlers for lineage queries, impact analysis, and time-travel operations.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use graphica_core::core::lineage::{ImpactAnalyzer, LineageEvent, LineageSink, ProposedChange};
use std::sync::Arc;

/// Get lineage for a specific record
pub async fn get_record_lineage(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<LineageResponse>, ApiError> {
    tracing::info!("Getting lineage for record: {}", id);

    // Try RDF store first (governance brain with SPARQL)
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;
        let sparql = crate::governance::sparql_templates::SparqlTemplates::get_entity_lineage(&id);

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                let total_count = results.len();
                tracing::info!("RDF query returned {} results", total_count);
                return Ok(Json(LineageResponse {
                    record_id: id.clone(),
                    events: results,
                    total_count,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query failed, falling back to RocksDB: {}", e);
            }
        }
    }

    // Fallback to RocksDB lineage storage
    let events = state
        .lineage_storage
        .get_record_lineage(&id)
        .map_err(|e| ApiError::internal(format!("Failed to query lineage: {}", e)))?;

    let total_count = events.len();

    Ok(Json(LineageResponse {
        record_id: id,
        events: events
            .into_iter()
            .map(|e| {
                serde_json::to_value(e).unwrap_or_else(|err| {
                    tracing::warn!("Failed to serialize lineage event: {}", err);
                    serde_json::Value::Null
                })
            })
            .collect(),
        total_count,
    }))
}

/// Get model impact analysis
pub async fn get_model_impact(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
    Query(params): Query<ModelImpactQuery>,
) -> Result<Json<ModelImpactResponse>, ApiError> {
    tracing::info!("Getting impact for model: {}@{}", model_id, params.version);

    // Try RDF store first (governance brain with SPARQL)
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;
        let sparql =
            crate::governance::sparql_templates::SparqlTemplates::get_model_impact(&model_id);

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                tracing::info!("RDF query returned {} affected entities", results.len());

                // Extract unique datasets from results
                let datasets: Vec<String> = results
                    .iter()
                    .filter_map(|r| r.get("entity").and_then(|e| e.as_str()).map(String::from))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                return Ok(Json(ModelImpactResponse {
                    model_id: model_id.clone(),
                    version: params.version.clone(),
                    affected_records: results.len() as u64,
                    datasets,
                    events: results.into_iter().take(100).collect(), // Limit to 100 for response size
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query failed, falling back to RocksDB: {}", e);
            }
        }
    }

    // Fallback to RocksDB lineage storage
    let events = state
        .lineage_storage
        .get_model_impact(&model_id, &params.version)
        .map_err(|e| ApiError::internal(format!("Failed to query model impact: {}", e)))?;

    // Extract unique datasets
    let mut datasets: std::collections::HashSet<String> = std::collections::HashSet::new();
    for event in &events {
        datasets.insert(event.dataset.clone());
    }

    Ok(Json(ModelImpactResponse {
        model_id,
        version: params.version,
        affected_records: events.len() as u64,
        datasets: datasets.into_iter().collect(),
        events: events
            .into_iter()
            .take(100)
            .map(|e| serde_json::to_value(e).unwrap())
            .collect(), // Limit to 100 for response size
    }))
}

/// Query lineage by time range or filters
pub async fn query_lineage(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<LineageQueryRequest>,
) -> Result<Json<LineageResponse>, ApiError> {
    tracing::info!("Querying lineage: {:?}", req);

    let events = if let (Some(start), Some(end)) = (req.start_time, req.end_time) {
        let start_dt = chrono::DateTime::parse_from_rfc3339(&start)
            .map_err(|e| ApiError::bad_request(format!("Invalid start_time: {}", e)))?
            .with_timezone(&chrono::Utc);

        let end_dt = chrono::DateTime::parse_from_rfc3339(&end)
            .map_err(|e| ApiError::bad_request(format!("Invalid end_time: {}", e)))?
            .with_timezone(&chrono::Utc);

        state
            .lineage_storage
            .query_by_time_range(start_dt, end_dt)
            .map_err(|e| ApiError::internal(format!("Failed to query lineage: {}", e)))?
    } else if let Some(run_id) = req.run_id {
        state
            .lineage_storage
            .get_run_lineage(&run_id)
            .map_err(|e| ApiError::internal(format!("Failed to query run lineage: {}", e)))?
    } else {
        return Err(ApiError::bad_request(
            "Must provide either start_time+end_time or run_id".to_string(),
        ));
    };

    Ok(Json(LineageResponse {
        record_id: "".to_string(),
        events: events
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap_or_else(|err| {
                    tracing::warn!("Failed to serialize lineage event: {}", err);
                    serde_json::Value::Null
                })
            })
            .collect(),
        total_count: events.len(),
    }))
}

/// Write lineage events (bulk)
pub async fn write_lineage_events(
    State(state): State<Arc<ApiState>>,
    Json(events): Json<Vec<LineageEvent>>,
) -> Result<Json<WriteLineageResponse>, ApiError> {
    let count = events.len();
    tracing::info!("Writing {} lineage events", count);

    for event in &events {
        state
            .lineage_storage
            .write(event.clone())
            .map_err(|e| ApiError::internal(format!("Failed to write lineage: {}", e)))?;
    }

    Ok(Json(WriteLineageResponse {
        success: true,
        count,
    }))
}

/// Time-travel lineage query: Get lineage as it existed at a specific timestamp
pub async fn get_record_lineage_as_of(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(params): Query<AsOfQuery>,
) -> Result<Json<LineageResponse>, ApiError> {
    tracing::info!(
        "Time-travel query for record: {} as-of: {:?}",
        id,
        params.timestamp
    );

    let events = state
        .lineage_storage
        .get_lineage_as_of(&id, params.timestamp)
        .map_err(|e| ApiError::internal(format!("Time-travel query failed: {}", e)))?;

    let total_count = events.len();
    Ok(Json(LineageResponse {
        record_id: id,
        events: events
            .into_iter()
            .map(|e| {
                serde_json::to_value(e).unwrap_or_else(|err| {
                    tracing::warn!("Failed to serialize lineage event: {}", err);
                    serde_json::Value::Null
                })
            })
            .collect(),
        total_count,
    }))
}

/// Forward impact analysis: What will be affected if this changes?
pub async fn forward_impact_analysis(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ForwardImpactQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("Forward impact analysis for source: {}", params.source);

    let analyzer = ImpactAnalyzer::new(state.lineage_storage.clone());

    // Parse source (format: "system:path")
    let parts: Vec<&str> = params.source.split(':').collect();
    if parts.len() != 2 {
        return Err(ApiError::bad_request(
            "Source must be in format 'system:path'".to_string(),
        ));
    }

    let source = graphica_core::core::lineage::DataRef {
        system: parts[0].to_string(),
        path: parts[1].to_string(),
        version: None,
        extracted_at: Utc::now(),
        cdc_position: None,
    };

    let report = analyzer
        .forward_impact(&source, params.as_of)
        .await
        .map_err(|e| ApiError::internal(format!("Impact analysis failed: {}", e)))?;

    Ok(Json(serde_json::to_value(report).unwrap_or_else(|err| {
        tracing::warn!("Failed to serialize quality report: {}", err);
        serde_json::json!({"error": "Serialization failed"})
    })))
}

/// Backward root-cause analysis: What sources caused this output?
pub async fn backward_root_cause(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<BackwardAnalysisQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("Root cause analysis for record: {}", params.record_id);

    let analyzer = ImpactAnalyzer::new(state.lineage_storage.clone());

    let report = analyzer
        .root_cause_analysis(&params.record_id, params.as_of)
        .await
        .map_err(|e| ApiError::internal(format!("Root cause analysis failed: {}", e)))?;

    Ok(Json(serde_json::to_value(report).unwrap_or_else(|err| {
        tracing::warn!("Failed to serialize quality report: {}", err);
        serde_json::json!({"error": "Serialization failed"})
    })))
}

/// Simulate change impact: What would break if I modify this?
pub async fn simulate_change(
    State(state): State<Arc<ApiState>>,
    Json(change): Json<ProposedChange>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("Simulating change: {:?}", change.change_type);

    let analyzer = ImpactAnalyzer::new(state.lineage_storage.clone());

    let report = analyzer
        .simulate_change(&change)
        .await
        .map_err(|e| ApiError::internal(format!("Change simulation failed: {}", e)))?;

    Ok(Json(serde_json::to_value(report).unwrap_or_else(|err| {
        tracing::warn!("Failed to serialize quality report: {}", err);
        serde_json::json!({"error": "Serialization failed"})
    })))
}

/// Get ML model training data with time-travel
pub async fn get_model_training_data_as_of(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
    Query(params): Query<AsOfQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!(
        "Getting training data for model {} as-of {:?}",
        model_id,
        params.timestamp
    );

    // Query model impact up to the timestamp
    let events = state
        .lineage_storage
        .query_by_time_range(
            params.timestamp - chrono::Duration::days(365),
            params.timestamp,
        )
        .map_err(|e| ApiError::internal(format!("Query failed: {}", e)))?;

    // Filter for events with this model
    let model_events: Vec<_> = events
        .into_iter()
        .filter(|e| e.model_refs.iter().any(|m| m.model_id == model_id))
        .collect();

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "as_of": params.timestamp,
        "training_events": model_events.len(),
        "events": model_events,
    })))
}

/// Get model lineage from RDF store
pub async fn get_model_lineage_rdf(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelLineageGraph>, ApiError> {
    let brain = state
        .governance_brain
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF governance brain not initialized".to_string()))?;

    // Use SPARQL template for model impact query
    let sparql = format!(
        r#"
        PREFIX prov: <http://www.w3.org/ns/prov#>
        PREFIX ml: <http://graphica.io/ontology/ml#>
        PREFIX gph: <http://graphica.io/ontology#>

        SELECT ?record ?dataset WHERE {{
            ?lineage prov:wasAssociatedWith ?model .
            ?model ml:modelName "{}" .
            ?lineage gph:recordId ?record .
            ?lineage gph:dataset ?dataset .
        }}
        LIMIT 1000
    "#,
        model_id
    );

    let results = brain
        .query(&sparql)
        .map_err(|e| ApiError::internal(format!("RDF query failed: {}", e)))?;

    let impacted_records: Vec<String> = results
        .iter()
        .filter_map(|r| r.get("record").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    Ok(Json(ModelLineageGraph {
        model_id,
        impacted_records: impacted_records.clone(),
        lineage_depth: impacted_records.len(),
    }))
}
