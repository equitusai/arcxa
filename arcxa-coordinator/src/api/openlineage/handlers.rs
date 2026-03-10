//! OpenLineage API Handlers
//!
//! REST API endpoints for receiving and serving OpenLineage events.
//! Compatible with the OpenLineage 1.0.0 specification.

use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use graphica_core::core::lineage::LineageEvent;
use graphica_core::openlineage::{EventType, LineageConverter, OpenLineageEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};

/// OpenLineage event ingestion endpoint
///
/// POST /api/v1/lineage
///
/// Receives OpenLineage events from external systems (Airflow, Spark, etc.)
/// and converts them to Graphica's internal lineage format for storage.
pub async fn ingest_event(
    State(state): State<Arc<ApiState>>,
    Json(event): Json<OpenLineageEvent>,
) -> Result<Json<IngestionResponse>, (StatusCode, String)> {
    debug!(
        "Received OpenLineage event: job={}.{}, run_id={}, event_type={:?}",
        event.job.namespace, event.job.name, event.run.run_id, event.event_type
    );

    // Validate the event
    if let Err(e) = validate_event(&event) {
        warn!("Invalid OpenLineage event: {}", e);
        return Err((StatusCode::BAD_REQUEST, e.to_string()));
    }

    // Convert OpenLineage event to Graphica's internal format
    match convert_openlineage_to_graphica(&event) {
        Ok(lineage_event) => {
            // Store the lineage event
            use graphica_core::core::lineage::LineageSink;
            if let Err(e) = state.lineage_storage.as_ref().write(lineage_event) {
                warn!("Failed to store lineage event: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to store event: {}", e),
                ));
            }

            info!(
                "Successfully ingested and stored OpenLineage event for run_id={}",
                event.run.run_id
            );

            Ok(Json(IngestionResponse {
                status: "accepted".to_string(),
                run_id: event.run.run_id.clone(),
                message: Some("Event successfully ingested and stored".to_string()),
            }))
        }
        Err(e) => {
            warn!("Failed to convert OpenLineage event: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid event format: {}", e),
            ))
        }
    }
}

/// Export Graphica lineage events in OpenLineage format
///
/// GET /api/v1/lineage/export
///
/// Converts Graphica's internal lineage events to OpenLineage format
/// for consumption by external tools (Marquez, DataHub, etc.)
pub async fn export_events(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ExportParams>,
) -> Result<Json<Vec<OpenLineageEvent>>, (StatusCode, String)> {
    debug!("Exporting lineage events with params: {:?}", params);

    use graphica_core::core::lineage::LineageSink;

    // Query internal lineage store based on parameters
    let graphica_events = if let Some(run_id) = &params.run_id {
        // Query by run_id (maps to record_id in LineageEvent)
        state
            .lineage_storage
            .as_ref()
            .get_record_lineage(run_id)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to query lineage: {}", e),
                )
            })?
    } else if let (Some(start), Some(end)) = (&params.start_time, &params.end_time) {
        // Query by time range
        let start_dt = chrono::DateTime::parse_from_rfc3339(start)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid start_time: {}", e),
                )
            })?
            .with_timezone(&chrono::Utc);
        let end_dt = chrono::DateTime::parse_from_rfc3339(end)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid end_time: {}", e)))?
            .with_timezone(&chrono::Utc);

        state
            .lineage_storage
            .as_ref()
            .query_by_time_range(start_dt, end_dt)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to query lineage: {}", e),
                )
            })?
    } else {
        // No filters - return empty to avoid overwhelming response
        vec![]
    };

    // Apply limit if specified
    let limit = params.limit.unwrap_or(100) as usize;
    let limited_events: Vec<_> = graphica_events.into_iter().take(limit).collect();

    // Convert to OpenLineage format
    let converter = LineageConverter::new();
    let openlineage_events = converter.convert_batch(&limited_events);

    info!("Exported {} OpenLineage events", openlineage_events.len());

    Ok(Json(openlineage_events))
}

