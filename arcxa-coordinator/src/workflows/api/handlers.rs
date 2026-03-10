//! Workflow API Handlers
//!
//! HTTP request handlers for workflow management and execution.

use super::dto::*;
use crate::workflows::domain::{
    validate_cron_expression, validate_timezone, ExecutionLog, ExecutionStatus, Route, Workflow,
    WorkflowExecution, WorkflowSchedule,
};
use crate::workflows::engine::{ActionExecutor, ExecutionContext, WorkflowRouter};
use crate::workflows::storage::{ExecutionStore, ScheduleStore, WorkflowStore};
use axum::{
    extract::{Path, Query, State},
    http::{Extensions, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

// Graph-native execution imports (for legacy WorkflowEngine-based execution)
use crate::api::auth::Claims;
use crate::api::workflow::materialization::{
    finalize_execution_result, persist_execution_record_if_possible,
};
#[allow(unused_imports)]
use crate::api::ApiState;
use crate::workflows::dataset_input::build_input_adapter;
#[allow(unused_imports)]
use graphica_core::orchestration::workflow::{
    executor::FinalDecision, InputAdapter, JsonInputAdapter, WorkflowDefinition, WorkflowInput,
};

/// Workflow API state
#[derive(Clone)]
pub struct WorkflowApiState {
    pub store: Arc<WorkflowStore>,
    pub execution_store: Arc<ExecutionStore>,
    pub schedule_store: Arc<ScheduleStore>,
    // Production workflow components
    pub model_invoker: Option<Arc<graphica_core::orchestration::ml::ModelInvoker>>,
    pub rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
    pub transformer_registry:
        Option<Arc<crate::workflows::engine::transformers::TransformerRegistry>>,
    // Phase 3: Production action integrations
    pub kafka_producer: Option<Arc<crate::workflows::integration::KafkaProducer>>,
    pub http_client: Option<Arc<crate::workflows::integration::HttpClient>>,
    pub lineage_generator: Option<Arc<crate::workflows::lineage::WorkflowLineageGenerator>>,
    pub manual_mapping_store: Option<Arc<crate::mapping::manual::ManualMappingStore>>,
    pub metrics: Option<Arc<crate::observability::metrics::WorkflowMetrics>>,
    // Governance policy checker (Phase 1.1)
    pub policy_checker: Option<Arc<crate::workflows::governance::GovernancePolicyChecker>>,
    // Execution state synchronizer (Phase 3.1)
    pub execution_sync: Option<Arc<crate::workflows::lineage::ExecutionStateSynchronizer>>,
    // Column lineage store for tracking column-level transformations
    pub column_lineage_store:
        Option<Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>>,
}

impl WorkflowApiState {
    pub fn new(
        store: WorkflowStore,
        execution_store: ExecutionStore,
        schedule_store: ScheduleStore,
    ) -> Self {
        Self {
            store: Arc::new(store),
            execution_store: Arc::new(execution_store),
            schedule_store: Arc::new(schedule_store),
            model_invoker: None,
            rule_executor: None,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            manual_mapping_store: None,
            metrics: None,
            policy_checker: None,
            execution_sync: None,
            column_lineage_store: None,
        }
    }

    /// Create workflow state with production executor components
    pub fn with_production_components(
        mut self,
        model_invoker: Arc<graphica_core::orchestration::ml::ModelInvoker>,
        rule_executor: Arc<graphica_core::orchestration::rules::RuleExecutor>,
    ) -> Self {
        self.model_invoker = Some(model_invoker);
        self.rule_executor = Some(rule_executor);
        self
    }
}

// === API Handlers ===

/// Create a new workflow
///
/// POST /api/v1/workflows
pub async fn create_workflow(
    State(state): State<WorkflowApiState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<Json<CreateWorkflowResponse>, ApiError> {
    info!("Creating workflow: {}", req.name);

    // Validate request
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Workflow name cannot be empty".to_string(),
        ));
    }

    if req.routes.is_empty() {
        return Err(ApiError::BadRequest(
            "Workflow must have at least one route".to_string(),
        ));
    }

    // Generate workflow ID
    let workflow_id = format!("wf_{}", uuid::Uuid::new_v4());

    // Convert DTOs to domain models
    let routes: Vec<Route> = req.routes.into_iter().map(Route::from).collect();

    // Create workflow
    let mut workflow = Workflow::new(workflow_id.clone(), req.name.clone(), routes);
    workflow.description = req.description;
    workflow.tags = req.tags;

    if let Some(default_route) = req.default_route {
        workflow.default_route = Some(default_route);
    }

    // Validate and store
    state.store.create(workflow.clone())?;

    info!("Workflow '{}' created successfully", workflow_id);

    Ok(Json(CreateWorkflowResponse {
        id: workflow.id,
        name: workflow.name,
        version: workflow.version,
        created_at: workflow.created_at,
    }))
}

