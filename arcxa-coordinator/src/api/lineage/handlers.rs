//! Lineage API Handlers
//!
//! REST API handlers for querying W3C PROV-compliant lineage data.

use super::types::*;
use crate::api::ApiState;
use crate::governance::rdf_store::RdfStore;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink, ModelRef, TransformRef};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// API error response
#[derive(Debug)]
pub enum LineageApiError {
    NotFound(String),
    QueryFailed(String),
    InvalidInput(String),
    InternalError(String),
}

impl IntoResponse for LineageApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            LineageApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            LineageApiError::QueryFailed(msg) => (StatusCode::BAD_REQUEST, msg),
            LineageApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            LineageApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

/// Get lineage for a specific record
///
/// Query: GET /api/v1/lineage/record/:record_id
#[utoipa::path(
    get,
    path = "/api/v1/lineage/record/{record_id}",
    params(
        ("record_id" = String, Path, description = "Record identifier"),
    ),
    responses(
        (status = 200, description = "Record lineage found", body = Vec<LineageRecordResponse>),
        (status = 404, description = "No lineage found for this record"),
        (status = 500, description = "RDF store not available or query failed"),
    ),
    tag = "Workflow Lineage"
)]
pub async fn get_record_lineage(
    State(state): State<Arc<ApiState>>,
    Path(record_id): Path<String>,
) -> Result<Json<Vec<LineageRecordResponse>>, LineageApiError> {
    info!("Querying lineage for record: {}", record_id);

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| LineageApiError::InternalError("RDF store not available".to_string()))?;

    // SPARQL query to get lineage for record (including transforms and models)
    let query = format!(
        r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX ml: <http://graphica.io/ml#>

SELECT ?recordId ?activity ?dataset ?runId ?tenantId ?ts ?sourceSystem ?sourcePath ?outputSystem ?outputPath
       ?transformId ?transformType ?modelId ?modelVersion ?modelType
WHERE {{
    GRAPH <http://graphica.io/lineage> {{
        ?activity gph:recordId ?recordId ;
        FILTER (?recordId = "{}")
                  gph:dataset ?dataset ;
                  gph:runId ?runId ;
                  gph:tenantId ?tenantId ;
                  prov:startedAtTime ?ts .

        OPTIONAL {{
            ?activity prov:used ?source .
            ?source gph:system ?sourceSystem ;
                    gph:path ?sourcePath .
        }}

        OPTIONAL {{
            ?output prov:wasGeneratedBy ?activity .
            ?output gph:system ?outputSystem ;
                    gph:path ?outputPath .
        }}

        OPTIONAL {{
            ?activity gph:wasTransformedBy ?transform .
            ?transform gph:transformId ?transformId ;
                       gph:transformType ?transformType .
        }}

        OPTIONAL {{
            ?activity prov:wasAssociatedWith ?model .
            ?model ml:modelId ?modelId ;
                   ml:version ?modelVersion .
            OPTIONAL {{ ?model ml:modelType ?modelType . }}
        }}
    }}
}}
"#,
        record_id
    );

    let results = rdf_store
        .query(&query)
        .map_err(|e| LineageApiError::QueryFailed(format!("SPARQL query failed: {}", e)))?;

    if results.is_empty() {
        return Err(LineageApiError::NotFound(format!(
            "No lineage found for record: {}",
            record_id
        )));
    }

    // Parse SPARQL results into LineageRecordResponse
    let lineage_events = parse_lineage_records(&results)?;

    debug!(
        "Found {} lineage events for record: {}",
        lineage_events.len(),
        record_id
    );

    Ok(Json(lineage_events))
}

