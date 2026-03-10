//! REST and gRPC API endpoints for RDF-first workflow operations
//!
//! All workflow operations generate RDF triples and are queryable via SPARQL.

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::governance::rdf_store::GraphicaRdfStore;
use crate::orchestration::workflow::executor::WorkflowExecutor;
use crate::orchestration::workflow::rdf_lineage::{
    FieldHistory, WorkflowImpact, WorkflowLineageGenerator,
};

// ============== REST API ==============

/// REST API router for workflow endpoints
pub fn workflow_routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/api/v1/workflows", axum::routing::post(register_workflow))
        .route(
            "/api/v1/workflows/:id/execute",
            axum::routing::post(execute_workflow),
        )
        .route(
            "/api/v1/workflows/:id/executions",
            axum::routing::get(get_executions),
        )
        .route(
            "/api/v1/workflows/:id/executions/:exec_id/lineage",
            axum::routing::get(get_execution_lineage),
        )
        .route(
            "/api/v1/workflows/:id/impact",
            axum::routing::get(analyze_impact),
        )
        .route(
            "/api/v1/transformations/:id",
            axum::routing::get(get_transformation),
        )
        .route(
            "/api/v1/transformations/:id/reverse",
            axum::routing::post(reverse_transformation),
        )
        .route(
            "/api/v1/entities/:id/workflows",
            axum::routing::get(get_entity_workflows),
        )
        .route(
            "/api/v1/fields/:entity_id/:field_name/history",
            axum::routing::get(get_field_history),
        )
}

#[derive(Clone)]
pub struct ApiState {
    pub workflow_executor: Arc<WorkflowExecutor>,
    pub lineage_generator: Arc<WorkflowLineageGenerator>,
            manual_mapping_store: None,
    pub rdf_store: Arc<GraphicaRdfStore>,
}

// Request/Response DTOs

#[derive(Debug, Deserialize)]
struct RegisterWorkflowRequest {
    pub name: String,
    pub steps: Vec<WorkflowStepDef>,
}