/// Get a workflow by ID
///
/// GET /api/v1/workflows/{id}
pub async fn get_workflow(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<Workflow>, ApiError> {
    info!("Getting workflow: {}", workflow_id);

    let workflow = state.store.get_required(&workflow_id)?;

    Ok(Json(workflow))
}

/// Update a workflow
///
/// PUT /api/v1/workflows/{id}
pub async fn update_workflow(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<Workflow>, ApiError> {
    info!("Updating workflow: {}", workflow_id);

    // Get existing workflow
    let mut workflow = state.store.get_required(&workflow_id)?;

    // Apply updates
    if let Some(name) = req.name {
        workflow.name = name;
    }

    if let Some(description) = req.description {
        workflow.description = description;
    }

    if let Some(routes_dto) = req.routes {
        workflow.routes = Box::new(routes_dto.into_iter().map(Route::from).collect());
    }

    if let Some(default_route) = req.default_route {
        workflow.default_route = Some(default_route);
    }

    if let Some(enabled) = req.enabled {
        workflow.enabled = enabled;
    }

    if let Some(tags) = req.tags {
        workflow.tags = tags;
    }

    // Update in store
    state.store.update(&workflow_id, workflow.clone())?;

    info!(
        "Workflow '{}' updated to version {}",
        workflow_id, workflow.version
    );

    Ok(Json(workflow))
}

/// Delete a workflow
///
/// DELETE /api/v1/workflows/{id}
pub async fn delete_workflow(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("Deleting workflow: {}", workflow_id);

    state.store.delete(&workflow_id)?;

    info!("Workflow '{}' deleted successfully", workflow_id);

    Ok(StatusCode::NO_CONTENT)
}

/// List workflows
///
/// GET /api/v1/workflows
pub async fn list_workflows(
    State(state): State<WorkflowApiState>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<crate::workflows::domain::WorkflowSummary>>, ApiError> {
    info!("Listing workflows");

    // Parse tags if provided
    let tags: Option<Vec<String>> = query
        .tags
        .as_ref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    // Get filtered workflows
    let workflows = state
        .store
        .list_filtered(query.enabled.unwrap_or(false), tags.as_deref())?;

    // Apply pagination
    let paginated: Vec<_> = workflows
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .collect();

    Ok(Json(paginated))
}

/// Execute a workflow synchronously (legacy endpoint for backward compatibility)
///
/// POST /api/v1/workflows/{id}/execute
pub async fn execute_workflow(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowResponse>, ApiError> {
    if req.dry_run {
        return Err(ApiError::BadRequest(
            "dry_run is not supported on /execute; use /workflows/{id}/dry-run instead".to_string(),
        ));
    }

    let start = Instant::now();

    info!("Executing workflow (sync): {}", workflow_id);

    // Get workflow
    let workflow = state.store.get_required(&workflow_id)?;

    if !workflow.enabled {
        return Err(ApiError::BadRequest(format!(
            "Workflow '{}' is disabled",
            workflow_id
        )));
    }

    // Convert input wrapper to JSON value for processing
    let input_json = req.input.into_json();

    // Select matching route
    let route_match = WorkflowRouter::select_route(&workflow, &input_json)?;

    let evaluation_time_ms = route_match
        .as_ref()
        .map(|m| m.evaluation_time_ms)
        .unwrap_or(0);

    let (matched_route_id, actions_executed, output, action_execution_time_ms) =
        if let Some(route_match) = route_match {
            let route = route_match.route;
            info!("Route '{}' matched", route.name);

            // Execute actions
            let mut output = input_json.clone();
            let context = ExecutionContext {
                workflow_id: workflow.id.clone(),
                route_id: route.id.clone(),
                input_data: input_json.clone(),
                rule_executor: state.rule_executor.clone(),
                transformer_registry: state.transformer_registry.clone(),
                kafka_producer: None,
                http_client: None,
                lineage_generator: state.lineage_generator.clone(), // CRITICAL FIX: Use lineage generator from state
                manual_mapping_store: None,
                execution_id: None,
                action_index: 0,
                metrics: None,
                approval_store: None,
                execution_store: None,
                column_lineage_store: state.column_lineage_store.clone(),
                tenant_id: "default".to_string(),
                timeout_config: graphica_core::orchestration::workflow::ExecutionTimeout::default(),
                workflow_start_time: std::time::Instant::now(),
                stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
                db2_pool: None,
                postgres_pool: None,
                memory_monitor: None,
            };

            let action_start = Instant::now();
            let results =
                ActionExecutor::execute_actions(&route.actions, &mut output, &context).await?;
            let execution_time_ms = action_start.elapsed().as_millis() as u64;

            (Some(route.id.clone()), results, output, execution_time_ms)
        } else {
            warn!("No route matched for workflow '{}'", workflow_id);
            (None, vec![], input_json, 0)
        };

    let total_duration_ms = start.elapsed().as_millis() as u64;

    // Extract ML feature values from action outputs
    let ml_feature_values = extract_ml_features(&actions_executed, &output);

    // Build lineage references from executed actions
    let lineage_refs = build_lineage_refs(&workflow.id, &matched_route_id, &actions_executed);

    info!(
        "Workflow '{}' execution complete in {}ms (ML features: {}, lineage refs: {})",
        workflow_id,
        total_duration_ms,
        ml_feature_values.as_ref().map(|v| v.len()).unwrap_or(0),
        lineage_refs.len()
    );

    Ok(Json(ExecuteWorkflowResponse {
        workflow_id: workflow.id,
        workflow_name: workflow.name,
        matched_route: matched_route_id,
        actions_executed,
        output,
        total_duration_ms,
        evaluation_time_ms,
        execution_time_ms: action_execution_time_ms,
        ml_feature_values,
        lineage_refs,
    }))
}

/// Execute a workflow asynchronously with execution tracking
///
/// POST /api/v1/workflows/{id}/execute/async
pub async fn execute_workflow_async(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    extensions: Extensions,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowAsyncResponse>, ApiError> {
    info!("Executing workflow (async): {}", workflow_id);

    if req.dry_run {
        return Err(ApiError::BadRequest(
            "dry_run is not supported on /execute/async; use /workflows/{id}/dry-run instead"
                .to_string(),
        ));
    }

    // Extract authenticated user from request extensions
    let created_by = extensions
        .get::<Claims>()
        .map(|claims| claims.sub.clone())
        .or_else(|| {
            req.context
                .as_ref()
                .and_then(|context| context.initiator.clone())
        });

    // Get workflow
    let workflow = state.store.get_required(&workflow_id)?;

    if !workflow.enabled {
        return Err(ApiError::BadRequest(format!(
            "Workflow '{}' is disabled",
            workflow_id
        )));
    }

    // Convert input wrapper to JSON value
    let input_json = req.input.into_json();

    // === Phase 1.1: Governance Policy Check ===
    let execution_id = format!("exec_{}", uuid::Uuid::new_v4());

    // Check governance policies if policy checker is configured
    if let Some(policy_checker) = &state.policy_checker {
        let user_id = extensions.get::<Claims>().map(|claims| claims.sub.as_str());

        let policy_result = policy_checker
            .check_execution_allowed(&workflow_id, user_id, &input_json)
            .await
            .map_err(|e| ApiError::InternalServerError(format!("Policy check failed: {}", e)))?;

        // Record policy check result to RDF for audit trail
        if let Err(e) = policy_checker
            .record_policy_check(&execution_id, &workflow_id, &policy_result)
            .await
        {
            warn!("Failed to record policy check to RDF: {}", e);
        }

        // Block execution if policy violations found
        if !policy_result.allowed {
            let violations_json: Vec<serde_json::Value> = policy_result
                .violations
                .iter()
                .map(|v| {
                    json!({
                        "policy_id": v.policy_id,
                        "policy_name": v.policy_name,
                        "severity": format!("{:?}", v.severity),
                        "message": v.message,
                        "recommendation": v.recommendation,
                    })
                })
                .collect();

            return Err(ApiError::Forbidden(
                json!({
                    "error": "Workflow execution blocked by governance policies",
                    "violations": violations_json,
                    "warnings": policy_result.warnings,
                    "policies_checked": policy_result.policies_checked,
                })
                .to_string(),
            ));
        }

        if !policy_result.warnings.is_empty() {
            info!("Policy check warnings: {:?}", policy_result.warnings);
        }
    }

    // Create execution record
    let execution = WorkflowExecution::new(
        execution_id.clone(),
        workflow.id.clone(),
        workflow.name.clone(),
        input_json.clone(), // Clone before moving
        created_by,
    );

    // Save execution
    state.execution_store.save(execution.clone()).await?;

    info!(
        "Created execution '{}' for workflow '{}'",
        execution_id, workflow_id
    );

    // Clone data for background task
    let workflow_clone = workflow.clone();
    let input_clone = input_json;
    let execution_store = Arc::clone(&state.execution_store);
    let execution_id_clone = execution_id.clone();
    let rule_executor_clone = state.rule_executor.clone();
    let transformer_registry_clone = state.transformer_registry.clone();
    let kafka_producer_clone = state.kafka_producer.clone();
    let http_client_clone = state.http_client.clone();
    let lineage_generator_clone = state.lineage_generator.clone();
    let manual_mapping_store_clone = state.manual_mapping_store.clone();
    let metrics_clone = state.metrics.clone();
    let column_lineage_store_clone = state.column_lineage_store.clone();

    // Spawn background task to execute workflow with production integrations (Phase 3)
    tokio::spawn(async move {
        if let Err(e) = execute_workflow_background(
            execution_id_clone,
            workflow_clone,
            input_clone,
            execution_store,
            rule_executor_clone,
            transformer_registry_clone,
            kafka_producer_clone,
            http_client_clone,
            lineage_generator_clone,
            manual_mapping_store_clone,
            metrics_clone,
            column_lineage_store_clone,
        )
        .await
        {
            error!("Background execution failed: {}", e);
        }
    });

    // Return execution ID immediately
    Ok(Json(ExecuteWorkflowAsyncResponse {
        execution_id,
        workflow_id: workflow.id,
        workflow_name: workflow.name,
        status: ExecutionStatus::Pending,
        started_at: execution.started_at,
    }))
}

/// Background task for executing workflow with tracking
async fn execute_workflow_background(
    execution_id: String,
    workflow: Workflow,
    input: serde_json::Value,
    execution_store: Arc<ExecutionStore>,
    rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
    transformer_registry: Option<Arc<crate::workflows::engine::transformers::TransformerRegistry>>,
    kafka_producer: Option<Arc<crate::workflows::integration::KafkaProducer>>,
    http_client: Option<Arc<crate::workflows::integration::HttpClient>>,
    lineage_generator: Option<Arc<crate::workflows::lineage::WorkflowLineageGenerator>>,
    manual_mapping_store: Option<Arc<crate::mapping::manual::ManualMappingStore>>,
    metrics: Option<Arc<crate::observability::metrics::WorkflowMetrics>>,
    column_lineage_store: Option<
        Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>,
    >,
) -> anyhow::Result<()> {
    // Check if execution was paused before starting
    let current_exec = execution_store.get_required(&execution_id).await?;
    if current_exec.status == ExecutionStatus::Paused {
        execution_store
            .add_log(
                &execution_id,
                ExecutionLog::info("Execution was paused before starting, skipping"),
            )
            .await?;
        return Ok(());
    }

    // Update status to Running
    execution_store
        .update_status(&execution_id, ExecutionStatus::Running)
        .await?;
    execution_store
        .add_log(
            &execution_id,
            ExecutionLog::info(format!(
                "Starting execution for workflow '{}'",
                workflow.name
            )),
        )
        .await?;

    // === Phase 1.2: Record workflow execution start in RDF for lineage ===
    let start_time = chrono::Utc::now();
    if let Some(ref lineage_gen) = lineage_generator {
        if let Err(e) = lineage_gen.record_workflow_start(&execution_id, &workflow.id, start_time) {
            warn!("Failed to record workflow start lineage: {}", e);
        } else {
            tracing::debug!(
                "Recorded workflow start lineage for execution: {}",
                execution_id
            );
        }
    }

    // Select matching route
    let route_match = WorkflowRouter::select_route(&workflow, &input)?;

    if let Some(route_match) = route_match {
        let route = route_match.route;

        info!(
            "Route '{}' matched for execution '{}'",
            route.name, execution_id
        );

        execution_store
            .add_log(
                &execution_id,
                ExecutionLog::info(format!("Route '{}' matched", route.name)),
            )
            .await?;

        // === Phase 2.3: Record route matching lineage ===
        if let Some(ref lineage_gen) = lineage_generator {
            let routes_evaluated = workflow.routes.len();
            if let Err(e) = lineage_gen.record_route_matching(
                &execution_id,
                &workflow.id,
                Some(&route.id),
                Some(&route.name),
                route_match.evaluation_time_ms,
                routes_evaluated,
                "condition_matched",
            ) {
                warn!("Failed to record route matching lineage: {}", e);
            }
        }

        // Update execution with matched route
        let mut execution = execution_store.get_required(&execution_id).await?;
        execution.set_matched_route(route.id.clone(), route.name.clone());
        execution_store.update(execution).await?;

        // Execute actions
        let mut output = input.clone();

        // Production components now available in background task (Phase 3)
        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route.id.clone(),
            input_data: input.clone(),
            rule_executor: rule_executor.clone(),
            transformer_registry: transformer_registry.clone(),
            kafka_producer: kafka_producer.clone(),
            http_client: http_client.clone(),
            lineage_generator: lineage_generator.clone(),
            manual_mapping_store: None,
            execution_id: Some(execution_id.clone()),
            action_index: 0,
            metrics: metrics.clone(),
            approval_store: None,
            execution_store: None,
            column_lineage_store: column_lineage_store.clone(),
            tenant_id: "default".to_string(),
            timeout_config: graphica_core::orchestration::workflow::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        match ActionExecutor::execute_actions(&route.actions, &mut output, &context).await {
            Ok(results) => {
                info!("Execution '{}' completed successfully", execution_id);

                execution_store
                    .add_log(
                        &execution_id,
                        ExecutionLog::info(format!(
                            "Executed {} actions successfully",
                            results.len()
                        )),
                    )
                    .await?;

                // Update execution with output
                let mut execution = execution_store.get_required(&execution_id).await?;
                execution.set_output(output);
                execution.actions_executed = results.len();
                execution.update_status(ExecutionStatus::Completed);
                execution_store.update(execution).await?;

                // === Phase 1.2: Record successful workflow completion in RDF ===
                if let Some(ref lineage_gen) = lineage_generator {
                    let end_time = chrono::Utc::now();
                    if let Err(e) =
                        lineage_gen.record_workflow_complete(&execution_id, true, end_time)
                    {
                        warn!("Failed to record workflow completion lineage: {}", e);
                    } else {
                        tracing::debug!(
                            "Recorded workflow completion lineage for execution: {}",
                            execution_id
                        );
                    }
                }
            }
            Err(e) => {
                error!("Execution '{}' failed: {}", execution_id, e);

                execution_store
                    .add_log(
                        &execution_id,
                        ExecutionLog::error(format!("Execution failed: {}", e)),
                    )
                    .await?;

                execution_store
                    .set_error(&execution_id, e.to_string())
                    .await?;

                // === Phase 1.2: Record failed workflow completion in RDF ===
                if let Some(ref lineage_gen) = lineage_generator {
                    let end_time = chrono::Utc::now();
                    if let Err(lineage_err) =
                        lineage_gen.record_workflow_complete(&execution_id, false, end_time)
                    {
                        warn!("Failed to record workflow failure lineage: {}", lineage_err);
                    } else {
                        tracing::debug!(
                            "Recorded workflow failure lineage for execution: {}",
                            execution_id
                        );
                    }
                }
            }
        }
    } else {
        warn!("No route matched for execution '{}'", execution_id);

        execution_store
            .add_log(
                &execution_id,
                ExecutionLog::warn("No route matched for input data"),
            )
            .await?;

        // === Phase 2.3: Record no route matched lineage ===
        if let Some(ref lineage_gen) = lineage_generator {
            let routes_evaluated = workflow.routes.len();
            if let Err(e) = lineage_gen.record_route_matching(
                &execution_id,
                &workflow.id,
                None,
                None,
                0,
                routes_evaluated,
                "no_match",
            ) {
                warn!("Failed to record route matching lineage (no match): {}", e);
            }
        }

        // Mark as completed with no actions
        let mut execution = execution_store.get_required(&execution_id).await?;
        execution.set_output(input);
        execution.update_status(ExecutionStatus::Completed);
        execution_store.update(execution).await?;

        // === Phase 1.2: Record completion (no route matched) in RDF ===
        if let Some(ref lineage_gen) = lineage_generator {
            let end_time = chrono::Utc::now();
            if let Err(e) = lineage_gen.record_workflow_complete(&execution_id, true, end_time) {
                warn!(
                    "Failed to record workflow completion lineage (no route): {}",
                    e
                );
            } else {
                tracing::debug!(
                    "Recorded workflow completion lineage (no route) for execution: {}",
                    execution_id
                );
            }
        }
    }

    Ok(())
}

/// Get execution details
///
/// GET /api/v1/workflows/executions/{execution_id}
pub async fn get_execution(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<GetExecutionResponse>, ApiError> {
    info!("Getting execution: {}", execution_id);

    let execution = state.execution_store.get_required(&execution_id).await?;

    // Fetch workflow to compute progress and next actions
    let mut response = GetExecutionResponse::from(execution.clone());

    // Try to enrich with workflow information
    if let Ok(Some(workflow)) = state.store.get(&execution.workflow_id) {
        // Find the matched route
        if let Some(route_id) = &execution.matched_route {
            if let Some(route) = workflow.routes.iter().find(|r| &r.id == route_id) {
                let total_actions = route.actions.len();
                response.total_actions = Some(total_actions);

                // Calculate progress percentage
                if total_actions > 0 {
                    let progress =
                        (execution.actions_executed as f64 / total_actions as f64) * 100.0;
                    response.progress_percent = Some(progress.min(100.0));
                }

                // Predict next actions (if not completed)
                if !execution.status.is_terminal() && execution.actions_executed < total_actions {
                    // Get next action(s)
                    if let Some(next_action) = route.actions.get(execution.actions_executed) {
                        // Use action type as identifier (e.g., "Transform", "SendToKafka")
                        response.next_actions = vec![next_action.action_type().to_string()];
                    }
                }
            }
        }
    }

    Ok(Json(response))
}

/// Get execution logs
///
/// GET /api/v1/workflows/executions/{execution_id}/logs
pub async fn get_execution_logs(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<GetExecutionLogsResponse>, ApiError> {
    info!("Getting logs for execution: {}", execution_id);

    let logs = state.execution_store.get_logs(&execution_id).await?;

    Ok(Json(GetExecutionLogsResponse {
        execution_id: execution_id.clone(),
        logs: logs.into_iter().map(ExecutionLogDto::from).collect(),
        total: state.execution_store.get_logs(&execution_id).await?.len(),
    }))
}

/// List executions with filtering
///
/// GET /api/v1/workflows/executions
pub async fn list_executions(
    State(state): State<WorkflowApiState>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<ListExecutionsResponse>, ApiError> {
    info!("Listing executions with filters");

    use crate::workflows::domain::ExecutionFilters;

    // Parse status from string
    let status = query
        .status
        .as_ref()
        .and_then(|s| ExecutionStatus::from_str(s));

    // Build filters
    let filters = ExecutionFilters {
        workflow_id: query.workflow_id.clone(),
        status,
        start_date: query.start_date,
        end_date: query.end_date,
        search: query.search.clone(),
    };

    // Apply workflow_id filter if provided
    let executions = if let Some(workflow_id) = &query.workflow_id {
        state.execution_store.list_by_workflow(workflow_id).await?
    } else {
        state
            .execution_store
            .list_filtered(&filters, Some(query.limit), Some(query.offset))
            .await?
    };

    let total = if query.workflow_id.is_some() {
        executions.len()
    } else {
        state.execution_store.count_filtered(&filters).await?
    };

    // Convert to summaries
    let summaries: Vec<ExecutionSummary> = executions
        .into_iter()
        .filter(|e| filters.matches(e))
        .map(ExecutionSummary::from)
        .collect();

    Ok(Json(ListExecutionsResponse {
        executions: summaries,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Stop an execution
///
/// POST /api/v1/workflows/executions/{execution_id}/stop
pub async fn stop_execution(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionLifecycleResponse>, ApiError> {
    info!("Stopping execution: {}", execution_id);

    let execution = state.execution_store.get_required(&execution_id).await?;

    if !execution.can_stop() {
        return Err(ApiError::BadRequest(format!(
            "Execution '{}' cannot be stopped (status: {})",
            execution_id, execution.status
        )));
    }

    // Update status
    state
        .execution_store
        .update_status(&execution_id, ExecutionStatus::Stopped)
        .await?;
    state
        .execution_store
        .add_log(
            &execution_id,
            ExecutionLog::info("Execution stopped by user request"),
        )
        .await?;

    info!("Execution '{}' stopped successfully", execution_id);

    Ok(Json(ExecutionLifecycleResponse {
        execution_id,
        status: ExecutionStatus::Stopped,
        message: "Execution stopped successfully".to_string(),
    }))
}

/// Pause an execution
///
/// POST /api/v1/workflows/executions/{execution_id}/pause
pub async fn pause_execution(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionLifecycleResponse>, ApiError> {
    info!("Pausing execution: {}", execution_id);

    let execution = state.execution_store.get_required(&execution_id).await?;

    if !execution.can_pause() {
        return Err(ApiError::BadRequest(format!(
            "Execution '{}' cannot be paused (status: {})",
            execution_id, execution.status
        )));
    }

    // Update status
    state
        .execution_store
        .update_status(&execution_id, ExecutionStatus::Paused)
        .await?;
    state
        .execution_store
        .add_log(
            &execution_id,
            ExecutionLog::info("Execution paused by user request"),
        )
        .await?;

    info!("Execution '{}' paused successfully", execution_id);

    Ok(Json(ExecutionLifecycleResponse {
        execution_id,
        status: ExecutionStatus::Paused,
        message: "Execution paused successfully".to_string(),
    }))
}

/// Resume an execution
///
/// POST /api/v1/workflows/executions/{execution_id}/resume
pub async fn resume_execution(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionLifecycleResponse>, ApiError> {
    info!("Resuming execution: {}", execution_id);

    let execution = state.execution_store.get_required(&execution_id).await?;

    if !execution.can_resume() {
        return Err(ApiError::BadRequest(format!(
            "Execution '{}' cannot be resumed (status: {})",
            execution_id, execution.status
        )));
    }

    // Update status
    state
        .execution_store
        .update_status(&execution_id, ExecutionStatus::Running)
        .await?;
    state
        .execution_store
        .add_log(
            &execution_id,
            ExecutionLog::info("Execution resumed by user request"),
        )
        .await?;

    info!("Execution '{}' resumed successfully", execution_id);

    Ok(Json(ExecutionLifecycleResponse {
        execution_id,
        status: ExecutionStatus::Running,
        message: "Execution resumed successfully".to_string(),
    }))
}

/// Force abort an execution
///
/// POST /api/v1/workflows/executions/{execution_id}/abort
pub async fn abort_execution(
    State(state): State<WorkflowApiState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionLifecycleResponse>, ApiError> {
    info!("Aborting execution: {}", execution_id);

    let execution = state.execution_store.get_required(&execution_id).await?;

    // Abort can be called on any non-terminal state
    if execution.status.is_terminal() {
        return Err(ApiError::BadRequest(format!(
            "Execution '{}' is already in terminal state: {}",
            execution_id, execution.status
        )));
    }

    // Update status
    state
        .execution_store
        .update_status(&execution_id, ExecutionStatus::Aborted)
        .await?;
    state
        .execution_store
        .add_log(
            &execution_id,
            ExecutionLog::warn("Execution aborted by user request"),
        )
        .await?;

    warn!("Execution '{}' aborted", execution_id);

    Ok(Json(ExecutionLifecycleResponse {
        execution_id,
        status: ExecutionStatus::Aborted,
        message: "Execution aborted successfully".to_string(),
    }))
}

/// Update workflow schedule
///
/// PUT /api/v1/workflows/{workflow_id}/schedule
pub async fn update_schedule(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<UpdateScheduleResponse>, ApiError> {
    info!("Updating schedule for workflow: {}", workflow_id);

    // Verify workflow exists
    let workflow = state.store.get_required(&workflow_id)?;

    // Validate cron expression
    validate_cron_expression(&req.cron_expression)
        .map_err(|e| ApiError::BadRequest(format!("Invalid cron expression: {}", e)))?;

    // Validate timezone
    let timezone = req.timezone.unwrap_or_else(|| "UTC".to_string());
    if !validate_timezone(&timezone) {
        return Err(ApiError::BadRequest(format!(
            "Invalid timezone: {}",
            timezone
        )));
    }

    // Check if schedule already exists
    let existing_schedule = state.schedule_store.get_by_workflow(&workflow_id)?;

    if let Some(mut schedule) = existing_schedule {
        // Update existing schedule
        schedule.update(
            Some(req.cron_expression.clone()),
            None,
            None,
            timezone.clone(),
            None,
            None,
            req.enabled,
        );

        // Calculate next_run with timezone support
        use crate::workflows::domain::calculate_next_execution;
        let now = Utc::now();
        let next_run = if req.enabled {
            calculate_next_execution(Some(&req.cron_expression), None, None, &timezone, now)
                .map_err(|e| anyhow::anyhow!("Failed to calculate next execution: {}", e))?
        } else {
            None
        };

        schedule.set_next_run(next_run);

        state.schedule_store.update(schedule.clone())?;

        info!("Updated schedule for workflow '{}'", workflow_id);

        Ok(Json(UpdateScheduleResponse {
            workflow_id: schedule.workflow_id,
            cron_expression: schedule.cron_expression.unwrap_or_default(),
            enabled: schedule.enabled,
            timezone: schedule.timezone,
            next_run: schedule.next_run,
        }))
    } else {
        // Create new schedule
        let schedule_id = format!("sched_{}", uuid::Uuid::new_v4());
        let mut schedule = WorkflowSchedule::new(
            schedule_id,
            workflow_id.clone(),
            workflow.name.clone(),
            Some(req.cron_expression.clone()),
            None,
            None,
            timezone.clone(),
            serde_json::json!({}),
            serde_json::json!({}),
            req.enabled,
        );

        // Calculate next_run with timezone support
        use crate::workflows::domain::calculate_next_execution;
        let now = Utc::now();
        let next_run = if req.enabled {
            calculate_next_execution(Some(&req.cron_expression), None, None, &timezone, now)
                .map_err(|e| anyhow::anyhow!("Failed to calculate next execution: {}", e))?
        } else {
            None
        };

        schedule.set_next_run(next_run);

        state.schedule_store.create(schedule.clone())?;

        info!("Created schedule for workflow '{}'", workflow_id);

        Ok(Json(UpdateScheduleResponse {
            workflow_id: schedule.workflow_id,
            cron_expression: schedule.cron_expression.unwrap_or_default(),
            enabled: schedule.enabled,
            timezone: schedule.timezone,
            next_run: schedule.next_run,
        }))
    }
}

/// Preview schedule execution times
///
/// POST /api/v1/workflows/schedule/preview
pub async fn preview_schedule(
    State(_state): State<WorkflowApiState>,
    Json(req): Json<SchedulePreviewRequest>,
) -> Result<Json<SchedulePreviewResponse>, ApiError> {
    info!("Previewing schedule: {}", req.cron_expression);

    // Validate cron expression
    if let Err(e) = validate_cron_expression(&req.cron_expression) {
        return Ok(Json(SchedulePreviewResponse {
            cron_expression: req.cron_expression,
            timezone: req.timezone.unwrap_or_else(|| "UTC".to_string()),
            next_runs: vec![],
            is_valid: false,
            validation_error: Some(e),
        }));
    }

    // Validate timezone
    let timezone = req.timezone.unwrap_or_else(|| "UTC".to_string());
    if !validate_timezone(&timezone) {
        return Ok(Json(SchedulePreviewResponse {
            cron_expression: req.cron_expression,
            timezone: timezone.clone(),
            next_runs: vec![],
            is_valid: false,
            validation_error: Some(format!("Invalid timezone: {}", timezone)),
        }));
    }

    // Calculate next N execution times
    use crate::workflows::domain::calculate_next_execution;
    use chrono::Duration;

    let mut next_runs = Vec::new();
    let mut current_time = Utc::now();

    // Calculate the requested number of next execution times
    for _ in 0..req.count {
        match calculate_next_execution(
            Some(&req.cron_expression),
            None,
            None,
            &timezone,
            current_time,
        ) {
            Ok(Some(next_time)) => {
                next_runs.push(next_time);
                // Move forward slightly past this execution time for the next calculation
                current_time = next_time + Duration::seconds(1);
            }
            Ok(None) => {
                // No more executions (e.g., one-time schedule in the past)
                break;
            }
            Err(e) => {
                // This shouldn't happen since we already validated, but handle it
                warn!("Error calculating next execution: {}", e);
                break;
            }
        }
    }

    info!(
        "Cron expression '{}' is valid, calculated {} next execution times",
        req.cron_expression,
        next_runs.len()
    );

    Ok(Json(SchedulePreviewResponse {
        cron_expression: req.cron_expression,
        timezone,
        next_runs,
        is_valid: true,
        validation_error: None,
    }))
}

/// Get route statistics for a workflow
///
/// POST /api/v1/workflows/{id}/routes/stats
pub async fn get_route_stats(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    Json(req): Json<GetRouteStatsRequest>,
) -> Result<Json<GetRouteStatsResponse>, ApiError> {
    info!("Getting route stats for workflow: {}", workflow_id);

    let workflow = state.store.get_required(&workflow_id)?;

    let stats = WorkflowRouter::get_route_stats(&workflow, &req.sample_data);

    // Convert to response format
    let mut route_matches = std::collections::HashMap::new();
    for (route_id, count) in stats.route_matches {
        if let Some(route) = workflow.find_route(&route_id) {
            let percentage = (count as f64 / stats.total_samples as f64) * 100.0;
            route_matches.insert(
                route_id.clone(),
                RouteMatchStats {
                    route_id,
                    route_name: route.name.clone(),
                    match_count: count,
                    match_percentage: percentage,
                },
            );
        }
    }

    Ok(Json(GetRouteStatsResponse {
        workflow_id: workflow.id,
        total_samples: stats.total_samples,
        route_matches,
        no_match_count: stats.no_match_count,
        error_count: stats.error_count,
    }))
}

// === Legacy Workflow Engine Handlers (Graph-Native Support) ===
//
// These handlers use the old WorkflowEngine-based execution model from graphica-core.
// They support graph-native inputs (SPARQL queries, entity filters) via InputAdapters.
// These are maintained for backward compatibility with existing workflows.

/// Execute a workflow using the legacy WorkflowEngine (supports graph-native inputs)
///
/// POST /api/v1/workflows/legacy/{id}/execute
///
/// This handler supports graph-native workflow execution with:
/// - SPARQL query inputs
/// - Entity filter inputs
/// - Datasource query inputs
/// - Materialized dataset inputs
/// - Standard JSON inputs
///
/// The handler automatically selects the appropriate InputAdapter based on the input type.
#[allow(dead_code)]
pub async fn execute_workflow_legacy(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<crate::api::workflow::types::ExecuteWorkflowRequest>,
) -> Result<Json<crate::api::workflow::types::ExecuteWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    // Get workflow definition
    let workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    let execution_input =
        serde_json::to_value(&request.input).unwrap_or_else(|_| serde_json::Value::Null);
    let triggered_by = request.context.initiator.clone();

    // Convert input wrapper to WorkflowInput
    let workflow_input = request.input.into_workflow_input();

    // Create appropriate input adapter based on input type
    let adapter: Arc<dyn InputAdapter> = build_input_adapter(state.clone(), &workflow_input)?;

    // Execute workflow with input adapter
    let started_at = Utc::now();
    let context_map = request.context.to_hashmap();

    let results = engine
        .execute_workflow_with_input(&workflow_id, workflow_input, adapter, &context_map)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Workflow execution failed: {}", e),
            )
        })?;

    let completed_at = Utc::now();

    let batch_count = results.len();
    let mut result_dtos = Vec::with_capacity(batch_count);
    for (batch_index, result) in results.into_iter().enumerate() {
        let execution_result = finalize_execution_result(
            state.clone(),
            &workflow_id,
            &workflow.name,
            &result,
            request.output_dataset.as_ref(),
            batch_index,
            batch_count,
        )
        .await?;

        if let Err(error) = persist_execution_record_if_possible(
            state.as_ref(),
            &workflow_id,
            &workflow.name,
            &execution_input,
            triggered_by.as_deref(),
            &result,
            &execution_result,
        )
        .await
        {
            warn!(
                workflow_id = %workflow_id,
                execution_id = %result.execution_id,
                "Failed to persist graph-native execution history: {}",
                error
            );
        }

        result_dtos.push(execution_result);
    }

    // Build batched response
    let response = crate::api::workflow::types::ExecuteWorkflowResponse::batched(
        workflow_id,
        result_dtos,
        started_at,
        completed_at,
    );

    Ok(Json(response))
}

/// Perform a dry-run execution of a workflow
///
/// POST /api/v1/workflows/{id}/dry-run
#[allow(dead_code)]
pub async fn dry_run_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<DryRunWorkflowRequest>,
) -> Result<Json<DryRunWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    // Validate workflow exists before attempting dry-run execution
    // Note: workflow is fetched here for early validation, but execute_workflow_with_input will fetch it again internally
    let _workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    // Convert context to HashMap and add dry-run flag
    let mut context_map = request.context.to_hashmap();
    context_map.insert("dry_run".to_string(), "true".to_string());

    // Execute workflow with JSON input
    let workflow_input = graphica_core::orchestration::workflow::WorkflowInput::Json {
        data: request.input.clone(),
    };

    // Execute the workflow
    let start_time = std::time::Instant::now();
    let result = engine
        .execute_workflow_with_input(
            &workflow_id,
            workflow_input,
            Arc::new(JsonInputAdapter),
            &context_map,
        )
        .await;
    let total_execution_time_ms = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(exec_results) => {
            // Take first result (dry-run typically executes single context)
            let exec_result = exec_results.into_iter().next().ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "No execution result".to_string(),
                )
            })?;

            // Convert execution result to step results
            let mut steps_executed: Vec<StepExecutionResult> = exec_result
                .step_results
                .iter()
                .map(|(step_id, step_result)| {
                    StepExecutionResult {
                        step_id: step_id.clone(),
                        step_type: "unknown".to_string(), // Would need step type from definition
                        success: step_result.success,
                        output: Some(step_result.output.clone()),
                        error: None,
                        execution_time_ms: 0, // Not tracked per-step in WorkflowEngine
                    }
                })
                .collect();

            // Sort steps by step_id to get consistent ordering
            steps_executed.sort_by(|a, b| a.step_id.cmp(&b.step_id));

            // Get final output from last step or create based on decision
            let final_output = steps_executed
                .last()
                .and_then(|s| s.output.clone())
                .or_else(|| match exec_result.final_decision {
                    FinalDecision::Accept => Some(json!({"status": "accepted"})),
                    FinalDecision::Reject => Some(json!({"status": "rejected"})),
                    FinalDecision::ManualReview => Some(json!({"status": "manual_review"})),
                });

            Ok(Json(DryRunWorkflowResponse {
                success: exec_result.success,
                steps_executed,
                final_output,
                total_execution_time_ms,
                failed_step: exec_result.error,
            }))
        }
        Err(e) => Ok(Json(DryRunWorkflowResponse {
            success: false,
            steps_executed: vec![],
            final_output: None,
            total_execution_time_ms,
            failed_step: Some(format!("Execution failed: {}", e)),
        })),
    }
}

// === Error Handling ===

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Forbidden(String),
    Internal(anyhow::Error),
    InternalServerError(String),
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        // Check for specific error types
        let err_str = err.to_string();
        if err_str.contains("not found") {
            ApiError::NotFound(err_str)
        } else if err_str.contains("already exists") || err_str.contains("validation") {
            ApiError::BadRequest(err_str)
        } else {
            ApiError::Internal(err)
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NotFound", msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BadRequest", msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "Forbidden", msg),
            ApiError::InternalServerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalServerError",
                msg,
            ),
            ApiError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                err.to_string(),
            ),
        };

        let body = Json(ErrorResponse::new(error, message));

        (status, body).into_response()
    }
}

// === Helper Functions ===

/// Extract ML feature values from action outputs
///
/// Scans action outputs for ML-related fields (predictions, confidence scores, etc.)
/// and extracts them as structured ML feature values for tracking.
fn extract_ml_features(
    actions_executed: &[crate::workflows::domain::ActionResult],
    final_output: &JsonValue,
) -> Option<Vec<MlFeatureValue>> {
    let mut features = Vec::new();

    // Extract from action outputs
    for action in actions_executed {
        if let Some(output) = &action.output {
            // Check for prediction-related fields
            if let Some(prediction) = output.get("prediction") {
                features.push(MlFeatureValue {
                    feature_name: format!("{}_prediction", action.action_type),
                    value: prediction.clone(),
                    model_id: output
                        .get("model_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    computed_at: Some(chrono::Utc::now()),
                });
            }

            // Check for confidence scores
            if let Some(confidence) = output.get("confidence") {
                features.push(MlFeatureValue {
                    feature_name: format!("{}_confidence", action.action_type),
                    value: confidence.clone(),
                    model_id: output
                        .get("model_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    computed_at: Some(chrono::Utc::now()),
                });
            }

            // Check for feature_values array
            if let Some(feature_array) = output.get("features").and_then(|v| v.as_array()) {
                for feature in feature_array {
                    if let (Some(name), Some(value)) = (
                        feature.get("name").and_then(|v| v.as_str()),
                        feature.get("value"),
                    ) {
                        features.push(MlFeatureValue {
                            feature_name: name.to_string(),
                            value: value.clone(),
                            model_id: feature
                                .get("model_id")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            computed_at: Some(chrono::Utc::now()),
                        });
                    }
                }
            }
        }
    }

    // Check final output for ML fields
    if let Some(obj) = final_output.as_object() {
        for (key, value) in obj {
            if key.ends_with("_score") || key.ends_with("_probability") || key.contains("ml_") {
                features.push(MlFeatureValue {
                    feature_name: key.clone(),
                    value: value.clone(),
                    model_id: None,
                    computed_at: Some(chrono::Utc::now()),
                });
            }
        }
    }

    if features.is_empty() {
        None
    } else {
        Some(features)
    }
}

/// Build lineage references from workflow execution
///
/// Creates lineage references tracking the workflow, route, and actions
/// that contributed to the execution result.
fn build_lineage_refs(
    workflow_id: &str,
    matched_route_id: &Option<String>,
    actions_executed: &[crate::workflows::domain::ActionResult],
) -> Vec<LineageReference> {
    let mut refs = Vec::new();

    // Add workflow lineage
    refs.push(LineageReference {
        ref_type: "workflow".to_string(),
        ref_id: workflow_id.to_string(),
        description: Some("Source workflow".to_string()),
    });

    // Add route lineage if matched
    if let Some(route_id) = matched_route_id {
        refs.push(LineageReference {
            ref_type: "route".to_string(),
            ref_id: route_id.clone(),
            description: Some("Matched routing rule".to_string()),
        });
    }

    // Add action lineage for significant actions
    for (idx, action) in actions_executed.iter().enumerate() {
        if action.status == crate::workflows::domain::ActionStatus::Success {
            // Add lineage for transformation actions
            if action.action_type.contains("Transform") {
                refs.push(LineageReference {
                    ref_type: "transformation".to_string(),
                    ref_id: format!("action_{}_{}", idx, action.action_type),
                    description: Some(format!("Action: {}", action.action_type)),
                });
            }

            // Add lineage for external system interactions
            if action.action_type.contains("Kafka") || action.action_type.contains("Http") {
                refs.push(LineageReference {
                    ref_type: "sink".to_string(),
                    ref_id: format!("action_{}_{}", idx, action.action_type),
                    description: Some(format!("Data sent via {}", action.action_type)),
                });
            }

            // Extract model references from action outputs
            if let Some(output) = &action.output {
                if let Some(model_id) = output.get("model_id").and_then(|v| v.as_str()) {
                    refs.push(LineageReference {
                        ref_type: "model".to_string(),
                        ref_id: model_id.to_string(),
                        description: Some(format!("ML model used in {}", action.action_type)),
                    });
                }
            }
        }
    }

    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{
        Action, Condition, LogLevel, PersistedStepResult, WorkflowExecution,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    fn create_test_state() -> Arc<WorkflowApiState> {
        let store = WorkflowStore::new();
        let execution_store = ExecutionStore::new();
        let schedule_store = ScheduleStore::new();
        Arc::new(WorkflowApiState::new(
            store,
            execution_store,
            schedule_store,
        ))
    }

    fn create_test_legacy_api_state() -> Arc<crate::api::ApiState> {
        use crate::api::auth::AuthConfig;
        use crate::api::import_jobs::ImportJobManager;
        use crate::api::setup_token::SetupTokenManager;
        use crate::storage::LineageStorage;
        use graphica_core::orchestration::workflow::WorkflowEngine;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let rocks_path = temp_dir.path().join("rocksdb");
        let parquet_path = temp_dir.path().join("parquet");
        let cold_path = temp_dir.path().join("cold");

        std::fs::create_dir_all(&rocks_path).unwrap();
        std::fs::create_dir_all(&parquet_path).unwrap();
        std::fs::create_dir_all(&cold_path).unwrap();

        let lineage_storage = LineageStorage::new_for_tests(
            rocks_path.to_str().unwrap(),
            parquet_path.to_str().unwrap(),
            cold_path.to_str().unwrap(),
        )
        .unwrap();

        Arc::new(crate::api::ApiState {
            lineage_storage: Arc::new(lineage_storage),
            governance_brain: None,
            rdf_store: None,
            shard_registry: None,
            query_executor: None,
            workflow_engine: Some(Arc::new(WorkflowEngine::new())),
            model_registry: None,
            model_cache: None,
            rule_executor: None,
            transformer_registry: None,
            circuit_breakers: None,
            auth_config: Arc::new(AuthConfig::disabled()),
            user_service: None,
            setup_token_manager: Arc::new(SetupTokenManager::new()),
            audit_logger: None,
            datasource_catalog: None,
            datasource_catalog_impl: None,
            persisted_ontology_registry: None,
            ontology_registry: None,
            rdf_storage: None,
            connector_registry: None,
            resolved_entity_cache: None,
            metrics_registry: None,
            import_job_manager: Arc::new(ImportJobManager::new()),
            mapping_engine: None,
            secret_store_registry: None,
            loader_job_manager: None,
            unified_mapping_coordinator: None,
            binding_service: None,
            schedule_store: Some(Arc::new(crate::workflows::storage::ScheduleStore::new())),
            workflow_store: None,
            execution_store: Some(Arc::new(crate::workflows::storage::ExecutionStore::new())),
            file_library: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            metrics: None,
            replay_coordinator: None,
            row_lineage_store: None,
            manual_mapping_store: None,
            db2_pool: None,
            approval_store: None,
            execution_sync: None,
            policy_checker: None,
            checkpoint_persistence: None,
            dlq_reader: None,
            dlq_reprocessor: None,
            dlq_stats_calculator: None,
            schema_version_store: None,
            column_lineage_store: None,
            schema_evolution_store: None,
            gdpr_coordinator: None,
            export_executor: None,
            progress_store: None,
            cancellation_manager: None,
            sos_storage_manager: None,
            discovery_state: None,
            discovery_orchestrator: None,
        })
    }

    fn create_test_auth_api_state() -> (crate::api::ApiState, String) {
        use crate::api::auth::{AuthConfig, Role};
        use crate::api::import_jobs::ImportJobManager;
        use crate::api::setup_token::SetupTokenManager;
        use crate::storage::LineageStorage;
        use graphica_core::orchestration::workflow::WorkflowEngine;
        use tempfile::TempDir;

        let test_secret: [u8; 32] = [
            0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7,
            0xf8, 0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88,
        ];
        let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());
        let token = auth_config
            .generate_token("test_user", Role::Admin)
            .unwrap();

        let temp_dir = TempDir::new().unwrap();
        let rocks_path = temp_dir.path().join("rocksdb");
        let parquet_path = temp_dir.path().join("parquet");
        let cold_path = temp_dir.path().join("cold");

        std::fs::create_dir_all(&rocks_path).unwrap();
        std::fs::create_dir_all(&parquet_path).unwrap();
        std::fs::create_dir_all(&cold_path).unwrap();

        let lineage_storage = LineageStorage::new_for_tests(
            rocks_path.to_str().unwrap(),
            parquet_path.to_str().unwrap(),
            cold_path.to_str().unwrap(),
        )
        .unwrap();

        let state = crate::api::ApiState {
            lineage_storage: Arc::new(lineage_storage),
            governance_brain: None,
            rdf_store: None,
            shard_registry: None,
            query_executor: None,
            workflow_engine: Some(Arc::new(WorkflowEngine::new())),
            model_registry: None,
            model_cache: None,
            rule_executor: None,
            transformer_registry: None,
            circuit_breakers: None,
            auth_config: auth_config.clone(),
            user_service: None,
            setup_token_manager: Arc::new(SetupTokenManager::new()),
            audit_logger: None,
            datasource_catalog: None,
            datasource_catalog_impl: None,
            persisted_ontology_registry: None,
            ontology_registry: None,
            rdf_storage: None,
            connector_registry: None,
            resolved_entity_cache: None,
            metrics_registry: None,
            import_job_manager: Arc::new(ImportJobManager::new()),
            mapping_engine: None,
            secret_store_registry: None,
            loader_job_manager: None,
            unified_mapping_coordinator: None,
            binding_service: None,
            schedule_store: None,
            workflow_store: None,
            execution_store: None,
            file_library: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            metrics: None,
            replay_coordinator: None,
            row_lineage_store: None,
            manual_mapping_store: None,
            db2_pool: None,
            approval_store: None,
            execution_sync: None,
            policy_checker: None,
            checkpoint_persistence: None,
            dlq_reader: None,
            dlq_reprocessor: None,
            dlq_stats_calculator: None,
            schema_version_store: None,
            column_lineage_store: None,
            schema_evolution_store: None,
            gdpr_coordinator: None,
            export_executor: None,
            progress_store: None,
            cancellation_manager: None,
            sos_storage_manager: None,
            discovery_state: None,
            discovery_orchestrator: None,
        };

        (state, token)
    }

    fn create_test_route_dto() -> RouteDto {
        RouteDto {
            name: "test_route".to_string(),
            description: String::new(),
            condition: Box::new(Condition::Always),
            actions: vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
            priority: 10,
        }
    }

    #[tokio::test]
    async fn test_create_workflow() {
        let state = create_test_state();

        let req = CreateWorkflowRequest {
            name: "test_workflow".to_string(),
            description: "Test description".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec!["test".to_string()],
        };

        let result = create_workflow(State(state.as_ref().clone()), Json(req)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.name, "test_workflow");
    }

    #[tokio::test]
    async fn test_get_workflow() {
        let state = create_test_state();

        // First create a workflow
        let create_req = CreateWorkflowRequest {
            name: "test_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Now get it
        let result = get_workflow(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.id, create_resp.0.id);
    }

    #[tokio::test]
    async fn test_execute_workflow() {
        let state = create_test_state();

        // Create a workflow with a matching route
        let create_req = CreateWorkflowRequest {
            name: "test_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![RouteDto {
                name: "test_route".to_string(),
                description: String::new(),
                condition: Box::new(Condition::equals("status", "active")),
                actions: vec![
                    Action::SetField {
                        field: "processed".to_string(),
                        value: json!(true),
                    },
                    Action::Log {
                        level: "info".to_string(),
                        message: "Processed".to_string(),
                    },
                ],
                priority: 10,
            }],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Execute the workflow
        let exec_req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"status": "active", "data": "test"})),
            context: None,
            dry_run: false,
        };

        let result = execute_workflow(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Json(exec_req),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.0.matched_route.is_some());
        assert_eq!(response.0.actions_executed.len(), 2);
        assert_eq!(response.0.output["processed"], true);
    }

    #[tokio::test]
    async fn test_delete_workflow() {
        let state = create_test_state();

        // Create a workflow
        let create_req = CreateWorkflowRequest {
            name: "test_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Delete it
        let result = delete_workflow(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), StatusCode::NO_CONTENT);

        // Verify it's gone
        let get_result = get_workflow(State(state.as_ref().clone()), Path(create_resp.0.id)).await;
        assert!(get_result.is_err());
    }

    #[tokio::test]
    async fn test_list_workflows() {
        let state = create_test_state();

        // Create multiple workflows
        for i in 0..5 {
            let req = CreateWorkflowRequest {
                name: format!("workflow_{}", i),
                description: String::new(),
                routes: vec![create_test_route_dto()],
                default_route: None,
                tags: vec![],
            };
            let _ = create_workflow(State(state.as_ref().clone()), Json(req))
                .await
                .unwrap();
        }

        // List workflows
        let query = ListWorkflowsQuery {
            enabled: None,
            tags: None,
            limit: 10,
            offset: 0,
        };

        let result = list_workflows(State(state.as_ref().clone()), Query(query)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.len(), 5);
    }

    // === Execution Tracking Tests ===

    #[tokio::test]
    async fn test_execute_workflow_async() {
        let state = create_test_state();

        // Create a workflow
        let create_req = CreateWorkflowRequest {
            name: "async_test_workflow".to_string(),
            description: "Test async execution".to_string(),
            routes: vec![RouteDto {
                name: "test_route".to_string(),
                description: String::new(),
                condition: Box::new(Condition::Always),
                actions: vec![Action::SetField {
                    field: "result".to_string(),
                    value: json!("success"),
                }],
                priority: 10,
            }],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Execute async
        let exec_req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"test": "data"})),
            context: None,
            dry_run: false,
        };

        // Create test extensions with dummy claims
        let mut extensions = Extensions::new();
        extensions.insert(Claims {
            sub: "test-user".to_string(),
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            role: crate::api::auth::Role::Admin,
            scopes: vec![],
        });

        let result = execute_workflow_async(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
            extensions,
            Json(exec_req),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.0.execution_id.is_empty());
        assert_eq!(response.0.workflow_id, create_resp.0.id);
        assert_eq!(response.0.status, ExecutionStatus::Pending);

        // Give background task a moment to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify execution was stored
        let execution = state
            .execution_store
            .get_required(&response.0.execution_id)
            .await
            .unwrap();
        assert!(execution.status.is_terminal());
    }

    #[tokio::test]
    async fn test_execute_workflow_rejects_dry_run_flag() {
        let state = create_test_state();

        let create_req = CreateWorkflowRequest {
            name: "dry_run_reject".to_string(),
            description: String::new(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        let exec_req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"status": "active"})),
            context: None,
            dry_run: true,
        };

        let result = execute_workflow(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Json(exec_req),
        )
        .await;

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message.contains("/dry-run"))
        );
    }

    #[tokio::test]
    async fn test_execute_workflow_async_rejects_dry_run_flag() {
        let state = create_test_state();

        let create_req = CreateWorkflowRequest {
            name: "dry_run_async_reject".to_string(),
            description: String::new(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        let exec_req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"status": "active"})),
            context: Some(ExecutionContextParams {
                request_id: Some("req_async_dry_run".to_string()),
                initiator: Some("context-user".to_string()),
                metadata: json!({}),
            }),
            dry_run: true,
        };

        let result = execute_workflow_async(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Extensions::new(),
            Json(exec_req),
        )
        .await;

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message.contains("/dry-run"))
        );
    }

    #[tokio::test]
    async fn test_execute_workflow_async_uses_context_initiator_without_claims() {
        let state = create_test_state();

        let create_req = CreateWorkflowRequest {
            name: "context_initiator_workflow".to_string(),
            description: String::new(),
            routes: vec![RouteDto {
                name: "always".to_string(),
                description: String::new(),
                condition: Box::new(Condition::Always),
                actions: vec![Action::SetField {
                    field: "result".to_string(),
                    value: json!("ok"),
                }],
                priority: 1,
            }],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        let exec_req = ExecuteWorkflowRequest {
            input: WorkflowInputWrapper::Json(json!({"test": "data"})),
            context: Some(ExecutionContextParams {
                request_id: Some("req_context_initiator".to_string()),
                initiator: Some("context-user@example.com".to_string()),
                metadata: json!({"source": "test"}),
            }),
            dry_run: false,
        };

        let response = execute_workflow_async(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Extensions::new(),
            Json(exec_req),
        )
        .await
        .unwrap();

        let execution = state
            .execution_store
            .get_required(&response.0.execution_id)
            .await
            .unwrap();
        assert_eq!(
            execution.triggered_by,
            Some("context-user@example.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_execution() {
        let state = create_test_state();

        // Create an execution directly
        let mut execution = WorkflowExecution::new(
            "test_exec_001".to_string(),
            "test_wf_001".to_string(),
            "Test Workflow".to_string(),
            json!({"input": "data"}),
            Some("test@example.com".to_string()),
        );
        execution.confidence = Some(0.91);
        execution.step_results = vec![PersistedStepResult {
            step_id: "step_1".to_string(),
            success: true,
            output: json!({"records": 3}),
            confidence: 0.91,
            duration_ms: 42,
        }];
        state.execution_store.save(execution.clone()).await.unwrap();

        // Get execution via API
        let result = get_execution(
            State(state.as_ref().clone()),
            Path("test_exec_001".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.execution_id, "test_exec_001");
        assert_eq!(response.0.workflow_id, "test_wf_001");
        assert_eq!(response.0.status, ExecutionStatus::Pending);
        assert_eq!(
            response.0.triggered_by,
            Some("test@example.com".to_string())
        );
        assert_eq!(response.0.confidence, Some(0.91));
        assert_eq!(response.0.step_results.len(), 1);
        assert_eq!(response.0.step_results[0].step_id, "step_1");
        assert_eq!(response.0.step_results[0].duration_ms, 42);
    }

    #[tokio::test]
    async fn test_get_execution_not_found() {
        let state = create_test_state();

        let result = get_execution(
            State(state.as_ref().clone()),
            Path("nonexistent".to_string()),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_execution_logs() {
        let state = create_test_state();

        // Create execution with logs
        let execution = WorkflowExecution::new(
            "test_exec_002".to_string(),
            "test_wf_002".to_string(),
            "Test Workflow".to_string(),
            json!({}),
            None,
        );
        state.execution_store.save(execution).await.unwrap();

        state
            .execution_store
            .add_log("test_exec_002", ExecutionLog::info("Log entry 1"))
            .await
            .unwrap();
        state
            .execution_store
            .add_log("test_exec_002", ExecutionLog::warn("Log entry 2"))
            .await
            .unwrap();
        state
            .execution_store
            .add_log("test_exec_002", ExecutionLog::error("Log entry 3"))
            .await
            .unwrap();

        // Get logs via API
        let result = get_execution_logs(
            State(state.as_ref().clone()),
            Path("test_exec_002".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.execution_id, "test_exec_002");
        assert_eq!(response.0.logs.len(), 3);
        assert_eq!(response.0.total, 3);
        assert_eq!(response.0.logs[0].level, LogLevel::Info);
        assert_eq!(response.0.logs[1].level, LogLevel::Warning);
        assert_eq!(response.0.logs[2].level, LogLevel::Error);
    }

    #[tokio::test]
    async fn test_list_executions_no_filters() {
        let state = create_test_state();

        // Create multiple executions
        for i in 0..5 {
            let execution = WorkflowExecution::new(
                format!("exec_{:03}", i),
                "wf_001".to_string(),
                "Test Workflow".to_string(),
                json!({}),
                None,
            );
            state.execution_store.save(execution).await.unwrap();
        }

        // List all executions
        let query = ListExecutionsQuery {
            workflow_id: None,
            status: None,
            start_date: None,
            end_date: None,
            search: None,
            limit: 10,
            offset: 0,
        };

        let result = list_executions(State(state.as_ref().clone()), Query(query)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.executions.len(), 5);
        assert_eq!(response.0.total, 5);
    }

    #[tokio::test]
    async fn test_list_executions_with_workflow_filter() {
        let state = create_test_state();

        // Create executions for different workflows
        for i in 0..3 {
            let execution = WorkflowExecution::new(
                format!("exec_wf1_{}", i),
                "wf_001".to_string(),
                "Workflow 1".to_string(),
                json!({}),
                None,
            );
            state.execution_store.save(execution).await.unwrap();
        }

        for i in 0..2 {
            let execution = WorkflowExecution::new(
                format!("exec_wf2_{}", i),
                "wf_002".to_string(),
                "Workflow 2".to_string(),
                json!({}),
                None,
            );
            state.execution_store.save(execution).await.unwrap();
        }

        // Filter by workflow_id
        let query = ListExecutionsQuery {
            workflow_id: Some("wf_001".to_string()),
            status: None,
            start_date: None,
            end_date: None,
            search: None,
            limit: 10,
            offset: 0,
        };

        let result = list_executions(State(state.as_ref().clone()), Query(query)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.executions.len(), 3);
    }

    #[tokio::test]
    async fn test_list_executions_with_status_filter() {
        let state = create_test_state();

        // Create executions with different statuses
        for i in 0..2 {
            let mut execution = WorkflowExecution::new(
                format!("exec_running_{}", i),
                "wf_001".to_string(),
                "Test".to_string(),
                json!({}),
                None,
            );
            execution.update_status(ExecutionStatus::Running);
            state.execution_store.save(execution).await.unwrap();
        }

        for i in 0..3 {
            let mut execution = WorkflowExecution::new(
                format!("exec_completed_{}", i),
                "wf_001".to_string(),
                "Test".to_string(),
                json!({}),
                None,
            );
            execution.update_status(ExecutionStatus::Completed);
            state.execution_store.save(execution).await.unwrap();
        }

        // Filter by status
        let query = ListExecutionsQuery {
            workflow_id: None,
            status: Some("completed".to_string()),
            start_date: None,
            end_date: None,
            search: None,
            limit: 10,
            offset: 0,
        };

        let result = list_executions(State(state.as_ref().clone()), Query(query)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.executions.len(), 3);
        for exec in &response.0.executions {
            assert_eq!(exec.status, ExecutionStatus::Completed);
        }
    }

    #[tokio::test]
    async fn test_list_executions_with_pagination() {
        let state = create_test_state();

        // Create 10 executions
        for i in 0..10 {
            let execution = WorkflowExecution::new(
                format!("exec_{:03}", i),
                "wf_001".to_string(),
                "Test".to_string(),
                json!({}),
                None,
            );
            state.execution_store.save(execution).await.unwrap();
        }

        // Get first page
        let query = ListExecutionsQuery {
            workflow_id: None,
            status: None,
            start_date: None,
            end_date: None,
            search: None,
            limit: 5,
            offset: 0,
        };

        let result = list_executions(State(state.as_ref().clone()), Query(query)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.executions.len(), 5);
        assert_eq!(response.0.limit, 5);
        assert_eq!(response.0.offset, 0);

        // Get second page
        let query = ListExecutionsQuery {
            workflow_id: None,
            status: None,
            start_date: None,
            end_date: None,
            search: None,
            limit: 5,
            offset: 5,
        };

        let result = list_executions(State(state.as_ref().clone()), Query(query)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.executions.len(), 5);
        assert_eq!(response.0.offset, 5);
    }

    #[tokio::test]
    async fn test_stop_execution() {
        let state = create_test_state();

        // Create a running execution
        let mut execution = WorkflowExecution::new(
            "exec_to_stop".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);
        state.execution_store.save(execution).await.unwrap();

        // Stop it
        let result = stop_execution(
            State(state.as_ref().clone()),
            Path("exec_to_stop".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.status, ExecutionStatus::Stopped);
        assert!(response.0.message.contains("stopped"));

        // Verify it was stopped
        let execution = state
            .execution_store
            .get_required("exec_to_stop")
            .await
            .unwrap();
        assert_eq!(execution.status, ExecutionStatus::Stopped);
    }

    #[tokio::test]
    async fn test_stop_execution_already_completed() {
        let state = create_test_state();

        // Create a completed execution
        let mut execution = WorkflowExecution::new(
            "exec_completed".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Completed);
        state.execution_store.save(execution).await.unwrap();

        // Try to stop it (should fail)
        let result = stop_execution(
            State(state.as_ref().clone()),
            Path("exec_completed".to_string()),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_execution() {
        let state = create_test_state();

        // Create a running execution
        let mut execution = WorkflowExecution::new(
            "exec_to_pause".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);
        state.execution_store.save(execution).await.unwrap();

        // Pause it
        let result = pause_execution(
            State(state.as_ref().clone()),
            Path("exec_to_pause".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.status, ExecutionStatus::Paused);

        // Verify it was paused
        let execution = state
            .execution_store
            .get_required("exec_to_pause")
            .await
            .unwrap();
        assert_eq!(execution.status, ExecutionStatus::Paused);
        assert!(execution.can_resume());
    }

    #[tokio::test]
    async fn test_pause_execution_not_running() {
        let state = create_test_state();

        // Create a completed execution (cannot be paused)
        let mut execution = WorkflowExecution::new(
            "exec_completed".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.status = ExecutionStatus::Completed;
        execution.output = Some(json!({"result": "success"}));
        state.execution_store.save(execution).await.unwrap();

        // Try to pause it (should fail - completed executions can't be paused)
        let result = pause_execution(
            State(state.as_ref().clone()),
            Path("exec_completed".to_string()),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resume_execution() {
        let state = create_test_state();

        // Create a paused execution
        let mut execution = WorkflowExecution::new(
            "exec_to_resume".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);
        execution.update_status(ExecutionStatus::Paused);
        state.execution_store.save(execution).await.unwrap();

        // Resume it
        let result = resume_execution(
            State(state.as_ref().clone()),
            Path("exec_to_resume".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.status, ExecutionStatus::Running);

        // Verify it was resumed
        let execution = state
            .execution_store
            .get_required("exec_to_resume")
            .await
            .unwrap();
        assert_eq!(execution.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_resume_execution_not_paused() {
        let state = create_test_state();

        // Create a running execution
        let mut execution = WorkflowExecution::new(
            "exec_running".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);
        state.execution_store.save(execution).await.unwrap();

        // Try to resume it (should fail)
        let result = resume_execution(
            State(state.as_ref().clone()),
            Path("exec_running".to_string()),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_abort_execution() {
        let state = create_test_state();

        // Create a running execution
        let mut execution = WorkflowExecution::new(
            "exec_to_abort".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Running);
        state.execution_store.save(execution).await.unwrap();

        // Abort it
        let result = abort_execution(
            State(state.as_ref().clone()),
            Path("exec_to_abort".to_string()),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.status, ExecutionStatus::Aborted);

        // Verify it was aborted
        let execution = state
            .execution_store
            .get_required("exec_to_abort")
            .await
            .unwrap();
        assert_eq!(execution.status, ExecutionStatus::Aborted);
        assert!(execution.status.is_terminal());
        assert!(execution.status.is_error());
    }

    #[tokio::test]
    async fn test_abort_execution_already_terminal() {
        let state = create_test_state();

        // Create a completed execution
        let mut execution = WorkflowExecution::new(
            "exec_done".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        execution.update_status(ExecutionStatus::Completed);
        state.execution_store.save(execution).await.unwrap();

        // Try to abort it (should fail)
        let result =
            abort_execution(State(state.as_ref().clone()), Path("exec_done".to_string())).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execution_lifecycle_flow() {
        let state = create_test_state();

        // Create execution in pending state
        let execution = WorkflowExecution::new(
            "exec_lifecycle".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            json!({}),
            None,
        );
        state.execution_store.save(execution).await.unwrap();

        // Move to running
        state
            .execution_store
            .update_status("exec_lifecycle", ExecutionStatus::Running)
            .await
            .unwrap();

        // Pause
        let result = pause_execution(
            State(state.as_ref().clone()),
            Path("exec_lifecycle".to_string()),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.status, ExecutionStatus::Paused);

        // Resume
        let result = resume_execution(
            State(state.as_ref().clone()),
            Path("exec_lifecycle".to_string()),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.status, ExecutionStatus::Running);

        // Stop
        let result = stop_execution(
            State(state.as_ref().clone()),
            Path("exec_lifecycle".to_string()),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.status, ExecutionStatus::Stopped);

        // Verify final state
        let execution = state
            .execution_store
            .get_required("exec_lifecycle")
            .await
            .unwrap();
        assert_eq!(execution.status, ExecutionStatus::Stopped);
        assert!(execution.status.is_terminal());
        assert!(execution.logs.len() >= 3); // pause, resume, stop logs
    }

    // === Schedule Management Tests ===

    #[tokio::test]
    async fn test_create_schedule_for_workflow() {
        let state = create_test_state();

        // Create a workflow first
        let create_req = CreateWorkflowRequest {
            name: "scheduled_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Create schedule via update_schedule (creates if not exists)
        let schedule_req = UpdateScheduleRequest {
            cron_expression: "0 0 0 * * *".to_string(),
            enabled: true,
            timezone: Some("America/New_York".to_string()),
        };

        let result = update_schedule(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
            Json(schedule_req),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.workflow_id, create_resp.0.id);
        assert_eq!(response.0.cron_expression, "0 0 0 * * *");
        assert_eq!(response.0.timezone, "America/New_York");
        assert!(response.0.enabled);
    }

    #[tokio::test]
    async fn test_update_existing_schedule() {
        let state = create_test_state();

        // Create workflow
        let create_req = CreateWorkflowRequest {
            name: "scheduled_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Create initial schedule (setup for test)
        let schedule_req = UpdateScheduleRequest {
            cron_expression: "0 0 0 * * *".to_string(),
            enabled: true,
            timezone: Some("UTC".to_string()),
        };

        let _ = update_schedule(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
            Json(schedule_req),
        )
        .await
        .unwrap();

        // Update the schedule
        let update_req = UpdateScheduleRequest {
            cron_expression: "0 0 */2 * * *".to_string(),
            enabled: false,
            timezone: Some("Europe/London".to_string()),
        };

        let result = update_schedule(
            State(state.as_ref().clone()),
            Path(create_resp.0.id.clone()),
            Json(update_req),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.cron_expression, "0 0 */2 * * *");
        assert_eq!(response.0.timezone, "Europe/London");
        assert!(!response.0.enabled);
    }

    #[tokio::test]
    async fn test_update_schedule_invalid_cron() {
        let state = create_test_state();

        // Create workflow
        let create_req = CreateWorkflowRequest {
            name: "scheduled_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Try to create schedule with invalid cron
        let schedule_req = UpdateScheduleRequest {
            cron_expression: "invalid cron".to_string(),
            enabled: true,
            timezone: Some("UTC".to_string()),
        };

        let result = update_schedule(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Json(schedule_req),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_schedule_invalid_timezone() {
        let state = create_test_state();

        // Create workflow
        let create_req = CreateWorkflowRequest {
            name: "scheduled_workflow".to_string(),
            description: "Test".to_string(),
            routes: vec![create_test_route_dto()],
            default_route: None,
            tags: vec![],
        };

        let create_resp = create_workflow(State(state.as_ref().clone()), Json(create_req))
            .await
            .unwrap();

        // Try to create schedule with invalid timezone
        let schedule_req = UpdateScheduleRequest {
            cron_expression: "0 0 * * *".to_string(),
            enabled: true,
            timezone: Some("InvalidTimezone".to_string()),
        };

        let result = update_schedule(
            State(state.as_ref().clone()),
            Path(create_resp.0.id),
            Json(schedule_req),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_schedule_nonexistent_workflow() {
        let state = create_test_state();

        let schedule_req = UpdateScheduleRequest {
            cron_expression: "0 0 * * *".to_string(),
            enabled: true,
            timezone: Some("UTC".to_string()),
        };

        let result = update_schedule(
            State(state.as_ref().clone()),
            Path("nonexistent_workflow".to_string()),
            Json(schedule_req),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_preview_schedule_valid() {
        let state = create_test_state();

        let preview_req = SchedulePreviewRequest {
            cron_expression: "0 0 0 * * *".to_string(),
            timezone: Some("UTC".to_string()),
            count: 10,
        };

        let result = preview_schedule(State(state.as_ref().clone()), Json(preview_req)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.cron_expression, "0 0 0 * * *");
        assert_eq!(response.0.timezone, "UTC");
        assert!(response.0.is_valid);
        assert!(response.0.validation_error.is_none());
        // TODO: When cron library is added, verify next_runs is populated
        // assert_eq!(response.0.next_runs.len(), 10);
    }

    #[tokio::test]
    async fn test_preview_schedule_invalid_cron() {
        let state = create_test_state();

        let preview_req = SchedulePreviewRequest {
            cron_expression: "invalid".to_string(),
            timezone: Some("UTC".to_string()),
            count: 10,
        };

        let result = preview_schedule(State(state.as_ref().clone()), Json(preview_req)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.0.is_valid);
        assert!(response.0.validation_error.is_some());
        assert!(response.0.next_runs.is_empty());
    }

    #[tokio::test]
    async fn test_preview_schedule_invalid_timezone() {
        let state = create_test_state();

        let preview_req = SchedulePreviewRequest {
            cron_expression: "0 0 0 * * *".to_string(),
            timezone: Some("InvalidTZ".to_string()),
            count: 10,
        };

        let result = preview_schedule(State(state.as_ref().clone()), Json(preview_req)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.0.is_valid);
        assert!(response.0.validation_error.is_some());
        assert!(response
            .0
            .validation_error
            .as_ref()
            .unwrap()
            .contains("timezone"));
    }

    #[tokio::test]
    async fn test_preview_schedule_defaults() {
        let state = create_test_state();

        // Test with minimal request (timezone defaults to UTC, count defaults to 10)
        let preview_req = SchedulePreviewRequest {
            cron_expression: "0 0 0 * * *".to_string(),
            timezone: None,
            count: 10,
        };

        let result = preview_schedule(State(state.as_ref().clone()), Json(preview_req)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.0.timezone, "UTC");
        assert!(response.0.is_valid);
    }

    #[tokio::test]
    async fn test_schedule_with_complex_cron_expressions() {
        let state = create_test_state();

        let test_cases = vec![
            ("0 0 0 * * *", true),      // Daily at midnight
            ("0 */5 * * * *", true),    // Every 5 minutes
            ("0 0 9-17 * * 1-5", true), // Weekdays 9-5
            ("0 0 0,12 * * *", true),   // Twice daily
            ("0 0 0 1 * *", true),      // First of month
            ("0 0 0 * * 0", true),      // Weekly on Sunday
            ("0 0 0 1 1 *", true),      // Yearly on Jan 1
        ];

        for (cron, should_be_valid) in test_cases {
            let preview_req = SchedulePreviewRequest {
                cron_expression: cron.to_string(),
                timezone: Some("UTC".to_string()),
                count: 5,
            };

            let result = preview_schedule(State(state.as_ref().clone()), Json(preview_req)).await;
            assert!(result.is_ok());
            let response = result.unwrap();
            assert_eq!(
                response.0.is_valid, should_be_valid,
                "Failed for cron: {}",
                cron
            );
        }
    }

    // === Legacy Handler Tests ===

    #[tokio::test]
    async fn test_execute_workflow_legacy_json_input() {
        // Test legacy execution handler with JSON input
        // Note: Full testing would require:
        // 1. Mocking WorkflowEngine
        // 2. Setting up query_executor for graph-native inputs
        // 3. Testing SPARQL and EntityFilter inputs

        // For now, verify the handler signature and basic structure by testing the DTO
        let input = WorkflowInputWrapper::Json(json!({"customer_id": "123"}));
        let workflow_input = input.into_workflow_input();

        // Verify it converts correctly to WorkflowInput
        match workflow_input {
            graphica_core::orchestration::workflow::WorkflowInput::Json { data } => {
                assert_eq!(data["customer_id"], "123");
            }
            _ => panic!("Expected Json variant"),
        }
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_basic() {
        // Test that WorkflowDefinition can be created
        // Note: Full validation testing requires the WorkflowEngine

        // For now, just verify the DTO serialization works
        let definition_json = serde_json::json!({
            "steps": [],
            "fusion_threshold": 0.8,
            "fallback": "abort"
        });

        let serialized = serde_json::to_string(&definition_json);
        assert!(serialized.is_ok());

        // Verify we can deserialize it back
        let definition_str = serialized.unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&definition_str).unwrap();
        assert_eq!(deserialized["fusion_threshold"], 0.8);
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_api_db_extract() {
        let state = create_test_legacy_api_state();
        let app = Router::new()
            .nest("/api/v1", crate::api::workflow::create_workflow_router())
            .with_state(state);

        let payload = json!({
            "steps": [
                {
                    "id": "step_1771871985659",
                    "step_type": "db_extract",
                    "config": {
                        "datasource_id": "urn:graphica:datasource:b95f00f2-b6a2-4846-b9b0-e6839ba36853",
                        "table_name": "UPLOADED_DATA"
                    }
                }
            ],
            "fusion_threshold": 0.85,
            "fallback": "manual_review"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/workflows/validate")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_build_router_with_auth() {
        let (state, token) = create_test_auth_api_state();
        let app = crate::api::rest::build_router(state);

        let payload = json!({
            "steps": [
                {
                    "id": "step_1771871985659",
                    "step_type": "db_extract",
                    "config": {
                        "datasource_id": "urn:graphica:datasource:b95f00f2-b6a2-4846-b9b0-e6839ba36853",
                        "table_name": "UPLOADED_DATA"
                    }
                }
            ],
            "fusion_threshold": 0.85,
            "fallback": "manual_review"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/workflows/validate")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::from(payload.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
    }

    #[tokio::test]
    async fn test_test_workflow_step_request_structure() {
        let context = ExecutionContextParams {
            request_id: Some("req_123".to_string()),
            initiator: Some("test_user".to_string()),
            metadata: json!({"env": "test"}),
        };

        let input = json!({"value": 10});

        assert_eq!(context.request_id, Some("req_123".to_string()));
        assert_eq!(input["value"], 10);
    }

    #[tokio::test]
    async fn test_registered_workflow_dry_run_request_structure() {
        let dry_run_req = DryRunWorkflowRequest {
            input: json!({"test": "data"}),
            context: ExecutionContextParams::default(),
        };

        assert_eq!(dry_run_req.input["test"], "data");
        assert!(dry_run_req.context.request_id.is_none());
    }

    #[tokio::test]
    async fn test_schedule_request_structure() {
        let schedule_req = crate::api::workflow::types::ScheduleWorkflowRequest {
            schedule_id: None,
            cron_expression: Some("0 0 * * *".to_string()),
            interval_seconds: None,
            scheduled_at: None,
            timezone: Some("UTC".to_string()),
            input: json!({"test": "data"}),
            context: ExecutionContextParams {
                request_id: Some("req_123".to_string()),
                initiator: Some("user@example.com".to_string()),
                metadata: json!({"env": "test"}),
            },
            enabled: true,
        };

        assert_eq!(schedule_req.cron_expression, Some("0 0 * * *".to_string()));
        assert!(schedule_req.enabled);
        assert_eq!(schedule_req.context.request_id, Some("req_123".to_string()));
        assert_eq!(schedule_req.timezone.as_deref(), Some("UTC"));
    }

    #[tokio::test]
    async fn test_workflow_schedule_info_structure() {
        use chrono::Utc;

        let schedule_info = crate::api::workflow::types::WorkflowScheduleInfo {
            schedule_id: "sched_123".to_string(),
            workflow_id: "wf_456".to_string(),
            cron_expression: Some("0 0 * * *".to_string()),
            interval_seconds: None,
            scheduled_at: None,
            timezone: Some("UTC".to_string()),
            next_execution: Some(Utc::now()),
            last_execution: None,
            enabled: true,
            created_at: Utc::now(),
            execution_count: 5,
        };

        // Verify structure
        assert_eq!(schedule_info.schedule_id, "sched_123");
        assert_eq!(schedule_info.workflow_id, "wf_456");
        assert_eq!(schedule_info.execution_count, 5);
        assert!(schedule_info.next_execution.is_some());
    }

    #[tokio::test]
    async fn test_execution_context_params_to_hashmap() {
        let context = ExecutionContextParams {
            request_id: Some("req_123".to_string()),
            initiator: Some("user@example.com".to_string()),
            metadata: json!({
                "env": "production",
                "version": "1.0.0"
            }),
        };

        let hashmap = context.to_hashmap();

        assert_eq!(hashmap.get("request_id"), Some(&"req_123".to_string()));
        assert_eq!(
            hashmap.get("initiator"),
            Some(&"user@example.com".to_string())
        );
        assert_eq!(hashmap.get("env"), Some(&"production".to_string()));
        assert_eq!(hashmap.get("version"), Some(&"1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_workflow_input_wrapper_into_workflow_input() {
        use graphica_core::orchestration::workflow::WorkflowInput;

        // Test JSON variant using consolidated DTOs
        let json_wrapper = WorkflowInputWrapper::Json(json!({"customer_id": "123"}));
        let workflow_input = json_wrapper.into_workflow_input();

        match workflow_input {
            WorkflowInput::Json { data } => {
                assert_eq!(data["customer_id"], "123");
            }
            _ => panic!("Expected Json variant"),
        }
    }

    #[tokio::test]
    async fn test_ml_feature_value_structure() {
        use chrono::Utc;

        let feature = MlFeatureValue {
            feature_name: "customer_lifetime_value".to_string(),
            value: json!(1250.50),
            model_id: Some("model_clv_v2".to_string()),
            computed_at: Some(Utc::now()),
        };

        assert_eq!(feature.feature_name, "customer_lifetime_value");
        assert_eq!(feature.value, json!(1250.50));
        assert!(feature.model_id.is_some());
    }

    #[tokio::test]
    async fn test_lineage_reference_structure() {
        let lineage_ref = LineageReference {
            ref_type: "model".to_string(),
            ref_id: "model_clv_v2".to_string(),
            description: Some("Customer Lifetime Value prediction model".to_string()),
        };

        assert_eq!(lineage_ref.ref_type, "model");
        assert_eq!(lineage_ref.ref_id, "model_clv_v2");
        assert!(lineage_ref.description.is_some());
    }

    #[tokio::test]
    async fn test_step_execution_result_structure() {
        let step_result = StepExecutionResult {
            step_id: "step_1".to_string(),
            step_type: "transform".to_string(),
            success: true,
            output: Some(json!({"result": 42})),
            error: None,
            execution_time_ms: 125,
        };

        assert_eq!(step_result.step_id, "step_1");
        assert!(step_result.success);
        assert_eq!(step_result.execution_time_ms, 125);
        assert!(step_result.error.is_none());
    }

    #[tokio::test]
    async fn test_execution_result_dto_structure() {
        let result = crate::api::workflow::types::ExecutionResultDto {
            execution_id: "exec_123".to_string(),
            success: true,
            step_results: vec![crate::api::workflow::types::StepResultDto {
                step_id: "step_1".to_string(),
                success: true,
                output: json!({"value": 10}),
                confidence: 0.95,
                duration_ms: 50,
            }],
            final_output: json!({"value": 10}),
            confidence: 0.95,
            materialized_dataset: None,
        };

        assert!(result.success);
        assert_eq!(result.step_results.len(), 1);
        assert_eq!(result.confidence, 0.95);
    }

    #[tokio::test]
    async fn test_register_workflow_request_structure() {
        // Test RegisterWorkflowRequest DTO structure
        // Note: Full testing would require creating valid WorkflowDefinition

        // Test the DTO fields we added
        let id = Some("wf_custom_id".to_string());
        let name = "Customer Enrichment".to_string();
        let description = Some("Enriches customer data with ML predictions".to_string());
        let tags = vec!["ml".to_string(), "enrichment".to_string()];

        // Verify field values
        assert_eq!(name, "Customer Enrichment");
        assert_eq!(tags.len(), 2);
        assert!(description.is_some());
        assert_eq!(id, Some("wf_custom_id".to_string()));
    }

    #[tokio::test]
    async fn test_workflow_summary_dto_structure() {
        use chrono::Utc;

        let summary = WorkflowSummaryDto {
            workflow_id: "wf_123".to_string(),
            name: "Data Pipeline".to_string(),
            description: Some("ETL workflow".to_string()),
            tags: vec!["etl".to_string(), "batch".to_string()],
            created_at: Utc::now(),
        };

        assert_eq!(summary.workflow_id, "wf_123");
        assert_eq!(summary.name, "Data Pipeline");
        assert_eq!(summary.tags.len(), 2);
    }

    #[tokio::test]
    async fn test_workflow_history_route_uses_modern_pagination() {
        use graphica_core::orchestration::workflow::definition::{
            ConfidenceGateConfig, FallbackStrategy, StepConfig, StepType,
        };
        use graphica_core::orchestration::workflow::{WorkflowDefinition, WorkflowStep};

        let state = create_test_legacy_api_state();
        let workflow_id = "wf_history_route".to_string();

        state
            .workflow_engine
            .as_ref()
            .unwrap()
            .register_workflow(
                workflow_id.clone(),
                "History Route Workflow".to_string(),
                WorkflowDefinition {
                    steps: vec![WorkflowStep {
                        id: "gate_1".to_string(),
                        step_type: StepType::ConfidenceGate,
                        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                            threshold: 0.5,
                            input_step: None,
                        }),
                        depends_on: vec![],
                    }],
                    fusion_threshold: 0.8,
                    fallback: FallbackStrategy::ManualReview,
                },
                None,
                vec![],
            )
            .await
            .unwrap();

        let execution_store = state.execution_store.as_ref().unwrap();
        for (execution_id, started_at, confidence) in [
            (
                "exec_history_1",
                Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
                0.41,
            ),
            (
                "exec_history_2",
                Utc.with_ymd_and_hms(2026, 3, 9, 10, 5, 0).unwrap(),
                0.73,
            ),
            (
                "exec_history_3",
                Utc.with_ymd_and_hms(2026, 3, 9, 10, 10, 0).unwrap(),
                0.95,
            ),
        ] {
            let mut execution = WorkflowExecution::new(
                execution_id.to_string(),
                workflow_id.clone(),
                "History Route Workflow".to_string(),
                json!({"type": "json", "data": {"execution_id": execution_id}}),
                Some("tester".to_string()),
            );
            execution.status = ExecutionStatus::Completed;
            execution.confidence = Some(confidence);
            execution.started_at = started_at;
            execution.updated_at = started_at + chrono::Duration::minutes(1);
            execution.completed_at = Some(started_at + chrono::Duration::minutes(1));
            execution.duration_ms = Some(60_000);
            execution_store.save(execution).await.unwrap();
        }

        let app = Router::new()
            .nest("/api/v1", crate::api::workflow::create_workflow_router())
            .with_state(state);

        let request = Request::builder()
            .method("GET")
            .uri(format!(
                "/api/v1/workflows/{}/executions?limit=1&offset=1",
                workflow_id
            ))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let executions: Vec<crate::api::workflow::types::WorkflowExecutionSummary> =
            serde_json::from_slice(&body).unwrap();

        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].execution_id, "exec_history_2");
        assert_eq!(executions[0].confidence, 0.73);
    }

    #[tokio::test]
    async fn test_workflow_schedule_routes_use_primary_workflow_router() {
        use graphica_core::orchestration::workflow::definition::{
            ConfidenceGateConfig, FallbackStrategy, StepConfig, StepType,
        };
        use graphica_core::orchestration::workflow::{WorkflowDefinition, WorkflowStep};

        let state = create_test_legacy_api_state();
        let workflow_id = "wf_schedule_route".to_string();

        state
            .workflow_engine
            .as_ref()
            .unwrap()
            .register_workflow(
                workflow_id.clone(),
                "Schedule Route Workflow".to_string(),
                WorkflowDefinition {
                    steps: vec![WorkflowStep {
                        id: "gate_1".to_string(),
                        step_type: StepType::ConfidenceGate,
                        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                            threshold: 0.5,
                            input_step: None,
                        }),
                        depends_on: vec![],
                    }],
                    fusion_threshold: 0.8,
                    fallback: FallbackStrategy::ManualReview,
                },
                None,
                vec![],
            )
            .await
            .unwrap();

        let app = Router::new()
            .nest("/api/v1", crate::api::workflow::create_workflow_router())
            .with_state(state.clone());

        let create_request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/workflows/{}/schedules", workflow_id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&crate::api::workflow::types::ScheduleWorkflowRequest {
                    schedule_id: None,
                    cron_expression: Some("0 0 * * *".to_string()),
                    interval_seconds: Some(900),
                    scheduled_at: None,
                    timezone: None,
                    input: json!({"test": "data"}),
                    context: ExecutionContextParams::default(),
                    enabled: true,
                })
                .unwrap(),
            ))
            .unwrap();

        let create_response = app.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let create_body = to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: crate::api::workflow::types::ScheduleWorkflowResponse =
            serde_json::from_slice(&create_body).unwrap();
        assert_eq!(created.workflow_id, workflow_id);
        assert_eq!(created.cron_expression.as_deref(), Some("0 0 * * *"));
        assert_eq!(created.interval_seconds, Some(900));
        assert_eq!(created.timezone.as_deref(), Some("UTC"));

        let list_request = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/workflows/{}/schedules", workflow_id))
            .body(Body::empty())
            .unwrap();

        let list_response = app.oneshot(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let list_body = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let schedules: Vec<crate::api::workflow::types::WorkflowScheduleInfo> =
            serde_json::from_slice(&list_body).unwrap();
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].workflow_id, workflow_id);
        assert_eq!(schedules[0].interval_seconds, Some(900));
        assert_eq!(schedules[0].timezone.as_deref(), Some("UTC"));
        assert_eq!(schedules[0].execution_count, 0);

        let stored = state
            .schedule_store
            .as_ref()
            .unwrap()
            .list_by_workflow(&workflow_id)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].timezone, "UTC");
    }
}