/// Get lineage graph (upstream + downstream) for a record
///
/// Query: GET /api/v1/lineage/record/:record_id/graph?max_depth=5
#[utoipa::path(
    get,
    path = "/api/v1/lineage/record/{record_id}/graph",
    params(
        ("record_id" = String, Path, description = "Record identifier"),
        ("max_depth" = Option<usize>, Query, description = "Maximum traversal depth (default: 5)"),
    ),
    responses(
        (status = 200, description = "Lineage graph found", body = LineageGraphResponse),
        (status = 404, description = "No lineage graph found for this record"),
        (status = 500, description = "RDF store not available or query failed"),
    ),
    tag = "Workflow Lineage"
)]
pub async fn get_lineage_graph(
    State(state): State<Arc<ApiState>>,
    Path(record_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<LineageGraphResponse>, LineageApiError> {
    info!("Querying lineage graph for record: {}", record_id);

    let max_depth: usize = params
        .get("max_depth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| LineageApiError::InternalError("RDF store not available".to_string()))?;

    // Query for full lineage graph with recursive upstream expansion
    let query = format!(
        r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX ml: <http://graphica.io/ml#>

SELECT DISTINCT ?recordId ?dataset ?runId ?tenantId ?ts
                ?sourceSystem ?sourcePath ?outputSystem ?outputPath
                ?transformId ?transformType ?modelId ?modelVersion ?modelType
WHERE {{
    GRAPH <http://graphica.io/lineage> {{
        # Start with the root record
        ?rootActivity gph:recordId "{}" .

        # Recursively find upstream activities
        # Use property path (prov:used/^prov:wasGeneratedBy)* to traverse the graph
        ?rootActivity (prov:used/^prov:wasGeneratedBy){{0,{max_depth}}} ?activity .

        # Extract all activity details
        ?activity gph:recordId ?recordId ;
                  gph:dataset ?dataset ;
                  gph:runId ?runId ;
                  gph:tenantId ?tenantId ;
                  prov:startedAtTime ?ts .

        # Get sources for this activity
        OPTIONAL {{
            ?activity prov:used ?source .
            ?source gph:system ?sourceSystem ;
                    gph:path ?sourcePath .
        }}

        # Get outputs for this activity
        OPTIONAL {{
            ?output prov:wasGeneratedBy ?activity .
            ?output gph:system ?outputSystem ;
                    gph:path ?outputPath .
        }}

        # Get transforms applied
        OPTIONAL {{
            ?activity gph:wasTransformedBy ?transform .
            ?transform gph:transformId ?transformId ;
                       gph:transformType ?transformType .
        }}

        # Get models used
        OPTIONAL {{
            ?activity prov:wasAssociatedWith ?model .
            ?model ml:modelId ?modelId ;
                   ml:version ?modelVersion .
            OPTIONAL {{ ?model ml:modelType ?modelType . }}
        }}
    }}
}}
LIMIT 1000
"#,
        record_id,
        max_depth = max_depth
    );

    let results = rdf_store
        .query(&query)
        .map_err(|e| LineageApiError::QueryFailed(format!("SPARQL query failed: {}", e)))?;

    if results.is_empty() {
        return Err(LineageApiError::NotFound(format!(
            "No lineage graph found for record: {}",
            record_id
        )));
    }

    // Parse results and build graph
    let events = parse_lineage_records(&results)?;
    let graph_response = build_lineage_graph_response(&record_id, events, max_depth)?;

    info!(
        "Built lineage graph for {}: {} total events",
        record_id, graph_response.total_events
    );

    Ok(Json(graph_response))
}

/// Get model impact analysis
///
/// Query: GET /api/v1/lineage/model/:model_id/impact?version=2.1.0
#[utoipa::path(
    get,
    path = "/api/v1/lineage/model/{model_id}/impact",
    params(
        ("model_id" = String, Path, description = "Model identifier"),
        ("version" = String, Query, description = "Model version (required)"),
    ),
    responses(
        (status = 200, description = "Model impact analysis found", body = ModelImpactResponse),
        (status = 400, description = "Missing version parameter"),
        (status = 404, description = "No impact found for this model"),
        (status = 500, description = "RDF store not available or query failed"),
    ),
    tag = "Workflow Lineage"
)]
pub async fn get_model_impact(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ModelImpactResponse>, LineageApiError> {
    let version = params
        .get("version")
        .ok_or_else(|| LineageApiError::InvalidInput("version parameter required".to_string()))?
        .clone();

    info!("Querying model impact for: {} v{}", model_id, version);

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| LineageApiError::InternalError("RDF store not available".to_string()))?;

    // SPARQL query for model impact
    let query = format!(
        r#"
PREFIX ml: <http://graphica.io/ml#>
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?recordId ?dataset ?runId ?ts ?outputPath
WHERE {{
    GRAPH <http://graphica.io/lineage> {{
        ?model ml:modelId "{}" ;
               ml:version "{}" .

        ?activity prov:wasAssociatedWith ?model ;
                  gph:recordId ?recordId ;
                  gph:dataset ?dataset ;
                  gph:runId ?runId ;
                  prov:startedAtTime ?ts .

        ?output prov:wasGeneratedBy ?activity .
        ?output gph:path ?outputPath .
    }}
}}
ORDER BY ?ts
"#,
        model_id, version
    );

    let results = rdf_store
        .query(&query)
        .map_err(|e| LineageApiError::QueryFailed(format!("SPARQL query failed: {}", e)))?;

    if results.is_empty() {
        return Err(LineageApiError::NotFound(format!(
            "No impact found for model: {} v{}",
            model_id, version
        )));
    }

    // Parse results into impact response
    let impact_response = parse_model_impact(&model_id, &version, results)?;

    info!(
        "Found {} records affected by model: {} v{}",
        impact_response.total_affected, model_id, version
    );

    Ok(Json(impact_response))
}