#[derive(Debug, Deserialize)]
struct WorkflowStepDef {
    pub id: String,
    pub step_type: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct RegisterWorkflowResponse {
    pub workflow_id: String,
    pub workflow_uri: String,
    pub triples_created: usize,
}

#[derive(Debug, Deserialize)]
struct ExecuteWorkflowRequest {
    pub entity_id: String,
    pub input_data: serde_json::Value,
    #[serde(default)]
    pub confidence_threshold: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ExecuteWorkflowResponse {
    pub execution_id: String,
    pub execution_uri: String,
    pub success: bool,
    pub confidence: f64,
    pub fields_modified: usize,
    pub predictions_made: usize,
    pub triples_generated: usize,
    pub lineage_depth: usize,
}

#[derive(Debug, Deserialize)]
struct GetExecutionsQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub min_confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ExecutionSummary {
    pub execution_id: String,
    pub execution_uri: String,
    pub started_at: String,
    pub completed_at: String,
    pub confidence: f64,
    pub success: bool,
}

#[derive(Debug, Deserialize)]
struct LineageQuery {
    pub format: Option<String>, // json, turtle, jsonld
}

#[derive(Debug, Serialize)]
struct TransformationDetails {
    pub transform_id: String,
    pub transform_uri: String,
    pub transformer_name: String,
    pub fields_modified: Vec<FieldModificationDetail>,
    pub is_reversible: bool,
    pub applied_at: String,
}

#[derive(Debug, Serialize)]
struct FieldModificationDetail {
    pub field_name: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
    pub confidence: f64,
}

// REST Endpoint Implementations

/// Register a new workflow definition (creates RDF triples)
async fn register_workflow(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterWorkflowRequest>,
) -> impl IntoResponse {
    let workflow_id = format!("wf_{}", uuid::Uuid::new_v4());
    let workflow_uri = format!("http://graphica.io/workflow#{}", workflow_id);

    // Generate RDF triples for workflow definition
    let mut triples = vec![];

    // Workflow type and metadata
    triples.push((
        workflow_uri.clone(),
        "rdf:type".to_string(),
        "wf:Workflow".to_string(),
    ));

    triples.push((
        workflow_uri.clone(),
        "wf:name".to_string(),
        request.name.clone(),
    ));

    // Add steps
    for (idx, step) in request.steps.iter().enumerate() {
        let step_uri = format!("{}/step/{}", workflow_uri, step.id);

        triples.push((step_uri.clone(), "rdf:type".to_string(), "wf:WorkflowStep".to_string()));

        triples.push((
            workflow_uri.clone(),
            "wf:hasStep".to_string(),
            step_uri.clone(),
        ));

        triples.push((
            step_uri.clone(),
            "wf:stepIndex".to_string(),
            idx.to_string(),
        ));

        triples.push((
            step_uri.clone(),
            "wf:stepType".to_string(),
            step.step_type.clone(),
        ));

        // Store config as JSON
        triples.push((
            step_uri,
            "wf:config".to_string(),
            step.config.to_string(),
        ));
    }

    // Store in RDF store
    match state
        .rdf_store
        .insert_triples(&triples, Some("workflows"))
        .await
    {
        Ok(_) => {
            let response = RegisterWorkflowResponse {
                workflow_id,
                workflow_uri,
                triples_created: triples.len(),
            };
            (StatusCode::CREATED, Json(response))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegisterWorkflowResponse {
                workflow_id: String::new(),
                workflow_uri: String::new(),
                triples_created: 0,
            }),
        ),
    }
}

/// Execute workflow on entity (generates comprehensive lineage triples)
async fn execute_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<ExecuteWorkflowRequest>,
) -> impl IntoResponse {
    let execution_id = format!("exec_{}", uuid::Uuid::new_v4());

    // Execute workflow using the workflow executor
    let result = state
        .workflow_executor
        .execute(
            &workflow_id,
            &request.entity_id,
            request.input_data,
            request.confidence_threshold,
        )
        .await;

    match result {
        Ok(execution) => {
            // Generate comprehensive lineage triples
            let lineage_result = state
                .lineage_generator
                .generate_execution_lineage(&execution, &request.entity_id)
                .await
                .unwrap();

            let response = ExecuteWorkflowResponse {
                execution_id: execution.id,
                execution_uri: lineage_result.execution_uri,
                success: true,
                confidence: execution.output_confidence,
                fields_modified: lineage_result.field_modifications.len(),
                predictions_made: lineage_result.predictions.len(),
                triples_generated: lineage_result.triples_generated,
                lineage_depth: lineage_result.lineage_depth,
            };

            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = ExecuteWorkflowResponse {
                execution_id,
                execution_uri: String::new(),
                success: false,
                confidence: 0.0,
                fields_modified: 0,
                predictions_made: 0,
                triples_generated: 0,
                lineage_depth: 0,
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// Get workflow executions with SPARQL query
async fn get_executions(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Query(query): Query<GetExecutionsQuery>,
) -> impl IntoResponse {
    let workflow_uri = format!("http://graphica.io/workflow#{}", workflow_id);

    // Build SPARQL query
    let mut sparql = format!(
        r#"
        PREFIX wf: <http://graphica.io/workflow#>
        PREFIX prov: <http://www.w3.org/ns/prov#>

        SELECT ?execution ?started ?completed ?confidence ?success
        WHERE {{
            ?execution wf:executedWorkflow <{workflow_uri}> ;
                      prov:startedAtTime ?started ;
                      prov:endedAtTime ?completed ;
                      wf:outputConfidence ?confidence ;
                      wf:success ?success .
        "#
    );

    // Add filters
    if let Some(start) = query.start_date {
        sparql.push_str(&format!("FILTER(?started >= \"{}\"^^xsd:dateTime) ", start));
    }

    if let Some(end) = query.end_date {
        sparql.push_str(&format!("FILTER(?completed <= \"{}\"^^xsd:dateTime) ", end));
    }

    if let Some(min_conf) = query.min_confidence {
        sparql.push_str(&format!("FILTER(?confidence >= {}) ", min_conf));
    }

    sparql.push_str("} ORDER BY DESC(?started) LIMIT 100");

    // Execute SPARQL query
    match state.rdf_store.query(&sparql).await {
        Ok(results) => {
            let executions: Vec<ExecutionSummary> = results
                .into_iter()
                .map(|binding| ExecutionSummary {
                    execution_id: extract_id(&binding["execution"]),
                    execution_uri: binding["execution"].as_str().unwrap_or("").to_string(),
                    started_at: binding["started"].as_str().unwrap_or("").to_string(),
                    completed_at: binding["completed"].as_str().unwrap_or("").to_string(),
                    confidence: binding["confidence"].as_f64().unwrap_or(0.0),
                    success: binding["success"].as_bool().unwrap_or(false),
                })
                .collect();

            Json(executions)
        }
        Err(_) => Json(vec![]),
    }
}

/// Get complete lineage for workflow execution
async fn get_execution_lineage(
    State(state): State<Arc<ApiState>>,
    Path((workflow_id, exec_id)): Path<(String, String)>,
    Query(query): Query<LineageQuery>,
) -> impl IntoResponse {
    let format = query.format.as_deref().unwrap_or("json");
    let exec_uri = format!("http://graphica.io/workflow#execution/{}", exec_id);

    // SPARQL query for complete lineage
    let sparql = format!(
        r#"
        PREFIX wf: <http://graphica.io/workflow#>
        PREFIX prov: <http://www.w3.org/ns/prov#>
        PREFIX gph: <http://graphica.io/ontology#>

        CONSTRUCT {{
            <{exec_uri}> ?p ?o .
            ?step wf:partOfExecution <{exec_uri}> .
            ?step ?sp ?so .
            ?transform prov:wasPartOf ?step .
            ?transform ?tp ?to .
            ?modification ?mp ?mo .
            ?entity prov:wasGeneratedBy <{exec_uri}> .
        }}
        WHERE {{
            <{exec_uri}> ?p ?o .

            OPTIONAL {{
                ?step wf:partOfExecution <{exec_uri}> .
                ?step ?sp ?so .

                OPTIONAL {{
                    ?transform prov:wasPartOf ?step .
                    ?transform ?tp ?to .

                    OPTIONAL {{
                        ?transform wf:modifiedField ?modification .
                        ?modification ?mp ?mo .
                    }}
                }}
            }}

            OPTIONAL {{
                ?entity prov:wasGeneratedBy <{exec_uri}> .
            }}
        }}
        "#
    );

    match state.rdf_store.construct_query(&sparql).await {
        Ok(graph) => {
            match format {
                "turtle" => {
                    // Serialize as Turtle
                    let turtle = serialize_to_turtle(&graph);
                    (StatusCode::OK, turtle).into_response()
                }
                "jsonld" => {
                    // Serialize as JSON-LD
                    let jsonld = serialize_to_jsonld(&graph);
                    Json(jsonld).into_response()
                }
                _ => {
                    // Default to JSON
                    Json(graph).into_response()
                }
            }
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({}))).into_response(),
    }
}

/// Analyze workflow impact via SPARQL
async fn analyze_impact(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> impl IntoResponse {
    match state
        .lineage_generator
        .query_workflow_impact(&workflow_id, None, None)
        .await
    {
        Ok(impact) => Json(json!({
            "total_entities": impact.total_entities,
            "total_fields_modified": impact.total_fields_modified,
            "total_predictions": impact.total_predictions,
            "average_confidence": impact.average_confidence,
        })),
        Err(_) => Json(json!({
            "error": "Failed to analyze workflow impact"
        })),
    }
}

/// Get transformation details with before/after values
async fn get_transformation(
    State(state): State<Arc<ApiState>>,
    Path(transform_id): Path<String>,
) -> impl IntoResponse {
    let transform_uri = format!("http://graphica.io/workflow#transform/{}", transform_id);

    let sparql = format!(
        r#"
        PREFIX wf: <http://graphica.io/workflow#>
        PREFIX prov: <http://www.w3.org/ns/prov#>

        SELECT ?transformer ?field ?old_value ?new_value ?confidence ?reversible ?timestamp
        WHERE {{
            <{transform_uri}> wf:transformerName ?transformer ;
                             wf:isReversible ?reversible ;
                             wf:modifiedField ?modification .

            ?modification wf:fieldName ?field ;
                        wf:oldValue ?old_value ;
                        wf:newValue ?new_value ;
                        wf:fieldConfidence ?confidence ;
                        prov:atTime ?timestamp .
        }}
        "#
    );

    match state.rdf_store.query(&sparql).await {
        Ok(results) => {
            if let Some(first) = results.first() {
                let fields: Vec<FieldModificationDetail> = results
                    .into_iter()
                    .map(|binding| FieldModificationDetail {
                        field_name: binding["field"].as_str().unwrap_or("").to_string(),
                        old_value: parse_json_value(&binding["old_value"]),
                        new_value: parse_json_value(&binding["new_value"]),
                        confidence: binding["confidence"].as_f64().unwrap_or(0.0),
                    })
                    .collect();

                let details = TransformationDetails {
                    transform_id: transform_id.clone(),
                    transform_uri,
                    transformer_name: first["transformer"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    fields_modified: fields,
                    is_reversible: first["reversible"].as_bool().unwrap_or(false),
                    applied_at: first["timestamp"].as_str().unwrap_or("").to_string(),
                };

                Json(details)
            } else {
                Json(TransformationDetails {
                    transform_id,
                    transform_uri,
                    transformer_name: String::new(),
                    fields_modified: vec![],
                    is_reversible: false,
                    applied_at: String::new(),
                })
            }
        }
        Err(_) => Json(TransformationDetails {
            transform_id,
            transform_uri,
            transformer_name: String::new(),
            fields_modified: vec![],
            is_reversible: false,
            applied_at: String::new(),
        }),
    }
}

/// Reverse a transformation (if reversible)
async fn reverse_transformation(
    State(state): State<Arc<ApiState>>,
    Path(transform_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> impl IntoResponse {
    let reason = request["reason"].as_str().unwrap_or("User requested");

    // Query for reversal data
    let transform_uri = format!("http://graphica.io/workflow#transform/{}", transform_id);

    let sparql = format!(
        r#"
        PREFIX wf: <http://graphica.io/workflow#>

        SELECT ?reversalData ?field ?old_value
        WHERE {{
            <{transform_uri}> wf:isReversible true ;
                             wf:reversalData ?reversalData ;
                             wf:modifiedField ?modification .

            ?modification wf:fieldName ?field ;
                        wf:oldValue ?old_value .
        }}
        "#
    );

    match state.rdf_store.query(&sparql).await {
        Ok(results) => {
            if results.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Transformation is not reversible or not found"
                    })),
                );
            }

            // Create reversal transformation
            let reversal_id = format!("reversal_{}", uuid::Uuid::new_v4());
            let mut fields_restored = vec![];

            for binding in results {
                let field = binding["field"].as_str().unwrap_or("").to_string();
                let old_value = parse_json_value(&binding["old_value"]);

                fields_restored.push(json!({
                    "field": field,
                    "restored_value": old_value
                }));
            }

            // Record reversal in RDF
            let reversal_uri = format!("http://graphica.io/workflow#transform/{}", reversal_id);
            let mut triples = vec![];

            triples.push((
                reversal_uri.clone(),
                "rdf:type".to_string(),
                "wf:TransformReversal".to_string(),
            ));

            triples.push((
                reversal_uri.clone(),
                "wf:reverses".to_string(),
                transform_uri.clone(),
            ));

            triples.push((
                reversal_uri.clone(),
                "wf:reversalReason".to_string(),
                reason.to_string(),
            ));

            triples.push((
                reversal_uri,
                "prov:atTime".to_string(),
                chrono::Utc::now().to_rfc3339(),
            ));

            // Mark original as reversed
            triples.push((
                transform_uri,
                "wf:reversedBy".to_string(),
                format!("http://graphica.io/workflow#transform/{}", reversal_id),
            ));

            state
                .rdf_store
                .insert_triples(&triples, Some("reversals"))
                .await
                .ok();

            (
                StatusCode::OK,
                Json(json!({
                    "reversal_id": reversal_id,
                    "fields_restored": fields_restored
                })),
            )
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Failed to reverse transformation"
            })),
        ),
    }
}

/// Get all workflows that processed an entity
async fn get_entity_workflows(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> impl IntoResponse {
    let entity_uri = format!("http://graphica.io/ontology#entity/{}", entity_id);

    let sparql = format!(
        r#"
        PREFIX wf: <http://graphica.io/workflow#>
        PREFIX prov: <http://www.w3.org/ns/prov#>

        SELECT ?workflow ?execution ?timestamp
               (COUNT(DISTINCT ?transform) as ?transformations)
               (COUNT(DISTINCT ?prediction) as ?predictions)
        WHERE {{
            <{entity_uri}> prov:wasGeneratedBy ?execution .

            ?execution wf:executedWorkflow ?workflow ;
                      prov:endedAtTime ?timestamp .

            OPTIONAL {{
                ?transform prov:wasPartOf ?step1 .
                ?step1 wf:partOfExecution ?execution .
                ?transform a wf:TransformAction .
            }}

            OPTIONAL {{
                ?prediction prov:wasPartOf ?step2 .
                ?step2 wf:partOfExecution ?execution .
                ?prediction a gph:DerivedAttribute .
            }}
        }}
        GROUP BY ?workflow ?execution ?timestamp
        ORDER BY DESC(?timestamp)
        "#
    );

    match state.rdf_store.query(&sparql).await {
        Ok(results) => {
            let workflows: Vec<_> = results
                .into_iter()
                .map(|binding| {
                    json!({
                        "workflow_id": extract_id(&binding["workflow"]),
                        "execution_id": extract_id(&binding["execution"]),
                        "executed_at": binding["timestamp"].as_str().unwrap_or(""),
                        "transformations_applied": binding["transformations"].as_u64().unwrap_or(0),
                        "predictions_made": binding["predictions"].as_u64().unwrap_or(0),
                    })
                })
                .collect();

            Json(workflows)
        }
        Err(_) => Json(vec![]),
    }
}

/// Get field modification history
async fn get_field_history(
    State(state): State<Arc<ApiState>>,
    Path((entity_id, field_name)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .lineage_generator
        .query_field_history(&entity_id, &field_name)
        .await
    {
        Ok(history) => {
            let items: Vec<_> = history
                .into_iter()
                .map(|h| {
                    json!({
                        "timestamp": h.timestamp.to_rfc3339(),
                        "old_value": h.old_value,
                        "new_value": h.new_value,
                        "confidence": h.confidence,
                        "workflow_id": h.workflow_id,
                        "transform_id": h.transform_id,
                    })
                })
                .collect();

            Json(items)
        }
        Err(_) => Json(vec![]),
    }
}

// Helper functions

fn extract_id(value: &serde_json::Value) -> String {
    value
        .as_str()
        .and_then(|s| s.split('#').last().or_else(|| s.split('/').last()))
        .unwrap_or("")
        .to_string()
}

fn parse_json_value(value: &serde_json::Value) -> serde_json::Value {
    if let Some(s) = value.as_str() {
        serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.to_string()))
    } else {
        value.clone()
    }
}

fn serialize_to_turtle(graph: &serde_json::Value) -> String {
    // Simplified turtle serialization
    format!("# RDF Turtle\n{}", serde_json::to_string_pretty(graph).unwrap())
}

fn serialize_to_jsonld(graph: &serde_json::Value) -> serde_json::Value {
    // Add JSON-LD context
    json!({
        "@context": {
            "wf": "http://graphica.io/workflow#",
            "prov": "http://www.w3.org/ns/prov#",
            "gph": "http://graphica.io/ontology#",
            "ml": "http://graphica.io/ml#"
        },
        "@graph": graph
    })
}

// ============== gRPC SERVICE ==============

use crate::orchestration::grpc::workflow::{
    workflow_service_server::{WorkflowService, WorkflowServiceServer},
    AnalyzeImpactRequest, AnalyzeImpactResponse, ExecuteWorkflowRequest as GrpcExecuteRequest,
    ExecuteWorkflowResponse as GrpcExecuteResponse, ExecutionEvent, FieldModified,
    GetTransformationRequest, ImpactAnalysis, QueryLineageRequest, QueryLineageResponse,
    ReverseTransformationRequest, ReverseTransformationResponse, StreamExecutionRequest,
    TransformationDetails as GrpcTransformDetails,
};

pub struct WorkflowGrpcService {
    workflow_executor: Arc<WorkflowExecutor>,
    lineage_generator: Arc<WorkflowLineageGenerator>,
            manual_mapping_store: None,
    rdf_store: Arc<GraphicaRdfStore>,
}

impl WorkflowGrpcService {
    pub fn new(
        workflow_executor: Arc<WorkflowExecutor>,
        lineage_generator: Arc<WorkflowLineageGenerator>,
            manual_mapping_store: None,
        rdf_store: Arc<GraphicaRdfStore>,
    ) -> Self {
        Self {
            workflow_executor,
            lineage_generator,
            rdf_store,
        }
    }

    pub fn into_service(self) -> WorkflowServiceServer<Self> {
        WorkflowServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl WorkflowService for WorkflowGrpcService {
    async fn execute_workflow(
        &self,
        request: Request<GrpcExecuteRequest>,
    ) -> Result<Response<GrpcExecuteResponse>, Status> {
        let req = request.into_inner();

        let execution = self
            .workflow_executor
            .execute(
                &req.workflow_id,
                &req.entity_id,
                serde_json::from_str(&req.input_data).unwrap(),
                req.confidence_threshold,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let lineage_result = self
            .lineage_generator
            .generate_execution_lineage(&execution, &req.entity_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = GrpcExecuteResponse {
            execution_id: execution.id,
            execution_uri: lineage_result.execution_uri,
            success: true,
            output_confidence: execution.output_confidence,
            fields_modified: lineage_result.field_modifications.len() as i32,
            predictions_made: lineage_result.predictions.len() as i32,
            triples_generated: lineage_result.triples_generated as i32,
            output_data: serde_json::to_string(&execution.output_data).unwrap(),
            modifications: vec![], // Would populate with actual modifications
            predictions: vec![],   // Would populate with actual predictions
        };

        Ok(Response::new(response))
    }

    type StreamExecutionStream =
        tokio_stream::wrappers::ReceiverStream<Result<ExecutionEvent, Status>>;

    async fn stream_execution(
        &self,
        request: Request<StreamExecutionRequest>,
    ) -> Result<Response<Self::StreamExecutionStream>, Status> {
        let req = request.into_inner();

        // Create channel for streaming events
        let (tx, rx) = tokio::sync::mpsc::channel(128);

        // Spawn task to execute workflow and stream events
        let executor = self.workflow_executor.clone();
        let entity_id = req.entity_id.clone();

        tokio::spawn(async move {
            // Execute workflow with event streaming
            // This would integrate with the actual workflow executor
            // to emit events as steps complete

            // Example event
            let event = ExecutionEvent {
                timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                step_id: "step_001".to_string(),
                step_type: "transform".to_string(),
                event: Some(crate::orchestration::grpc::workflow::execution_event::Event::FieldModified(
                    FieldModified {
                        field_name: "email".to_string(),
                        old_value: "OLD@EXAMPLE.COM".to_string(),
                        new_value: "old@example.com".to_string(),
                        confidence: 1.0,
                    },
                )),
            };

            tx.send(Ok(event)).await.ok();
        });

        Ok(Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn query_lineage(
        &self,
        request: Request<QueryLineageRequest>,
    ) -> Result<Response<QueryLineageResponse>, Status> {
        let req = request.into_inner();
        let exec_uri = format!("http://graphica.io/workflow#execution/{}", req.execution_id);

        // Use provided SPARQL or default template
        let sparql = if !req.sparql_query.is_empty() {
            req.sparql_query
        } else {
            // Default full lineage query
            format!(
                r#"
                CONSTRUCT {{
                    <{exec_uri}> ?p ?o .
                    ?s ?p2 ?o2 .
                }}
                WHERE {{
                    <{exec_uri}> ?p ?o .
                    OPTIONAL {{ ?s ?p2 ?o2 . FILTER(CONTAINS(STR(?s), "{exec_uri}")) }}
                }}
                "#
            )
        };

        let graph = self
            .rdf_store
            .construct_query(&sparql)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = QueryLineageResponse {
            execution_id: req.execution_id,
            lineage_graph: serde_json::to_string(&graph).unwrap(),
            format: req.format,
        };

        Ok(Response::new(response))
    }

    async fn get_transformation(
        &self,
        request: Request<GetTransformationRequest>,
    ) -> Result<Response<GrpcTransformDetails>, Status> {
        // Implementation would query RDF store for transformation details
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn reverse_transformation(
        &self,
        request: Request<ReverseTransformationRequest>,
    ) -> Result<Response<ReverseTransformationResponse>, Status> {
        // Implementation would reverse transformation using stored data
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn analyze_impact(
        &self,
        request: Request<AnalyzeImpactRequest>,
    ) -> Result<Response<ImpactAnalysis>, Status> {
        let req = request.into_inner();

        let impact = self
            .lineage_generator
            .query_workflow_impact(&req.workflow_id, None, None)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = ImpactAnalysis {
            workflow_id: req.workflow_id,
            total_entities: impact.total_entities as i32,
            total_fields_modified: impact.total_fields_modified as i32,
            total_predictions: impact.total_predictions as i32,
            average_confidence: impact.average_confidence,
        };

        Ok(Response::new(response))
    }
}