/// Get namespaces (for OpenLineage backend compatibility)
///
/// GET /api/v1/namespaces
pub async fn list_namespaces(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<NamespacesResponse>, (StatusCode, String)> {
    // Return list of job namespaces from our lineage data
    // For now, return a default namespace
    Ok(Json(NamespacesResponse {
        namespaces: vec![NamespaceInfo {
            name: "graphica".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }],
    }))
}

/// Get jobs in a namespace
///
/// GET /api/v1/namespaces/{namespace}/jobs
pub async fn list_jobs(
    State(state): State<Arc<ApiState>>,
    Path(namespace): Path<String>,
) -> Result<Json<JobsResponse>, (StatusCode, String)> {
    debug!("Listing jobs in namespace: {}", namespace);

    // TODO: Query jobs from lineage store
    Ok(Json(JobsResponse { jobs: vec![] }))
}

/// Get job details
///
/// GET /api/v1/namespaces/{namespace}/jobs/{job}
pub async fn get_job(
    State(state): State<Arc<ApiState>>,
    Path((namespace, job)): Path<(String, String)>,
) -> Result<Json<JobInfo>, (StatusCode, String)> {
    debug!("Getting job {}.{}", namespace, job);

    // TODO: Query job details from lineage store
    Err((StatusCode::NOT_FOUND, "Job not found".to_string()))
}

/// Get runs for a job
///
/// GET /api/v1/namespaces/{namespace}/jobs/{job}/runs
pub async fn list_runs(
    State(state): State<Arc<ApiState>>,
    Path((namespace, job)): Path<(String, String)>,
    Query(params): Query<RunsQueryParams>,
) -> Result<Json<RunsResponse>, (StatusCode, String)> {
    debug!("Listing runs for job {}.{}", namespace, job);

    // TODO: Query runs from lineage store
    Ok(Json(RunsResponse { runs: vec![] }))
}

/// Get run details
///
/// GET /api/v1/namespaces/{namespace}/jobs/{job}/runs/{run_id}
pub async fn get_run(
    State(state): State<Arc<ApiState>>,
    Path((namespace, job, run_id)): Path<(String, String, String)>,
) -> Result<Json<RunInfo>, (StatusCode, String)> {
    debug!("Getting run {} for job {}.{}", run_id, namespace, job);

    // TODO: Query run details from lineage store
    Err((StatusCode::NOT_FOUND, "Run not found".to_string()))
}

/// Validate OpenLineage event structure
fn validate_event(event: &OpenLineageEvent) -> anyhow::Result<()> {
    if event.run.run_id.is_empty() {
        anyhow::bail!("run_id cannot be empty");
    }

    if event.job.namespace.is_empty() {
        anyhow::bail!("job.namespace cannot be empty");
    }

    if event.job.name.is_empty() {
        anyhow::bail!("job.name cannot be empty");
    }

    if event.producer.is_empty() {
        anyhow::bail!("producer cannot be empty");
    }

    Ok(())
}

/// Convert OpenLineage event to Graphica's internal LineageEvent format
fn convert_openlineage_to_graphica(ol_event: &OpenLineageEvent) -> anyhow::Result<LineageEvent> {
    use graphica_core::core::lineage::{DataRef, TransformRef};
    use std::collections::HashMap;
    use uuid::Uuid;

    // Create dataset name from job
    let dataset = format!("{}.{}", ol_event.job.namespace, ol_event.job.name);

    // Convert input datasets to DataRefs
    let source_refs: Vec<DataRef> = ol_event
        .inputs
        .iter()
        .map(|input| DataRef {
            system: input.namespace.clone(),
            path: input.name.clone(),
            version: None,
            extracted_at: ol_event.event_time,
            cdc_position: None,
        })
        .collect();

    // Convert output datasets - use the first output or create a default one
    let output_ref = if let Some(output) = ol_event.outputs.first() {
        DataRef {
            system: output.namespace.clone(),
            path: output.name.clone(),
            version: None,
            extracted_at: ol_event.event_time,
            cdc_position: None,
        }
    } else {
        // If no outputs, use the job as output
        DataRef {
            system: ol_event.job.namespace.clone(),
            path: ol_event.job.name.clone(),
            version: None,
            extracted_at: ol_event.event_time,
            cdc_position: None,
        }
    };

    // Extract tenant_id from job namespace or use default
    let tenant_id = ol_event.job.namespace.clone();

    // Create a simple transform record if there's a SQL facet
    let transforms: Vec<TransformRef> = if let Some(sql_facet) = ol_event.job.facets.get("sql") {
        if let Some(query) = sql_facet.get("query").and_then(|v| v.as_str()) {
            let mut params = HashMap::new();
            params.insert("sql".to_string(), serde_json::json!(query));

            vec![TransformRef {
                id: Uuid::new_v4(),
                transform_type: "sql_transform".to_string(),
                rule_id: "openlineage_sql".to_string(),
                version: "1.0.0".to_string(),
                parameters: params,
                applied_at: ol_event.event_time,
                fields_modified: vec![],
            }]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Build metadata from producer and other facets
    let mut metadata = HashMap::new();
    metadata.insert("producer".to_string(), ol_event.producer.clone());
    metadata.insert(
        "event_type".to_string(),
        format!("{:?}", ol_event.event_type),
    );

    Ok(LineageEvent {
        id: Uuid::new_v4(),
        dataset,
        record_id: ol_event.run.run_id.clone(),
        source_refs,
        transforms,
        model_refs: vec![],
        output_ref,
        ts: ol_event.event_time,
        run_id: ol_event.run.run_id.clone(),
        tenant_id,
        correlation_id: None,
        metadata,
    })
}

// ===== Request/Response Types =====

#[derive(Debug, Serialize, ToSchema)]
pub struct IngestionResponse {
    pub status: String,
    pub run_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ExportParams {
    /// Filter by job namespace
    pub namespace: Option<String>,
    /// Filter by job name
    pub job_name: Option<String>,
    /// Filter by run ID
    pub run_id: Option<String>,
    /// Start time (RFC3339 format)
    pub start_time: Option<String>,
    /// End time (RFC3339 format)
    pub end_time: Option<String>,
    /// Limit number of results
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NamespacesResponse {
    pub namespaces: Vec<NamespaceInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NamespaceInfo {
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobsResponse {
    pub jobs: Vec<JobInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobInfo {
    pub namespace: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub latest_run: Option<RunInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunsResponse {
    pub runs: Vec<RunInfo>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RunsQueryParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub state: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_event_valid() {
        let event = OpenLineageEvent::new(
            EventType::Complete,
            "run-123".to_string(),
            "airflow".to_string(),
            "my-dag".to_string(),
            "https://airflow.example.com".to_string(),
        );

        assert!(validate_event(&event).is_ok());
    }

    #[test]
    fn test_validate_event_empty_run_id() {
        let mut event = OpenLineageEvent::new(
            EventType::Complete,
            "".to_string(), // Empty run_id
            "airflow".to_string(),
            "my-dag".to_string(),
            "https://airflow.example.com".to_string(),
        );

        assert!(validate_event(&event).is_err());
    }

    #[test]
    fn test_validate_event_empty_namespace() {
        let mut event = OpenLineageEvent::new(
            EventType::Complete,
            "run-123".to_string(),
            "".to_string(), // Empty namespace
            "my-dag".to_string(),
            "https://airflow.example.com".to_string(),
        );

        assert!(validate_event(&event).is_err());
    }
}