/// Get lineage for a specific run
///
/// Query: GET /api/v1/lineage/run/:run_id
#[utoipa::path(
    get,
    path = "/api/v1/lineage/run/{run_id}",
    params(
        ("run_id" = String, Path, description = "Workflow run identifier"),
    ),
    responses(
        (status = 200, description = "Run lineage found", body = RunLineageResponse),
        (status = 404, description = "No lineage found for this run"),
        (status = 500, description = "RDF store not available or query failed"),
    ),
    tag = "Workflow Lineage"
)]
pub async fn get_run_lineage(
    State(state): State<Arc<ApiState>>,
    Path(run_id): Path<String>,
) -> Result<Json<RunLineageResponse>, LineageApiError> {
    info!("Querying lineage for run: {}", run_id);

    if let Some(rdf_store) = state.rdf_store.as_ref() {
        // SPARQL query for run lineage
        let query = format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?recordId ?dataset ?tenantId ?ts ?sourceSystem ?sourcePath ?outputSystem ?outputPath
WHERE {{
    GRAPH <http://graphica.io/lineage> {{
        ?activity gph:runId "{}" ;
                  gph:recordId ?recordId ;
                  gph:dataset ?dataset ;
                  gph:tenantId ?tenantId ;
                  prov:startedAtTime ?ts .

        OPTIONAL {{
            ?activity prov:used ?source .
            ?source gph:system ?sourceSystem ;
                    gph:path ?sourcePath .
        }}

        OPTIONAL {{
            ?output prov:wasGeneratedBy ?activity .
            ?output gph:system ?outputSystem ;
                    gph:path ?outputPath .
        }}
    }}
}}
ORDER BY ?ts
"#,
            run_id
        );

        match rdf_store.query(&query) {
            Ok(results) if !results.is_empty() => {
                let run_response = parse_run_lineage(&run_id, results)?;

                info!(
                    "Found {} records for run: {} from RDF lineage",
                    run_response.total_records, run_id
                );

                return Ok(Json(run_response));
            }
            Ok(_) => {
                warn!(
                    "No RDF lineage found for run {}; falling back to durable lineage storage",
                    run_id
                );
            }
            Err(e) => {
                warn!(
                    "RDF run lineage query failed for {}; falling back to durable lineage storage: {}",
                    run_id, e
                );
            }
        }
    }

    let events = state
        .lineage_storage
        .get_run_lineage(&run_id)
        .map_err(|e| {
            LineageApiError::InternalError(format!("Failed to query run lineage: {}", e))
        })?;

    if events.is_empty() {
        return Err(LineageApiError::NotFound(format!(
            "No lineage found for run: {}",
            run_id
        )));
    }

    let run_response = parse_run_lineage_events(&run_id, events)?;

    info!(
        "Found {} records for run: {} from durable lineage storage",
        run_response.total_records, run_id
    );

    Ok(Json(run_response))
}

/// Query lineage by time range
///
/// Query: POST /api/v1/lineage/time-range
#[utoipa::path(
    post,
    path = "/api/v1/lineage/time-range",
    request_body = TimeRangeLineageQuery,
    responses(
        (status = 200, description = "Lineage events found in time range", body = TimeRangeLineageResponse),
        (status = 500, description = "RDF store not available or query failed"),
    ),
    tag = "Workflow Lineage"
)]
pub async fn query_lineage_by_time_range(
    State(state): State<Arc<ApiState>>,
    Json(query_params): Json<TimeRangeLineageQuery>,
) -> Result<Json<TimeRangeLineageResponse>, LineageApiError> {
    info!(
        "Querying lineage from {} to {}",
        query_params.start, query_params.end
    );

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| LineageApiError::InternalError("RDF store not available".to_string()))?;

    // Build SPARQL query with optional dataset filter
    let dataset_filter = query_params
        .dataset
        .as_ref()
        .map(|d| format!("FILTER (?dataset = \"{}\")", d))
        .unwrap_or_default();

    let limit = query_params.limit.unwrap_or(1000);

    let query = format!(
        r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?recordId ?dataset ?runId ?tenantId ?ts
WHERE {{
    GRAPH <http://graphica.io/lineage> {{
        ?activity gph:recordId ?recordId ;
                  gph:dataset ?dataset ;
                  gph:runId ?runId ;
                  gph:tenantId ?tenantId ;
                  prov:startedAtTime ?ts .

        FILTER (?ts >= "{}"^^xsd:dateTime && ?ts <= "{}"^^xsd:dateTime)
        {}
    }}
}}
ORDER BY ?ts
LIMIT {}
"#,
        query_params.start.to_rfc3339(),
        query_params.end.to_rfc3339(),
        dataset_filter,
        limit
    );

    let results = rdf_store
        .query(&query)
        .map_err(|e| LineageApiError::QueryFailed(format!("SPARQL query failed: {}", e)))?;

    // Parse results
    let time_range_response =
        parse_time_range_lineage(query_params.start, query_params.end, results)?;

    info!(
        "Found {} events between {} and {}",
        time_range_response.total_events, query_params.start, query_params.end
    );

    Ok(Json(time_range_response))
}

// Helper functions

fn parse_lineage_records(
    results: &[serde_json::Value],
) -> Result<Vec<LineageRecordResponse>, LineageApiError> {
    // Group results by record_id since one record can have multiple transforms/models
    let mut records_map: HashMap<String, LineageRecordResponse> = HashMap::new();

    for result in results {
        // Extract fields from SPARQL result
        let record_id = result["recordId"]
            .as_str()
            .ok_or_else(|| LineageApiError::InternalError("Missing recordId".to_string()))?
            .to_string();

        let dataset = result["dataset"].as_str().unwrap_or("unknown").to_string();

        let run_id = result["runId"].as_str().unwrap_or("").to_string();
        let tenant_id = result["tenantId"].as_str().unwrap_or("default").to_string();

        let timestamp = result["ts"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        // Get or create record entry
        let record_entry = records_map.entry(record_id.clone()).or_insert_with(|| {
            // Parse sources
            let mut sources = Vec::new();
            if let Some(source_system) = result["sourceSystem"].as_str() {
                if let Some(source_path) = result["sourcePath"].as_str() {
                    sources.push(DataRefDto {
                        system: source_system.to_string(),
                        path: source_path.to_string(),
                        version: None,
                        extracted_at: timestamp,
                        cdc_position: None,
                    });
                }
            }

            // Parse output
            let output = DataRefDto {
                system: result["outputSystem"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                path: result["outputPath"].as_str().unwrap_or("").to_string(),
                version: None,
                extracted_at: timestamp,
                cdc_position: None,
            };

            LineageRecordResponse {
                record_id: record_id.clone(),
                dataset: dataset.clone(),
                run_id: run_id.clone(),
                tenant_id: tenant_id.clone(),
                timestamp,
                sources,
                transforms: vec![],
                models: vec![],
                output,
                metadata: HashMap::new(),
            }
        });

        // Parse transforms if present
        if let (Some(transform_id), Some(transform_type)) = (
            result["transformId"].as_str(),
            result["transformType"].as_str(),
        ) {
            // Check if this transform is already added
            if !record_entry.transforms.iter().any(|t| t.id == transform_id) {
                record_entry.transforms.push(TransformDto {
                    id: transform_id.to_string(),
                    transform_type: transform_type.to_string(),
                    rule_id: result["ruleId"].as_str().unwrap_or("").to_string(),
                    version: result["transformVersion"]
                        .as_str()
                        .unwrap_or("1.0.0")
                        .to_string(),
                    parameters: HashMap::new(),
                    applied_at: timestamp,
                    fields_modified: vec![],
                });
            }
        }

        // Parse models if present
        if let (Some(model_id), Some(model_version)) =
            (result["modelId"].as_str(), result["modelVersion"].as_str())
        {
            // Check if this model is already added
            if !record_entry
                .models
                .iter()
                .any(|m| m.model_id == model_id && m.version == model_version)
            {
                record_entry.models.push(ModelDto {
                    model_id: model_id.to_string(),
                    version: model_version.to_string(),
                    model_type: result["modelType"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    params_hash: result["paramsHash"].as_str().unwrap_or("").to_string(),
                    training_data: vec![],
                    metrics: ModelMetricsDto {
                        accuracy: None,
                        precision: None,
                        recall: None,
                        f1_score: None,
                        rmse: None,
                        custom_metrics: HashMap::new(),
                    },
                    registry_uri: result["registryUri"].as_str().unwrap_or("").to_string(),
                    inference_at: timestamp,
                    features_used: vec![],
                    outputs: vec![],
                });
            }
        }
    }

    Ok(records_map.into_values().collect())
}

fn build_lineage_graph_response(
    record_id: &str,
    events: Vec<LineageRecordResponse>,
    _max_depth: usize,
) -> Result<LineageGraphResponse, LineageApiError> {
    let total_events = events.len();

    // Build adjacency list for graph traversal
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut upstream_records = HashSet::new();
    let downstream_records = HashSet::new();
    let mut source_systems = HashSet::new();
    let mut output_systems = HashSet::new();
    let mut transform_count = 0;
    let mut model_count = 0;

    for event in &events {
        // Build graph edges (output -> sources for upstream traversal)
        let mut sources_for_event = Vec::new();
        for source in &event.sources {
            upstream_records.insert(source.path.clone());
            source_systems.insert(source.system.clone());
            sources_for_event.push(source.path.clone());
        }
        graph.insert(event.output.path.clone(), sources_for_event);
        output_systems.insert(event.output.system.clone());
        transform_count += event.transforms.len();
        model_count += event.models.len();
    }

    // Calculate lineage depth (longest path from root to any source)
    let lineage_depth = calculate_lineage_depth(record_id, &graph);

    // Detect circular dependencies
    let has_circular_dependency = detect_cycles(&graph);

    Ok(LineageGraphResponse {
        root_record_id: record_id.to_string(),
        events,
        upstream_records: upstream_records.into_iter().collect(),
        downstream_records: downstream_records.into_iter().collect(),
        lineage_depth,
        total_events,
        statistics: LineageStatistics {
            source_systems: source_systems.len(),
            transform_count,
            model_count,
            output_systems: output_systems.len(),
            has_circular_dependency,
        },
    })
}

/// Calculate lineage depth using BFS (breadth-first search)
fn calculate_lineage_depth(root: &str, graph: &HashMap<String, Vec<String>>) -> usize {
    let mut max_depth = 0;
    let mut queue: std::collections::VecDeque<(String, usize)> = std::collections::VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    queue.push_back((root.to_string(), 0));
    visited.insert(root.to_string());

    while let Some((node, depth)) = queue.pop_front() {
        max_depth = max_depth.max(depth);

        if let Some(sources) = graph.get(&node) {
            for source in sources {
                if !visited.contains(source) {
                    visited.insert(source.clone());
                    queue.push_back((source.clone(), depth + 1));
                }
            }
        }
    }

    max_depth
}

/// Detect cycles using DFS (depth-first search) with color marking
fn detect_cycles(graph: &HashMap<String, Vec<String>>) -> bool {
    #[derive(PartialEq, Clone)]
    enum Color {
        White, // Not visited
        Gray,  // Currently visiting (in recursion stack)
        Black, // Completely visited
    }

    let mut colors: HashMap<String, Color> = HashMap::new();

    // Initialize all nodes as white
    for node in graph.keys() {
        colors.insert(node.clone(), Color::White);
    }

    fn dfs_visit(
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        colors: &mut HashMap<String, Color>,
    ) -> bool {
        colors.insert(node.to_string(), Color::Gray);

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                let neighbor_color = colors.get(neighbor).cloned().unwrap_or(Color::White);

                match neighbor_color {
                    Color::White => {
                        if dfs_visit(neighbor, graph, colors) {
                            return true; // Cycle found
                        }
                    }
                    Color::Gray => {
                        return true; // Back edge found = cycle
                    }
                    Color::Black => {
                        // Already visited, no cycle through this path
                    }
                }
            }
        }

        colors.insert(node.to_string(), Color::Black);
        false
    }

    // Check for cycles starting from each unvisited node
    for node in graph.keys() {
        if colors.get(node) == Some(&Color::White) {
            if dfs_visit(node, graph, &mut colors) {
                return true;
            }
        }
    }

    false
}

fn parse_model_impact(
    model_id: &str,
    version: &str,
    results: Vec<serde_json::Value>,
) -> Result<ModelImpactResponse, LineageApiError> {
    let mut affected_records = Vec::new();
    let mut datasets = HashSet::new();
    let mut first_impact: Option<DateTime<Utc>> = None;
    let mut last_impact: Option<DateTime<Utc>> = None;

    for result in results {
        let record_id = result["recordId"]
            .as_str()
            .ok_or_else(|| LineageApiError::InternalError("Missing recordId".to_string()))?
            .to_string();

        let dataset = result["dataset"].as_str().unwrap_or("unknown").to_string();
        let run_id = result["runId"].as_str().unwrap_or("").to_string();
        let output_path = result["outputPath"].as_str().unwrap_or("").to_string();

        let timestamp = result["ts"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        // Track first and last impact
        if first_impact.is_none() || Some(timestamp) < first_impact {
            first_impact = Some(timestamp);
        }
        if last_impact.is_none() || Some(timestamp) > last_impact {
            last_impact = Some(timestamp);
        }

        datasets.insert(dataset.clone());

        affected_records.push(AffectedRecordDto {
            record_id,
            dataset,
            run_id,
            timestamp,
            output_path,
        });
    }

    Ok(ModelImpactResponse {
        model_id: model_id.to_string(),
        version: version.to_string(),
        total_affected: affected_records.len(),
        affected_records,
        first_impact,
        last_impact,
        datasets: datasets.into_iter().collect(),
    })
}

fn parse_run_lineage(
    run_id: &str,
    results: Vec<serde_json::Value>,
) -> Result<RunLineageResponse, LineageApiError> {
    let mut datasets = HashSet::new();
    let mut start_time: Option<DateTime<Utc>> = None;
    let mut end_time: Option<DateTime<Utc>> = None;

    // Parse events
    let events = parse_lineage_records(&results)?;

    for result in &results {
        if let Some(dataset) = result["dataset"].as_str() {
            datasets.insert(dataset.to_string());
        }

        if let Some(ts_str) = result["ts"].as_str() {
            if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                let ts_utc = ts.with_timezone(&Utc);
                if start_time.is_none() || Some(ts_utc) < start_time {
                    start_time = Some(ts_utc);
                }
                if end_time.is_none() || Some(ts_utc) > end_time {
                    end_time = Some(ts_utc);
                }
            }
        }
    }

    Ok(RunLineageResponse {
        run_id: run_id.to_string(),
        total_records: events.len(),
        events,
        datasets: datasets.into_iter().collect(),
        start_time,
        end_time,
    })
}

fn parse_run_lineage_events(
    run_id: &str,
    events: Vec<LineageEvent>,
) -> Result<RunLineageResponse, LineageApiError> {
    let mut datasets = HashSet::new();
    let mut start_time: Option<DateTime<Utc>> = None;
    let mut end_time: Option<DateTime<Utc>> = None;

    let records = events
        .into_iter()
        .map(|event| {
            datasets.insert(event.dataset.clone());

            if start_time.is_none() || Some(event.ts) < start_time {
                start_time = Some(event.ts);
            }
            if end_time.is_none() || Some(event.ts) > end_time {
                end_time = Some(event.ts);
            }

            LineageRecordResponse {
                record_id: event.record_id,
                dataset: event.dataset,
                run_id: event.run_id,
                tenant_id: event.tenant_id,
                timestamp: event.ts,
                sources: event.source_refs.iter().map(data_ref_to_dto).collect(),
                transforms: event.transforms.iter().map(transform_to_dto).collect(),
                models: event.model_refs.iter().map(model_to_dto).collect(),
                output: data_ref_to_dto(&event.output_ref),
                metadata: event.metadata,
            }
        })
        .collect::<Vec<_>>();

    Ok(RunLineageResponse {
        run_id: run_id.to_string(),
        total_records: records.len(),
        events: records,
        datasets: datasets.into_iter().collect(),
        start_time,
        end_time,
    })
}

fn data_ref_to_dto(data_ref: &DataRef) -> DataRefDto {
    DataRefDto {
        system: data_ref.system.clone(),
        path: data_ref.path.clone(),
        version: data_ref.version.clone(),
        extracted_at: data_ref.extracted_at,
        cdc_position: data_ref
            .cdc_position
            .as_ref()
            .map(|position| CdcPositionDto {
                topic: position.topic.clone(),
                partition: position.partition,
                offset: position.offset,
                lsn: position.lsn.clone(),
            }),
    }
}

fn transform_to_dto(transform: &TransformRef) -> TransformDto {
    TransformDto {
        id: transform.id.to_string(),
        transform_type: transform.transform_type.clone(),
        rule_id: transform.rule_id.clone(),
        version: transform.version.clone(),
        parameters: transform.parameters.clone(),
        applied_at: transform.applied_at,
        fields_modified: transform.fields_modified.clone(),
    }
}

fn model_to_dto(model: &ModelRef) -> ModelDto {
    ModelDto {
        model_id: model.model_id.clone(),
        version: model.version.clone(),
        model_type: model.model_type.clone(),
        params_hash: model.params_hash.clone(),
        training_data: model.training_data.iter().map(data_ref_to_dto).collect(),
        metrics: ModelMetricsDto {
            accuracy: model.metrics.accuracy,
            precision: model.metrics.precision,
            recall: model.metrics.recall,
            f1_score: model.metrics.f1_score,
            rmse: model.metrics.rmse,
            custom_metrics: model.metrics.custom_metrics.clone(),
        },
        registry_uri: model.registry_uri.clone(),
        inference_at: model.inference_at,
        features_used: model.features_used.clone(),
        outputs: model.outputs.clone(),
    }
}

fn parse_time_range_lineage(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    results: Vec<serde_json::Value>,
) -> Result<TimeRangeLineageResponse, LineageApiError> {
    let mut datasets = HashSet::new();

    for result in &results {
        if let Some(dataset) = result["dataset"].as_str() {
            datasets.insert(dataset.to_string());
        }
    }

    let events = parse_lineage_records(&results)?;
    let total_events = events.len();

    Ok(TimeRangeLineageResponse {
        start,
        end,
        total_events,
        events,
        datasets: datasets.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::{DataRef, LineageEvent};
    use uuid::Uuid;

    #[test]
    fn test_parse_run_lineage_events_builds_response_from_storage_events() {
        let now = Utc::now();
        let event = LineageEvent {
            id: Uuid::new_v4(),
            dataset: "customer_dim".to_string(),
            record_id: "customer_dim:customer_id=C001".to_string(),
            source_refs: vec![DataRef {
                system: "postgres".to_string(),
                path: "public.arcxa_mcp_source:customer_id=C001".to_string(),
                version: None,
                extracted_at: now,
                cdc_position: None,
            }],
            transforms: Vec::new(),
            model_refs: Vec::new(),
            output_ref: DataRef {
                system: "postgresql".to_string(),
                path: "public.arcxa_mcp_unified_target".to_string(),
                version: None,
                extracted_at: now,
                cdc_position: None,
            },
            ts: now,
            run_id: "unified_load_loadjob_123".to_string(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::from([("session_id".to_string(), "unified_123".to_string())]),
        };

        let response =
            parse_run_lineage_events("unified_load_loadjob_123", vec![event]).expect("response");

        assert_eq!(response.run_id, "unified_load_loadjob_123");
        assert_eq!(response.total_records, 1);
        assert_eq!(
            response.events[0].record_id,
            "customer_dim:customer_id=C001"
        );
        assert_eq!(
            response.events[0].output.path,
            "public.arcxa_mcp_unified_target"
        );
        assert_eq!(response.datasets, vec!["customer_dim".to_string()]);
        assert!(response.start_time.is_some());
        assert!(response.end_time.is_some());
    }
}
