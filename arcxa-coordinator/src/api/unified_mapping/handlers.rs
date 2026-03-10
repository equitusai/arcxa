//! Unified Mapping API Handlers
//!
//! HTTP request handlers for unified mapping operations including
//! session management, conflict resolution, and database loading.

use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::mapping::bindings::{
    BindingProvenance as DomainBindingProvenance, UpsertBindingRequest,
};
use crate::mapping::execution::{
    ExecutionBackend, ExecutionRequest, ExecutionTelemetryEvent, ExecutorRegistry,
};
use crate::mapping::loader::LoaderConfig;
use crate::mapping::multi_source::conflict::ConflictResolver;
use crate::mapping::multi_source::CreateUnifiedSessionRequest as CoreCreateRequest;
use crate::mapping::planner::{
    GoalFilter as PlannerGoalFilter, GoalRequest as PlannerGoalRequest, GoalSqlPlanner,
    PhysicalFieldBinding, SqlDialect as PlannerSqlDialect,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use std::sync::Arc;
use utoipa;

fn api_to_planner_dialect(dialect: SqlDialect) -> PlannerSqlDialect {
    match dialect {
        SqlDialect::Postgresql => PlannerSqlDialect::PostgreSql,
        SqlDialect::Edb => PlannerSqlDialect::Edb,
        SqlDialect::Oracle => PlannerSqlDialect::Oracle,
        SqlDialect::Saphana => PlannerSqlDialect::SapHana,
        SqlDialect::Db2 => PlannerSqlDialect::Db2,
        SqlDialect::Databricks => PlannerSqlDialect::Databricks,
    }
}

fn planner_to_api_dialect(dialect: PlannerSqlDialect) -> SqlDialect {
    match dialect {
        PlannerSqlDialect::PostgreSql => SqlDialect::Postgresql,
        PlannerSqlDialect::Edb => SqlDialect::Edb,
        PlannerSqlDialect::Oracle => SqlDialect::Oracle,
        PlannerSqlDialect::SapHana => SqlDialect::Saphana,
        PlannerSqlDialect::Db2 => SqlDialect::Db2,
        PlannerSqlDialect::Databricks => SqlDialect::Databricks,
    }
}

fn api_dialect_to_storage_value(dialect: &SqlDialect) -> &'static str {
    match dialect {
        SqlDialect::Postgresql => "postgresql",
        SqlDialect::Edb => "edb",
        SqlDialect::Oracle => "oracle",
        SqlDialect::Saphana => "saphana",
        SqlDialect::Db2 => "db2",
        SqlDialect::Databricks => "databricks",
    }
}

fn explain_metadata_for_dialect(dialect: PlannerSqlDialect) -> ExplainMetadataResponse {
    match dialect {
        PlannerSqlDialect::PostgreSql | PlannerSqlDialect::Edb => ExplainMetadataResponse {
            mode: "inline_result_set".to_string(),
            follow_up_query: None,
            notes: vec!["Explain output is returned directly by the query.".to_string()],
        },
        PlannerSqlDialect::Oracle => ExplainMetadataResponse {
            mode: "plan_table".to_string(),
            follow_up_query: Some(
                "SELECT * FROM TABLE(DBMS_XPLAN.DISPLAY(NULL, NULL, 'BASIC +PREDICATE'))"
                    .to_string(),
            ),
            notes: vec![
                "Run EXPLAIN PLAN FOR first, then read plan table output via DBMS_XPLAN."
                    .to_string(),
            ],
        },
        PlannerSqlDialect::SapHana => ExplainMetadataResponse {
            mode: "statement_name_plan_table".to_string(),
            follow_up_query: Some(
                "SELECT * FROM EXPLAIN_PLAN_TABLE WHERE STATEMENT_NAME = 'GRAPHICA_PLAN' ORDER BY ID"
                    .to_string(),
            ),
            notes: vec![
                "Explain output is persisted to EXPLAIN_PLAN_TABLE for the statement name."
                    .to_string(),
            ],
        },
        PlannerSqlDialect::Db2 => ExplainMetadataResponse {
            mode: "db2_explain_tables".to_string(),
            follow_up_query: Some(
                "SELECT * FROM EXPLAIN_OPERATOR ORDER BY EXPLAIN_REQUESTER, EXPLAIN_TIME, OPERATOR_ID"
                    .to_string(),
            ),
            notes: vec![
                "Ensure DB2 explain tables are created for the executing schema.".to_string(),
                "Use EXPLAIN_INSTANCE filters in production to isolate a single run.".to_string(),
            ],
        },
        PlannerSqlDialect::Databricks => ExplainMetadataResponse {
            mode: "inline_result_set".to_string(),
            follow_up_query: None,
            notes: vec![
                "EXPLAIN FORMAT=JSON returns plan text/JSON in the immediate result set."
                    .to_string(),
            ],
        },
    }
}

fn database_type_to_execution_backend(database_type: &DatabaseType) -> Option<ExecutionBackend> {
    match database_type {
        DatabaseType::DB2 => Some(ExecutionBackend::Db2),
        DatabaseType::Oracle => Some(ExecutionBackend::Oracle),
        DatabaseType::Databricks => Some(ExecutionBackend::Databricks),
        DatabaseType::PostgreSQL => None,
    }
}

fn database_type_storage_value(database_type: &DatabaseType) -> &'static str {
    match database_type {
        DatabaseType::PostgreSQL => "postgresql",
        DatabaseType::DB2 => "db2",
        DatabaseType::Oracle => "oracle",
        DatabaseType::Databricks => "databricks",
    }
}

fn api_load_status_from_domain(
    status: crate::mapping::multi_source::UnifiedLoadJobStatus,
) -> LoadJobStatus {
    match status {
        crate::mapping::multi_source::UnifiedLoadJobStatus::Queued => LoadJobStatus::Queued,
        crate::mapping::multi_source::UnifiedLoadJobStatus::Running => LoadJobStatus::Running,
        crate::mapping::multi_source::UnifiedLoadJobStatus::Submitted => LoadJobStatus::Submitted,
        crate::mapping::multi_source::UnifiedLoadJobStatus::Completed => LoadJobStatus::Completed,
        crate::mapping::multi_source::UnifiedLoadJobStatus::Failed => LoadJobStatus::Failed,
        crate::mapping::multi_source::UnifiedLoadJobStatus::Cancelled => LoadJobStatus::Cancelled,
    }
}

fn callback_status_to_domain(
    status: &ExternalLoadJobCallbackStatus,
) -> crate::mapping::multi_source::UnifiedLoadJobStatus {
    match status {
        ExternalLoadJobCallbackStatus::Running => {
            crate::mapping::multi_source::UnifiedLoadJobStatus::Running
        }
        ExternalLoadJobCallbackStatus::Submitted => {
            crate::mapping::multi_source::UnifiedLoadJobStatus::Submitted
        }
        ExternalLoadJobCallbackStatus::Completed => {
            crate::mapping::multi_source::UnifiedLoadJobStatus::Completed
        }
        ExternalLoadJobCallbackStatus::Failed => {
            crate::mapping::multi_source::UnifiedLoadJobStatus::Failed
        }
        ExternalLoadJobCallbackStatus::Cancelled => {
            crate::mapping::multi_source::UnifiedLoadJobStatus::Cancelled
        }
    }
}

fn execution_backend_label(backend: ExecutionBackend) -> &'static str {
    match backend {
        ExecutionBackend::Db2 => "db2",
        ExecutionBackend::Oracle => "oracle",
        ExecutionBackend::Databricks => "databricks",
    }
}

fn emit_external_execution_observability_event(event: &ExecutionTelemetryEvent) {
    tracing::info!(
        event_id = %event.event_id,
        backend = %execution_backend_label(event.backend),
        run_id = %event.run_id,
        session_id = %event.session_id,
        status = ?event.status,
        external_run_id = ?event.external_run_id,
        message = %event.message,
        "External execution telemetry event"
    );
}

fn record_external_execution_lineage(
    lineage_generator: Option<&Arc<crate::workflows::lineage::WorkflowLineageGenerator>>,
    execution_id: &str,
    backend: ExecutionBackend,
    status: &str,
    started_at: chrono::DateTime<Utc>,
    completed_at: chrono::DateTime<Utc>,
) {
    if let Some(generator) = lineage_generator {
        let step_id = format!(
            "external_executor_{}_{}",
            execution_backend_label(backend),
            status
        );
        let step_type = format!(
            "external_executor:{}:{}",
            execution_backend_label(backend),
            status
        );
        if let Err(e) = generator.record_step_execution(
            execution_id,
            &step_id,
            &step_type,
            Vec::new(),
            started_at,
            completed_at,
        ) {
            tracing::warn!(
                "Failed to record external executor lineage step {}: {}",
                step_id,
                e
            );
        }
    }
}

fn binding_response_from_domain(
    binding: crate::mapping::bindings::OntologyPhysicalBinding,
) -> OntologyBindingResponse {
    let sql_dialect = match binding.dialect.to_lowercase().as_str() {
        "edb" => SqlDialect::Edb,
        "oracle" => SqlDialect::Oracle,
        "saphana" | "sap_hana" | "sap hana" => SqlDialect::Saphana,
        "db2" => SqlDialect::Db2,
        "databricks" => SqlDialect::Databricks,
        _ => SqlDialect::Postgresql,
    };

    OntologyBindingResponse {
        id: binding.id,
        source_id: binding.source_id,
        entity_uri: binding.entity_uri,
        ontology_uri: binding.ontology_uri,
        table: binding.table,
        column: binding.column,
        sql_dialect,
        confidence: binding.confidence,
        status: format!("{:?}", binding.status).to_lowercase(),
        version: binding.version,
        binding_hash: binding.binding_hash,
        updated_at: binding.updated_at,
        updated_by: binding.updated_by,
    }
}

// ============================================================================
// Session Management Handlers
// ============================================================================

/// Plan goal-driven SQL from ontology property requirements.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/plan-sql",
    request_body = PlanGoalSqlRequest,
    responses(
        (status = 200, description = "Goal SQL planned successfully", body = PlanGoalSqlResponse),
        (status = 400, description = "Invalid request or missing ontology bindings", body = ErrorResponse),
        (status = 500, description = "Internal server error while inferring schema or planning SQL", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn plan_goal_sql(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<PlanGoalSqlRequest>,
) -> Result<Json<PlanGoalSqlResponse>, ApiError> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Data source catalog not available".to_string())
    })?;

    if request.required_properties.is_empty() {
        return Err(ApiError::bad_request(
            "required_properties cannot be empty".to_string(),
        ));
    }
    if request.binding_strategy == GoalBindingStrategy::Inline && request.bindings.is_empty() {
        return Err(ApiError::bad_request(
            "bindings cannot be empty when binding_strategy is inline".to_string(),
        ));
    }

    let schema = catalog
        .infer_schema(
            &request.source_id,
            request.table_name.as_deref(),
            request.sample_size,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to infer schema: {}", e)))?;

    let planner = GoalSqlPlanner::new(schema.clone());

    let goal = PlannerGoalRequest {
        entity_uri: request.entity_uri.clone(),
        required_properties: request.required_properties.clone(),
        filters: request
            .filters
            .iter()
            .map(|f| PlannerGoalFilter {
                ontology_uri: f.ontology_uri.clone(),
                value: f.value.clone(),
            })
            .collect(),
        limit: request.limit,
    };

    let bindings: Vec<PhysicalFieldBinding> = match request.binding_strategy {
        GoalBindingStrategy::Inline => request
            .bindings
            .iter()
            .map(|binding| PhysicalFieldBinding {
                ontology_uri: binding.ontology_uri.clone(),
                table: binding.table.clone(),
                column: binding.column.clone(),
                confidence: binding.confidence,
            })
            .collect(),
        GoalBindingStrategy::Stored => {
            let binding_service = state.binding_service.as_ref().ok_or_else(|| {
                ApiError::service_unavailable(
                    "Stored binding strategy requested but binding service is not available"
                        .to_string(),
                )
            })?;

            let stale = binding_service
                .mark_stale_bindings_for_schema(
                    &request.source_id,
                    &request.entity_uri,
                    &schema,
                    "system:plan_sql_schema_refresh",
                )
                .map_err(|e| {
                    ApiError::internal(format!("Failed to apply binding drift checks: {}", e))
                })?;
            if stale > 0 {
                tracing::warn!(
                    "Marked {} ontology binding(s) as stale for source '{}' entity '{}'",
                    stale,
                    request.source_id,
                    request.entity_uri
                );
            }

            binding_service
                .get_current_bindings_for_goal(
                    &request.source_id,
                    &request.entity_uri,
                    &request.required_properties,
                )
                .map_err(|e| ApiError::internal(format!("Failed to load stored bindings: {}", e)))?
                .into_iter()
                .map(|binding| PhysicalFieldBinding {
                    ontology_uri: binding.ontology_uri,
                    table: binding.table,
                    column: binding.column,
                    confidence: binding.confidence,
                })
                .collect()
        }
    };

    let planned = planner
        .plan_goal_with_options(
            &goal,
            &bindings,
            api_to_planner_dialect(request.sql_dialect.clone()),
            request.include_explain_plan,
        )
        .map_err(|e| ApiError::bad_request(format!("Failed to plan SQL: {}", e)))?;

    Ok(Json(PlanGoalSqlResponse {
        source_id: request.source_id,
        schema_name: schema.name,
        sql_dialect: planner_to_api_dialect(planned.dialect),
        binding_strategy: request.binding_strategy,
        sql: planned.sql,
        explain_sql: planned.explain_sql,
        explain_metadata: if request.include_explain_plan {
            Some(explain_metadata_for_dialect(planned.dialect))
        } else {
            None
        },
        selected_tables: planned.selected_tables,
        covered_properties: planned.covered_properties,
        missing_properties: planned.missing_properties,
        joins: planned
            .joins
            .into_iter()
            .map(|join| PlannedJoinResponse {
                from_table: join.from_table,
                to_table: join.to_table,
                condition: join.condition,
            })
            .collect(),
        parameters: planned
            .parameters
            .into_iter()
            .map(|param| PlannedSqlParameterResponse {
                index: param.index,
                placeholder: param.placeholder,
                ontology_uri: param.ontology_uri,
                value: param.value,
                data_type: param.data_type,
            })
            .collect(),
    }))
}

/// Upsert ontology-to-physical bindings for a source/entity.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/bindings",
    request_body = UpsertOntologyBindingsRequest,
    responses(
        (status = 200, description = "Bindings upserted", body = UpsertOntologyBindingsResponse),
        (status = 400, description = "Invalid binding payload", body = ErrorResponse),
        (status = 503, description = "Binding service unavailable", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn upsert_ontology_bindings(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<UpsertOntologyBindingsRequest>,
) -> Result<Json<UpsertOntologyBindingsResponse>, ApiError> {
    let binding_service = state.binding_service.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Binding service not available".to_string())
    })?;

    if request.bindings.is_empty() {
        return Err(ApiError::bad_request(
            "bindings must contain at least one binding".to_string(),
        ));
    }

    let UpsertOntologyBindingsRequest {
        source_id,
        entity_uri,
        updated_by,
        bindings,
    } = request;

    let mut updated = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let provenance = binding.provenance.unwrap_or(BindingProvenanceInput {
            workflow_id: None,
            session_id: None,
            approved_by: Some(updated_by.clone()),
            approval_reason: None,
            observed_schema_hash: None,
        });

        let saved = binding_service
            .upsert_binding(UpsertBindingRequest {
                source_id: source_id.clone(),
                entity_uri: entity_uri.clone(),
                ontology_uri: binding.ontology_uri,
                table: binding.table,
                column: binding.column,
                dialect: api_dialect_to_storage_value(&binding.sql_dialect).to_string(),
                confidence: binding.confidence,
                updated_by: updated_by.clone(),
                provenance: DomainBindingProvenance {
                    workflow_id: provenance.workflow_id,
                    session_id: provenance.session_id,
                    approved_by: provenance.approved_by,
                    approval_reason: provenance.approval_reason,
                    observed_schema_hash: provenance.observed_schema_hash,
                },
            })
            .map_err(|e| ApiError::internal(format!("Failed to upsert binding: {}", e)))?;

        updated.push(binding_response_from_domain(saved));
    }

    Ok(Json(UpsertOntologyBindingsResponse { updated }))
}

/// List current ontology bindings for a source/entity.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/bindings",
    params(ListOntologyBindingsQuery),
    responses(
        (status = 200, description = "Current bindings", body = ListOntologyBindingsResponse),
        (status = 503, description = "Binding service unavailable", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn list_ontology_bindings(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListOntologyBindingsQuery>,
) -> Result<Json<ListOntologyBindingsResponse>, ApiError> {
    let binding_service = state.binding_service.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Binding service not available".to_string())
    })?;

    let bindings = binding_service
        .list_current_bindings(&query.source_id, &query.entity_uri)
        .map_err(|e| ApiError::internal(format!("Failed to list bindings: {}", e)))?;

    Ok(Json(ListOntologyBindingsResponse {
        bindings: bindings
            .into_iter()
            .map(binding_response_from_domain)
            .collect(),
    }))
}

/// List version history for a source/entity/property binding.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/bindings/history",
    params(BindingHistoryQuery),
    responses(
        (status = 200, description = "Binding history", body = BindingHistoryResponse),
        (status = 503, description = "Binding service unavailable", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn binding_history(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<BindingHistoryQuery>,
) -> Result<Json<BindingHistoryResponse>, ApiError> {
    let binding_service = state.binding_service.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Binding service not available".to_string())
    })?;

    let history = binding_service
        .list_binding_history(&query.source_id, &query.entity_uri, &query.ontology_uri)
        .map_err(|e| ApiError::internal(format!("Failed to load binding history: {}", e)))?;

    Ok(Json(BindingHistoryResponse {
        history: history
            .into_iter()
            .map(binding_response_from_domain)
            .collect(),
    }))
}

/// Diff ontology requirements against current source/entity binding coverage.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/bindings/coverage",
    request_body = BindingCoverageRequest,
    responses(
        (status = 200, description = "Binding coverage diff", body = BindingCoverageResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 503, description = "Binding service unavailable", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn binding_coverage(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<BindingCoverageRequest>,
) -> Result<Json<BindingCoverageResponse>, ApiError> {
    let binding_service = state.binding_service.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Binding service not available".to_string())
    })?;

    if request.required_properties.is_empty() {
        return Err(ApiError::bad_request(
            "required_properties cannot be empty".to_string(),
        ));
    }

    let schema = if request.validate_schema {
        let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
            ApiError::service_unavailable(
                "Data source catalog required when validate_schema=true".to_string(),
            )
        })?;
        let schema = catalog
            .infer_schema(
                &request.source_id,
                request.table_name.as_deref(),
                request.sample_size,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to infer schema: {}", e)))?;

        let stale_count = binding_service
            .mark_stale_bindings_for_schema(
                &request.source_id,
                &request.entity_uri,
                &schema,
                "system:binding_coverage_schema_refresh",
            )
            .map_err(|e| ApiError::internal(format!("Failed to refresh stale bindings: {}", e)))?;
        if stale_count > 0 {
            tracing::warn!(
                "Marked {} stale binding(s) during coverage diff for source '{}' entity '{}'",
                stale_count,
                request.source_id,
                request.entity_uri
            );
        }
        Some(schema)
    } else {
        None
    };

    let diff = binding_service
        .diff_coverage(
            &request.source_id,
            &request.entity_uri,
            &request.required_properties,
            schema.as_ref(),
        )
        .map_err(|e| ApiError::internal(format!("Failed to compute binding coverage: {}", e)))?;

    Ok(Json(BindingCoverageResponse {
        source_id: request.source_id,
        entity_uri: request.entity_uri,
        required_properties: diff.required_properties,
        covered_properties: diff.covered_properties,
        missing_properties: diff.missing_properties,
        stale_properties: diff.stale_properties,
        unmapped_properties: diff.unmapped_properties,
        coverage_ratio: diff.coverage_ratio,
    }))
}

/// Create new unified mapping session
///
/// Creates a new unified mapping session by consolidating multiple source CSV mapping
/// sessions into a single unified schema targeting a normalized relational database.
/// The system automatically detects conflicts between overlapping field mappings and
/// suggests resolution strategies.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/unified-sessions",
    request_body = CreateUnifiedSessionRequest,
    responses(
        (status = 200, description = "Unified session created successfully with field mappings and detected conflicts", body = UnifiedSessionResponse),
        (status = 400, description = "Invalid request - missing required fields or invalid source session IDs", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to create unified session", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn create_unified_session(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateUnifiedSessionRequest>,
) -> Result<Json<UnifiedSessionResponse>, ApiError> {
    // Get unified mapping coordinator
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::info!(
        "Creating unified session from {} source sessions for user {}",
        request.source_session_ids.len(),
        request.created_by
    );

    // Create coordinator request
    let core_request = CoreCreateRequest {
        source_session_ids: request.source_session_ids.clone(),
        target_database: request.target_database.clone(),
        created_by: request.created_by.clone(),
    };

    // Create unified session
    let response = coordinator
        .create_unified_session(core_request)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create unified session: {}", e);
            ApiError::internal(format!("Failed to create unified session: {}", e))
        })?;

    // Retrieve the created session to return full details
    let session = coordinator
        .get_unified_session(&response.session_id)
        .map_err(|e| {
            tracing::error!("Failed to retrieve created session: {}", e);
            ApiError::internal(format!("Failed to retrieve created session: {}", e))
        })?
        .ok_or_else(|| ApiError::internal("Created session not found".to_string()))?;

    // Convert to response DTO
    let response_dto: UnifiedSessionResponse = session.into();

    tracing::info!(
        "Created unified session {}: {} field mappings, {} conflicts",
        response_dto.id,
        response_dto.field_mappings.len(),
        response_dto.conflicts.len()
    );

    Ok(Json(response_dto))
}

/// Get unified session by ID
///
/// Retrieves a unified mapping session by its unique identifier, including all field
/// mappings, conflicts, resolution strategies, and current status.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/unified-sessions/{id}",
    params(
        ("id" = String, Path, description = "Unified session ID")
    ),
    responses(
        (status = 200, description = "Unified session retrieved successfully", body = UnifiedSessionResponse),
        (status = 404, description = "Unified session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to retrieve session", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn get_unified_session(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<UnifiedSessionResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::debug!("Getting unified session: {}", id);

    let session = coordinator
        .get_unified_session(&id)
        .map_err(|e| {
            tracing::error!("Failed to get unified session {}: {}", id, e);
            ApiError::internal(format!("Failed to get unified session: {}", e))
        })?
        .ok_or_else(|| ApiError::not_found(format!("Unified session not found: {}", id)))?;

    Ok(Json(session.into()))
}

/// List unified sessions with optional filtering
///
/// Lists all unified mapping sessions with optional filtering by status and creator.
/// Supports pagination for large result sets. Returns summary information including
/// statistics for each session.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/unified-sessions",
    params(
        ListUnifiedSessionsQuery
    ),
    responses(
        (status = 200, description = "List of unified sessions with pagination metadata", body = ListUnifiedSessionsResponse),
        (status = 500, description = "Internal server error - failed to list sessions", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn list_unified_sessions(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListUnifiedSessionsQuery>,
) -> Result<Json<ListUnifiedSessionsResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::debug!("Listing unified sessions with limit={}", query.limit);

    // Get all sessions (filtering will be applied in-memory)
    let all_sessions = coordinator
        .list_unified_sessions(Some(query.limit + query.offset))
        .map_err(|e| {
            tracing::error!("Failed to list unified sessions: {}", e);
            ApiError::internal(format!("Failed to list unified sessions: {}", e))
        })?;

    // Apply filters
    let mut filtered: Vec<_> = all_sessions
        .into_iter()
        .filter(|session| {
            // Filter by status if specified
            if let Some(ref status) = query.status {
                if session.status != *status {
                    return false;
                }
            }
            // Filter by created_by if specified
            if let Some(ref created_by) = query.created_by {
                if &session.created_by != created_by {
                    return false;
                }
            }
            true
        })
        .collect();

    let total_count = filtered.len();

    // Apply pagination
    let sessions: Vec<UnifiedSessionSummary> = filtered
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|session| {
            let stats = SessionStatistics {
                total_field_mappings: session.field_mappings.len(),
                total_conflicts: session.conflicts.len(),
                unresolved_conflicts: session.conflicts.iter().filter(|c| !c.resolved).count(),
                total_source_sessions: session.source_sessions.len(),
            };

            UnifiedSessionSummary {
                id: session.id.clone(),
                source_sessions: session.source_sessions.clone(),
                status: session.status.clone(),
                created_at: session.created_at,
                created_by: session.created_by.clone(),
                updated_at: session.updated_at,
                stats,
            }
        })
        .collect();

    Ok(Json(ListUnifiedSessionsResponse {
        sessions,
        total_count,
        offset: query.offset,
        limit: query.limit,
    }))
}

/// Update unified session (field mappings)
///
/// Updates an existing unified mapping session, allowing modification of target database
/// configuration and field mappings. Commonly used to adjust mappings after initial
/// creation or to change the target database.
#[utoipa::path(
    put,
    path = "/api/v1/mapping/unified-sessions/{id}",
    params(
        ("id" = String, Path, description = "Unified session ID")
    ),
    request_body = UpdateUnifiedSessionRequest,
    responses(
        (status = 200, description = "Unified session updated successfully", body = UnifiedSessionResponse),
        (status = 400, description = "Invalid request - malformed field mappings or database config", body = ErrorResponse),
        (status = 404, description = "Unified session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to update session", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn update_unified_session(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateUnifiedSessionRequest>,
) -> Result<Json<UnifiedSessionResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::info!("Updating unified session: {}", id);

    // Get existing session
    let mut session = coordinator
        .get_unified_session(&id)
        .map_err(|e| {
            tracing::error!("Failed to get unified session {}: {}", id, e);
            ApiError::internal(format!("Failed to get unified session: {}", e))
        })?
        .ok_or_else(|| ApiError::not_found(format!("Unified session not found: {}", id)))?;

    // Update target database if provided
    if let Some(target_database) = request.target_database {
        session.target_database = target_database;
    }

    // Update field mappings if provided
    if let Some(field_mappings) = request.field_mappings {
        session.field_mappings = field_mappings;
    }

    // Update timestamp
    session.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Store updated session (assumes coordinator has an update method)
    // For now, we'll use the storage directly
    tracing::warn!(
        "Update functionality not fully implemented - session modifications may not persist"
    );

    Ok(Json(session.into()))
}

/// Delete unified session
///
/// Permanently deletes a unified mapping session. This operation cannot be undone.
/// Source mapping sessions are not affected by this operation.
#[utoipa::path(
    delete,
    path = "/api/v1/mapping/unified-sessions/{id}",
    params(
        ("id" = String, Path, description = "Unified session ID")
    ),
    responses(
        (status = 204, description = "Unified session deleted successfully"),
        (status = 404, description = "Unified session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to delete session", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn delete_unified_session(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::info!("Deleting unified session: {}", id);

    coordinator.delete_unified_session(&id).map_err(|e| {
        tracing::error!("Failed to delete unified session {}: {}", id, e);
        ApiError::internal(format!("Failed to delete unified session: {}", e))
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Conflict Resolution Handlers
// ============================================================================

/// Resolve conflicts in unified session
///
/// Resolves field mapping conflicts by applying user-specified resolution strategies.
/// Supports multiple strategies including UsePrimary, Merge, Coalesce, and CustomRule.
/// Once all conflicts are resolved, the session status transitions to ReadyToLoad.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/unified-sessions/{id}/resolve-conflicts",
    params(
        ("id" = String, Path, description = "Unified session ID")
    ),
    request_body = ResolveConflictsRequest,
    responses(
        (status = 200, description = "Conflicts resolved successfully with updated session status", body = ResolveConflictsResponse),
        (status = 400, description = "Invalid request - invalid conflict IDs or resolution strategies", body = ErrorResponse),
        (status = 404, description = "Unified session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to resolve conflicts", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn resolve_conflicts(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<ResolveConflictsRequest>,
) -> Result<Json<ResolveConflictsResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::info!("Resolving conflicts for unified session: {}", id);

    // Get session
    let mut session = coordinator
        .get_unified_session(&id)
        .map_err(|e| {
            tracing::error!("Failed to get unified session {}: {}", id, e);
            ApiError::internal(format!("Failed to get unified session: {}", e))
        })?
        .ok_or_else(|| ApiError::not_found(format!("Unified session not found: {}", id)))?;

    // Apply resolutions to conflicts
    let mut conflicts_resolved = 0;
    for (conflict_id, resolution_choice) in request.resolutions {
        if let Some(conflict) = session.conflicts.iter_mut().find(|c| c.id == conflict_id) {
            // Mark conflict as resolved
            conflict.resolved = true;
            conflicts_resolved += 1;

            // Update corresponding field mapping with resolution strategy
            if let Some(mapping) = session
                .field_mappings
                .iter_mut()
                .find(|m| m.ontology_term_uri == conflict.ontology_term_uri)
            {
                mapping.conflict_resolution = resolution_choice.strategy;
            }
        }
    }

    // Update session status if all conflicts resolved
    let remaining_conflicts = session.conflicts.iter().filter(|c| !c.resolved).count();
    let new_status = if remaining_conflicts == 0 {
        crate::mapping::multi_source::UnifiedSessionStatus::ReadyToLoad
    } else {
        crate::mapping::multi_source::UnifiedSessionStatus::ConflictsDetected
    };

    session.status = new_status.clone();
    session.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Store updated session
    // TODO: Add update method to coordinator
    tracing::warn!("Conflict resolution updates may not persist - update method needed");

    Ok(Json(ResolveConflictsResponse {
        session_id: id,
        conflicts_resolved,
        remaining_conflicts,
        new_status,
    }))
}

// ============================================================================
// Database Loading Handlers
// ============================================================================

/// Load unified session to target database
///
/// Initiates a background job to load unified session data to the target database.
/// Supports PostgreSQL, DB2, and Oracle databases with high-performance bulk loading
/// (COPY for PostgreSQL). The session must be in ReadyToLoad status with all conflicts
/// resolved before loading can begin.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/unified-sessions/{id}/load",
    params(
        ("id" = String, Path, description = "Unified session ID")
    ),
    request_body = LoadToDatabaseRequest,
    responses(
        (status = 200, description = "Load job queued successfully - use job_id to track progress", body = LoadToDatabaseResponse),
        (status = 400, description = "Invalid request - session not ready to load or invalid database config", body = ErrorResponse),
        (status = 404, description = "Unified session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to initiate load job", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn load_to_database(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<LoadToDatabaseRequest>,
) -> Result<Json<LoadToDatabaseResponse>, ApiError> {
    let coordinator = state
        .unified_mapping_coordinator
        .as_ref()
        .ok_or_else(|| {
            ApiError::internal("Unified mapping coordinator not initialized".to_string())
        })?
        .clone();

    tracing::info!(
        "Loading unified session {} to {:?} database",
        id,
        request.database_type
    );

    // Get session
    let session = coordinator
        .get_unified_session(&id)
        .map_err(|e| {
            tracing::error!("Failed to get unified session {}: {}", id, e);
            ApiError::internal(format!("Failed to get unified session: {}", e))
        })?
        .ok_or_else(|| ApiError::not_found(format!("Unified session not found: {}", id)))?;

    // Check if session is ready to load
    if session.status != crate::mapping::multi_source::UnifiedSessionStatus::ReadyToLoad {
        return Err(ApiError::bad_request(format!(
            "Session not ready to load. Status: {:?}. Please resolve all conflicts first.",
            session.status
        )));
    }

    // Persist load job and mark session as loading.
    let load_job = coordinator
        .create_load_job(&id, database_type_storage_value(&request.database_type))
        .map_err(|e| ApiError::internal(format!("Failed to create load job: {}", e)))?;
    let load_job_id = load_job.id.clone();

    coordinator
        .update_unified_session_status(
            &id,
            crate::mapping::multi_source::UnifiedSessionStatus::Loading,
        )
        .map_err(|e| ApiError::internal(format!("Failed to set session to loading: {}", e)))?;

    // Create loader configuration
    let _loader_config = LoaderConfig {
        batch_size: request.batch_size,
        create_tables: request.create_tables,
        drop_existing: false,
        use_transactions: true,
    };

    // Initialize conflict resolver (for future use)
    let _conflict_resolver = ConflictResolver::new();

    // Build connection string from connection config
    let connection_string = format!(
        "host={} port={} dbname={} user={} password={}{}",
        request.connection_config.host,
        request.connection_config.port,
        request.connection_config.database,
        request.connection_config.username,
        request.connection_config.password,
        request
            .connection_config
            .ssl_mode
            .as_ref()
            .map(|mode| format!(" sslmode={}", mode))
            .unwrap_or_default()
    );

    // Spawn background task to perform the actual load
    let session_clone = session.clone();
    let coordinator_clone = coordinator.clone();
    let database_type = request.database_type.clone();
    let load_job_id_clone = load_job_id.clone();
    let batch_size = request.batch_size;
    let create_tables = request.create_tables;
    let connection_config = request.connection_config.clone();
    let lineage_generator = state.lineage_generator.clone();

    let response_message = if request.database_type == DatabaseType::PostgreSQL {
        "Load job queued successfully. Use the load job ID to check status.".to_string()
    } else {
        format!(
            "Load job queued successfully. External {} executor submission started.",
            format!("{:?}", request.database_type).to_lowercase()
        )
    };

    tokio::spawn(async move {
        tracing::info!("Starting background load job: {}", load_job_id_clone);
        if let Err(e) = coordinator_clone.update_load_job_status(
            &load_job_id_clone,
            crate::mapping::multi_source::UnifiedLoadJobStatus::Running,
            None,
            None,
        ) {
            tracing::warn!(
                "Failed to mark load job {} as running: {}",
                load_job_id_clone,
                e
            );
        }

        let execution_lineage_id = format!("unified_load_{}", load_job_id_clone);
        let workflow_started_at = Utc::now();
        if let Some(generator) = &lineage_generator {
            if let Err(e) = generator.record_workflow_start(
                &execution_lineage_id,
                &session_clone.id,
                workflow_started_at,
            ) {
                tracing::warn!(
                    "Failed to record load workflow start lineage for {}: {}",
                    execution_lineage_id,
                    e
                );
            }
        }

        // Actual database loading logic
        let result = async {
            use crate::mapping::loader::postgres_bulk::{
                LoadMode as PgLoadMode, PostgreSQLBulkConfig, PostgreSQLBulkLoader,
            };
            use std::collections::HashMap;

            // Create loader based on database type
            match &database_type {
                DatabaseType::PostgreSQL => {
                    tracing::info!(
                        "Creating PostgreSQL bulk loader for session {}",
                        session_clone.id
                    );

                    // Configure PostgreSQL bulk loader
                    let bulk_config = PostgreSQLBulkConfig {
                        connection_string,
                        load_mode: PgLoadMode::Copy, // Use high-performance COPY
                        batch_size,
                        create_tables,
                        drop_existing: false,
                        use_transactions: true,
                        copy_delimiter: ',',
                        copy_quote: '"',
                        copy_null: String::new(),
                    };

                    // Create loader with database connection
                    let loader = PostgreSQLBulkLoader::new(bulk_config).await.map_err(|e| {
                        anyhow::anyhow!("Failed to create PostgreSQL loader: {}", e)
                    })?;

                    // TODO: Extract source data from CSV files
                    // For now, we use empty source data - this will be populated in Phase 2
                    let source_data: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();

                    tracing::info!("Loading data to PostgreSQL database...");

                    // Load data from unified session
                    let load_result = loader
                        .load_from_session(&session_clone, source_data)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to load data: {}", e))?;

                    tracing::info!(
                        "Load completed: {} rows processed, {} rows inserted",
                        load_result.rows_processed,
                        load_result.rows_inserted
                    );

                    if let Err(e) = coordinator_clone.update_load_job_progress(
                        &load_job_id_clone,
                        crate::mapping::multi_source::UnifiedLoadProgress {
                            total_rows: load_result.rows_processed as usize,
                            rows_processed: load_result.rows_processed as usize,
                            rows_succeeded: load_result.rows_inserted as usize,
                            rows_failed: load_result.rows_skipped as usize,
                            percentage_complete: 100.0,
                        },
                    ) {
                        tracing::warn!(
                            "Failed to update progress for load job {}: {}",
                            load_job_id_clone,
                            e
                        );
                    }

                    Ok::<(), anyhow::Error>(())
                }
                DatabaseType::DB2 | DatabaseType::Oracle | DatabaseType::Databricks => {
                    let backend = database_type_to_execution_backend(&database_type)
                        .ok_or_else(|| anyhow::anyhow!("No execution backend mapping found"))?;
                    let executors = ExecutorRegistry::default_scaffold();

                    let mut executor_options = std::collections::HashMap::new();
                    if let Some(ssl_mode) = connection_config.ssl_mode.clone() {
                        executor_options.insert("ssl_mode".to_string(), ssl_mode);
                    }
                    if let Some(pool_size) = connection_config.pool_size {
                        executor_options.insert("pool_size".to_string(), pool_size.to_string());
                    }

                    let execution_request = ExecutionRequest {
                        run_id: load_job_id_clone.clone(),
                        session_id: session_clone.id.clone(),
                        target_host: connection_config.host.clone(),
                        target_port: connection_config.port,
                        target_database: connection_config.database.clone(),
                        target_username: connection_config.username.clone(),
                        options: executor_options,
                    };
                    let execution_started_at = Utc::now();

                    let outcome = match executors.execute(backend, execution_request.clone()).await {
                        Ok(outcome) => outcome,
                        Err(e) => {
                            record_external_execution_lineage(
                                lineage_generator.as_ref(),
                                &execution_lineage_id,
                                backend,
                                "failed",
                                execution_started_at,
                                Utc::now(),
                            );
                            return Err(anyhow::anyhow!("External executor failed: {}", e));
                        }
                    };

                    let telemetry_event = outcome.to_telemetry_event(&execution_request);
                    emit_external_execution_observability_event(&telemetry_event);
                    if let Err(e) = coordinator_clone.update_load_job_status(
                        &load_job_id_clone,
                        crate::mapping::multi_source::UnifiedLoadJobStatus::Submitted,
                        None,
                        outcome.external_run_id.clone(),
                    ) {
                        tracing::warn!(
                            "Failed to mark load job {} as submitted: {}",
                            load_job_id_clone,
                            e
                        );
                    }
                    record_external_execution_lineage(
                        lineage_generator.as_ref(),
                        &execution_lineage_id,
                        outcome.backend,
                        "submitted",
                        execution_started_at,
                        Utc::now(),
                    );

                    tracing::info!(
                        "External executor submitted for backend={:?} run_id={} external_run_id={:?}",
                        outcome.backend,
                        outcome.run_id,
                        outcome.external_run_id
                    );
                    Ok(())
                }
            }
        }
        .await;

        if let Some(generator) = &lineage_generator {
            if let Err(e) = generator.record_workflow_complete(
                &execution_lineage_id,
                result.is_ok(),
                Utc::now(),
            ) {
                tracing::warn!(
                    "Failed to record load workflow completion lineage for {}: {}",
                    execution_lineage_id,
                    e
                );
            }
        }

        match result {
            Ok(_) => {
                let terminal_status = if matches!(database_type, DatabaseType::PostgreSQL) {
                    crate::mapping::multi_source::UnifiedLoadJobStatus::Completed
                } else {
                    crate::mapping::multi_source::UnifiedLoadJobStatus::Submitted
                };
                if let Err(e) = coordinator_clone.update_load_job_status(
                    &load_job_id_clone,
                    terminal_status,
                    None,
                    None,
                ) {
                    tracing::warn!(
                        "Failed to update terminal status for load job {}: {}",
                        load_job_id_clone,
                        e
                    );
                }
                if matches!(database_type, DatabaseType::PostgreSQL) {
                    if let Err(e) = coordinator_clone.update_unified_session_status(
                        &session_clone.id,
                        crate::mapping::multi_source::UnifiedSessionStatus::Completed,
                    ) {
                        tracing::warn!(
                            "Failed to mark unified session {} completed: {}",
                            session_clone.id,
                            e
                        );
                    }
                }
                tracing::info!(
                    "Background load job {} completed successfully",
                    load_job_id_clone
                );
            }
            Err(e) => {
                if let Err(update_err) = coordinator_clone.update_load_job_status(
                    &load_job_id_clone,
                    crate::mapping::multi_source::UnifiedLoadJobStatus::Failed,
                    Some(e.to_string()),
                    None,
                ) {
                    tracing::warn!(
                        "Failed to mark load job {} failed: {}",
                        load_job_id_clone,
                        update_err
                    );
                }
                if let Err(update_err) = coordinator_clone.update_unified_session_status(
                    &session_clone.id,
                    crate::mapping::multi_source::UnifiedSessionStatus::Failed {
                        error: e.to_string(),
                    },
                ) {
                    tracing::warn!(
                        "Failed to mark unified session {} failed: {}",
                        session_clone.id,
                        update_err
                    );
                }
                tracing::error!("Background load job {} failed: {}", load_job_id_clone, e);
            }
        }
    });

    Ok(Json(LoadToDatabaseResponse {
        session_id: id,
        load_job_id,
        status: LoadJobStatus::Queued,
        message: response_message,
    }))
}

/// Get load job status
///
/// Retrieves the current status and progress of a database load job.
/// Returns detailed metrics including rows processed, success/failure counts,
/// and completion percentage. Use this endpoint to poll for job completion.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/load-jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Load job ID returned from load request")
    ),
    responses(
        (status = 200, description = "Load job status retrieved successfully", body = LoadJobStatusResponse),
        (status = 404, description = "Load job not found", body = ErrorResponse),
        (status = 500, description = "Internal server error - failed to retrieve job status", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn get_load_job_status(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<LoadJobStatusResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::debug!("Getting load job status: {}", job_id);

    let job = coordinator
        .get_load_job(&job_id)
        .map_err(|e| ApiError::internal(format!("Failed to read load job status: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Load job not found: {}", job_id)))?;

    Ok(Json(LoadJobStatusResponse {
        job_id: job.id,
        session_id: job.session_id,
        database_type: Some(job.database_type),
        status: api_load_status_from_domain(job.status),
        progress: LoadProgress {
            total_rows: job.progress.total_rows,
            rows_processed: job.progress.rows_processed,
            rows_succeeded: job.progress.rows_succeeded,
            rows_failed: job.progress.rows_failed,
            percentage_complete: job.progress.percentage_complete,
        },
        started_at: job.started_at,
        completed_at: job.completed_at,
        error_message: job.error_message,
        external_run_id: job.external_run_id,
    }))
}

/// Callback endpoint for external executor job updates.
///
/// External backends (DB2/Oracle/Databricks) call this endpoint to report
/// status transitions after asynchronous submission.
#[utoipa::path(
    post,
    path = "/api/v1/mapping/load-jobs/{job_id}/callback",
    params(
        ("job_id" = String, Path, description = "Load job ID returned from load request")
    ),
    request_body = ExternalLoadJobCallbackRequest,
    responses(
        (status = 200, description = "Load job callback processed", body = ExternalLoadJobCallbackResponse),
        (status = 400, description = "Invalid callback for job state/database type", body = ErrorResponse),
        (status = 404, description = "Load job not found", body = ErrorResponse),
        (status = 500, description = "Internal server error while updating callback status", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn external_load_job_callback(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<String>,
    Json(request): Json<ExternalLoadJobCallbackRequest>,
) -> Result<Json<ExternalLoadJobCallbackResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    let existing = coordinator
        .get_load_job(&job_id)
        .map_err(|e| ApiError::internal(format!("Failed to fetch load job: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Load job not found: {}", job_id)))?;

    if existing.database_type.eq_ignore_ascii_case("postgresql") {
        return Err(ApiError::bad_request(
            "External callback is only valid for DB2/Oracle/Databricks load jobs".to_string(),
        ));
    }

    if let Some(progress) = request.progress.clone() {
        coordinator
            .update_load_job_progress(
                &job_id,
                crate::mapping::multi_source::UnifiedLoadProgress {
                    total_rows: progress.total_rows,
                    rows_processed: progress.rows_processed,
                    rows_succeeded: progress.rows_succeeded,
                    rows_failed: progress.rows_failed,
                    percentage_complete: progress.percentage_complete,
                },
            )
            .map_err(|e| {
                ApiError::internal(format!("Failed to update load job progress: {}", e))
            })?;
    }

    let domain_status = callback_status_to_domain(&request.status);
    let error_message = match request.status {
        ExternalLoadJobCallbackStatus::Failed | ExternalLoadJobCallbackStatus::Cancelled => Some(
            request
                .message
                .clone()
                .unwrap_or_else(|| "External executor reported terminal failure".to_string()),
        ),
        _ => None,
    };
    coordinator
        .update_load_job_status(
            &job_id,
            domain_status.clone(),
            error_message.clone(),
            request.external_run_id.clone(),
        )
        .map_err(|e| ApiError::internal(format!("Failed to update load job status: {}", e)))?;

    let maybe_session_status = match request.status {
        ExternalLoadJobCallbackStatus::Completed => {
            Some(crate::mapping::multi_source::UnifiedSessionStatus::Completed)
        }
        ExternalLoadJobCallbackStatus::Failed | ExternalLoadJobCallbackStatus::Cancelled => {
            Some(crate::mapping::multi_source::UnifiedSessionStatus::Failed {
                error: error_message.unwrap_or_else(|| "External load failed".to_string()),
            })
        }
        ExternalLoadJobCallbackStatus::Running | ExternalLoadJobCallbackStatus::Submitted => None,
    };

    if let Some(session_status) = maybe_session_status {
        coordinator
            .update_unified_session_status(&existing.session_id, session_status)
            .map_err(|e| ApiError::internal(format!("Failed to update unified session: {}", e)))?;
    }

    // Record callback status into lineage if available.
    let callback_time = Utc::now();
    let callback_step_status = match request.status {
        ExternalLoadJobCallbackStatus::Running => "running",
        ExternalLoadJobCallbackStatus::Submitted => "submitted",
        ExternalLoadJobCallbackStatus::Completed => "completed",
        ExternalLoadJobCallbackStatus::Failed => "failed",
        ExternalLoadJobCallbackStatus::Cancelled => "cancelled",
    };
    let execution_lineage_id = format!("unified_load_{}", job_id);
    if let Some(generator) = &state.lineage_generator {
        let step_id = format!("external_executor_callback_{}", callback_step_status);
        let step_type = format!("external_executor:callback:{}", callback_step_status);
        if let Err(e) = generator.record_step_execution(
            &execution_lineage_id,
            &step_id,
            &step_type,
            Vec::new(),
            callback_time,
            callback_time,
        ) {
            tracing::warn!(
                "Failed to record callback lineage step for {}: {}",
                job_id,
                e
            );
        }

        if matches!(
            request.status,
            ExternalLoadJobCallbackStatus::Completed
                | ExternalLoadJobCallbackStatus::Failed
                | ExternalLoadJobCallbackStatus::Cancelled
        ) {
            let success = matches!(request.status, ExternalLoadJobCallbackStatus::Completed);
            if let Err(e) =
                generator.record_workflow_complete(&execution_lineage_id, success, callback_time)
            {
                tracing::warn!(
                    "Failed to record callback workflow completion for {}: {}",
                    job_id,
                    e
                );
            }
        }
    }

    Ok(Json(ExternalLoadJobCallbackResponse {
        job_id: existing.id,
        session_id: existing.session_id,
        status: api_load_status_from_domain(domain_status),
        message: request
            .message
            .unwrap_or_else(|| "External load-job callback processed".to_string()),
    }))
}

// ============================================================================
// Statistics Handlers
// ============================================================================

/// Get global statistics for all unified sessions
///
/// Retrieves aggregated statistics across all unified mapping sessions including
/// session counts by status, total conflicts, field mappings, and database type
/// distribution. Useful for monitoring and reporting.
#[utoipa::path(
    get,
    path = "/api/v1/mapping/unified-sessions/statistics",
    responses(
        (status = 200, description = "Global statistics retrieved successfully", body = GlobalStatisticsResponse),
        (status = 500, description = "Internal server error - failed to retrieve statistics", body = ErrorResponse),
    ),
    tag = "Unified Mapping"
)]
pub async fn get_global_statistics(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<GlobalStatisticsResponse>, ApiError> {
    let coordinator = state.unified_mapping_coordinator.as_ref().ok_or_else(|| {
        ApiError::internal("Unified mapping coordinator not initialized".to_string())
    })?;

    tracing::debug!("Getting global unified mapping statistics");

    // Get all sessions
    let all_sessions = coordinator.list_unified_sessions(None).map_err(|e| {
        tracing::error!("Failed to list unified sessions: {}", e);
        ApiError::internal(format!("Failed to list unified sessions: {}", e))
    })?;

    let total_sessions = all_sessions.len();
    let mut sessions_by_status = std::collections::HashMap::new();
    let mut total_conflicts = 0;
    let mut total_field_mappings = 0;

    for session in &all_sessions {
        *sessions_by_status
            .entry(format!("{:?}", session.status))
            .or_insert(0) += 1;
        total_conflicts += session.conflicts.len();
        total_field_mappings += session.field_mappings.len();
    }

    // Placeholder for database types
    let mut database_types = std::collections::HashMap::new();
    database_types.insert("PostgreSQL".to_string(), 0);
    database_types.insert("DB2".to_string(), 0);
    database_types.insert("Oracle".to_string(), 0);

    Ok(Json(GlobalStatisticsResponse {
        total_sessions,
        sessions_by_status,
        total_conflicts,
        total_field_mappings,
        database_types,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::AuthConfig;
    use crate::api::setup_token::SetupTokenManager;
    use crate::mapping::bindings::BindingService;
    use crate::mapping::bindings::BindingStore;
    use crate::mapping::multi_source::storage::UnifiedMappingStorage;
    use crate::mapping::multi_source::{TargetDatabaseConfig, UnifiedMappingCoordinator};
    use crate::mapping::storage::MappingStorage;
    use crate::mapping::types::{
        FieldApprovalStatus, FieldMappingState, MappingSession, MappingSessionConfig,
        MappingSessionStatus, MappingSessionSummary, SelectedMapping, TableMapping,
    };
    use crate::storage::LineageStorage;
    use async_trait::async_trait;
    use chrono::Utc;
    use graphica_core::catalog::api_types::{
        ColumnDefinition, ConnectionTestResult, DataSourceResponse, DataSourceStatus,
        ListDataSourcesRequest, ListDataSourcesResponse, QueryResult, SchemaDefinition,
        TableDefinition, UpdateDataSourcePatch,
    };
    use graphica_core::catalog::client::{DataSourceCatalog, UsageStatistics};
    use graphica_core::catalog::types::DataSource;
    use graphica_core::errors::GraphicaError;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use uuid::Uuid;

    struct MockSchemaCatalog {
        schema: SchemaDefinition,
    }

    impl MockSchemaCatalog {
        fn new(schema: SchemaDefinition) -> Self {
            Self { schema }
        }
    }

    #[async_trait]
    impl DataSourceCatalog for MockSchemaCatalog {
        async fn register_source(
            &self,
            _source: DataSource,
        ) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::NotFound(
                "register_source not implemented".to_string(),
            ))
        }

        async fn get_source(&self, _id: &str) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::NotFound(
                "get_source not implemented".to_string(),
            ))
        }

        async fn update_source(
            &self,
            _id: &str,
            _updates: UpdateDataSourcePatch,
        ) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::NotFound(
                "update_source not implemented".to_string(),
            ))
        }

        async fn delete_source(&self, _id: &str) -> Result<(), GraphicaError> {
            Err(GraphicaError::NotFound(
                "delete_source not implemented".to_string(),
            ))
        }

        async fn list_sources(
            &self,
            _request: &ListDataSourcesRequest,
        ) -> Result<ListDataSourcesResponse, GraphicaError> {
            Err(GraphicaError::NotFound(
                "list_sources not implemented".to_string(),
            ))
        }

        async fn test_connection(&self, _id: &str) -> Result<ConnectionTestResult, GraphicaError> {
            Err(GraphicaError::NotFound(
                "test_connection not implemented".to_string(),
            ))
        }

        async fn infer_schema(
            &self,
            _id: &str,
            _table_name: Option<&str>,
            _sample_size: usize,
        ) -> Result<SchemaDefinition, GraphicaError> {
            Ok(self.schema.clone())
        }

        async fn execute_query(
            &self,
            _id: &str,
            _query: &str,
            _parameters: HashMap<String, serde_json::Value>,
            _limit: Option<usize>,
        ) -> Result<QueryResult, GraphicaError> {
            Err(GraphicaError::NotFound(
                "execute_query not implemented".to_string(),
            ))
        }

        async fn mark_synced(&self, _id: &str) -> Result<(), GraphicaError> {
            Err(GraphicaError::NotFound(
                "mark_synced not implemented".to_string(),
            ))
        }

        async fn update_status(
            &self,
            _id: &str,
            _status: DataSourceStatus,
            _error_message: Option<String>,
        ) -> Result<(), GraphicaError> {
            Err(GraphicaError::NotFound(
                "update_status not implemented".to_string(),
            ))
        }

        async fn search_sources(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<DataSourceResponse>, GraphicaError> {
            Err(GraphicaError::NotFound(
                "search_sources not implemented".to_string(),
            ))
        }

        async fn get_sources_by_tag(
            &self,
            _tag: &str,
        ) -> Result<Vec<DataSourceResponse>, GraphicaError> {
            Err(GraphicaError::NotFound(
                "get_sources_by_tag not implemented".to_string(),
            ))
        }

        async fn get_usage_stats(&self, _id: &str) -> Result<UsageStatistics, GraphicaError> {
            Err(GraphicaError::NotFound(
                "get_usage_stats not implemented".to_string(),
            ))
        }

        async fn get_source_by_title(
            &self,
            _title: &str,
        ) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::NotFound(
                "get_source_by_title not implemented".to_string(),
            ))
        }
    }

    fn schema_for_plan_tests() -> SchemaDefinition {
        SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "orders".to_string(),
                columns: vec![
                    ColumnDefinition {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        primary_key: true,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                    ColumnDefinition {
                        name: "total".to_string(),
                        data_type: "DECIMAL".to_string(),
                        nullable: false,
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                ],
                estimated_rows: Some(10),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        }
    }

    fn seed_mapping_session() -> MappingSession {
        MappingSession {
            session_id: "seed_session_001".to_string(),
            source_id: "source-a".to_string(),
            status: MappingSessionStatus::Active,
            tables: vec![TableMapping {
                table_name: "orders".to_string(),
                field_mappings: vec![FieldMappingState {
                    field_id: "seed_field_order_total".to_string(),
                    field_name: "order_total".to_string(),
                    data_type: "DECIMAL".to_string(),
                    sample_values: vec!["42.00".to_string()],
                    candidates: vec![],
                    selected_mapping: Some(SelectedMapping {
                        ontology_term_uri: "http://example.org/orderTotal".to_string(),
                        confidence: 0.98,
                        was_top_candidate: true,
                        transformation: None,
                    }),
                    approval_status: FieldApprovalStatus::Approved,
                    reviewed_by: Some("tester".to_string()),
                    reviewed_at: Some(Utc::now().timestamp()),
                    notes: None,
                }],
                metadata: None,
            }],
            created_by: "tester".to_string(),
            created_at: Utc::now().timestamp(),
            reviewed_by: Some("tester".to_string()),
            reviewed_at: Some(Utc::now().timestamp()),
            applied_at: None,
            config: MappingSessionConfig::default(),
            summary: MappingSessionSummary::default(),
        }
    }

    fn create_plan_test_state() -> Arc<ApiState> {
        let mut state = (*create_test_api_state()).clone();

        let binding_store_path = format!("/tmp/graphica-binding-test-{}", Uuid::new_v4());
        std::fs::create_dir_all(&binding_store_path).unwrap();
        let binding_store = Arc::new(BindingStore::new(&binding_store_path).unwrap());
        state.binding_service = Some(Arc::new(BindingService::new(binding_store)));
        state.datasource_catalog = Some(Arc::new(MockSchemaCatalog::new(schema_for_plan_tests())));

        Arc::new(state)
    }

    fn create_test_api_state() -> Arc<ApiState> {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        let unified_dir = temp_dir.path().join("unified");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&unified_dir).unwrap();

        let source_storage = Arc::new(MappingStorage::new(source_dir.to_str().unwrap()).unwrap());
        source_storage
            .store_session(&seed_mapping_session())
            .unwrap();
        let unified_storage =
            Arc::new(UnifiedMappingStorage::new(unified_dir.to_str().unwrap()).unwrap());
        let coordinator = Arc::new(UnifiedMappingCoordinator::new(
            source_storage,
            unified_storage,
        ));

        // Create minimal lineage storage
        let lineage_path = temp_dir.path().join("lineage");
        let rocks_path = lineage_path.join("rocks").to_str().unwrap().to_string();
        let parquet_path = lineage_path.join("parquet").to_str().unwrap().to_string();
        let cold_path = lineage_path.join("cold").to_str().unwrap().to_string();

        let lineage_storage = Arc::new(
            LineageStorage::new(&rocks_path, &parquet_path, &cold_path, "localhost:9092").unwrap(),
        );

        // Test secret with enough entropy
        let test_secret: [u8; 32] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ];
        let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());
        let setup_token_manager = Arc::new(SetupTokenManager::new());

        Arc::new(ApiState {
            lineage_storage,
            governance_brain: None,
            rdf_store: None,
            shard_registry: None,
            query_executor: None,
            workflow_engine: None,
            model_registry: None,
            model_cache: None,
            rule_executor: None,
            transformer_registry: None,
            circuit_breakers: None,
            auth_config,
            user_service: None,
            setup_token_manager,
            audit_logger: None,
            datasource_catalog: None,
            datasource_catalog_impl: None,
            import_job_manager: Arc::new(crate::api::import_jobs::ImportJobManager::new()),
            persisted_ontology_registry: None,
            ontology_registry: None,
            rdf_storage: None,
            connector_registry: None,
            resolved_entity_cache: None,
            metrics_registry: None,
            mapping_engine: None,
            secret_store_registry: None,
            loader_job_manager: None,
            schedule_store: None,
            workflow_store: None,
            execution_store: None,
            unified_mapping_coordinator: Some(coordinator),
            binding_service: None,
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

    #[tokio::test]
    async fn test_list_unified_sessions_empty() {
        let state = create_test_api_state();
        let query = ListUnifiedSessionsQuery {
            status: None,
            created_by: None,
            offset: 0,
            limit: 50,
        };

        let result = list_unified_sessions(State(state.clone()), Query(query)).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.sessions.len(), 0);
        assert_eq!(response.total_count, 0);
    }

    #[tokio::test]
    async fn test_get_unified_session_not_found() {
        let state = create_test_api_state();

        let result =
            get_unified_session(State(state.clone()), Path("nonexistent".to_string())).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_unified_session_not_found() {
        let state = create_test_api_state();

        let result =
            delete_unified_session(State(state.clone()), Path("nonexistent".to_string())).await;

        // Should return error for nonexistent session
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_global_statistics() {
        let state = create_test_api_state();

        let result = get_global_statistics(State(state.clone())).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.total_sessions, 0);
    }

    #[tokio::test]
    async fn test_binding_history_tracks_versions() {
        let state = create_plan_test_state();
        let request = UpsertOntologyBindingsRequest {
            source_id: "source-a".to_string(),
            entity_uri: "http://example.org/Order".to_string(),
            updated_by: "tester".to_string(),
            bindings: vec![OntologyBindingInput {
                ontology_uri: "http://example.org/orderTotal".to_string(),
                table: "orders".to_string(),
                column: "total".to_string(),
                sql_dialect: SqlDialect::Postgresql,
                confidence: 0.91,
                provenance: None,
            }],
        };

        let first = upsert_ontology_bindings(State(state.clone()), Json(request.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(first.updated.len(), 1);
        assert_eq!(first.updated[0].version, 1);

        let mut second_req = request;
        second_req.bindings[0].table = "orders_v2".to_string();
        second_req.bindings[0].column = "total_amount".to_string();
        let second = upsert_ontology_bindings(State(state.clone()), Json(second_req))
            .await
            .unwrap()
            .0;
        assert_eq!(second.updated[0].version, 2);

        let history = binding_history(
            State(state.clone()),
            Query(BindingHistoryQuery {
                source_id: "source-a".to_string(),
                entity_uri: "http://example.org/Order".to_string(),
                ontology_uri: "http://example.org/orderTotal".to_string(),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(history.history.len(), 2);
        assert_eq!(history.history[0].version, 1);
        assert_eq!(history.history[1].version, 2);
    }

    #[tokio::test]
    async fn test_binding_coverage_diff_reports_unmapped_and_covered() {
        let state = create_plan_test_state();

        upsert_ontology_bindings(
            State(state.clone()),
            Json(UpsertOntologyBindingsRequest {
                source_id: "source-a".to_string(),
                entity_uri: "http://example.org/Order".to_string(),
                updated_by: "tester".to_string(),
                bindings: vec![OntologyBindingInput {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    table: "orders".to_string(),
                    column: "total".to_string(),
                    sql_dialect: SqlDialect::Postgresql,
                    confidence: 0.95,
                    provenance: None,
                }],
            }),
        )
        .await
        .unwrap();

        let coverage = binding_coverage(
            State(state),
            Json(BindingCoverageRequest {
                source_id: "source-a".to_string(),
                entity_uri: "http://example.org/Order".to_string(),
                required_properties: vec![
                    "http://example.org/orderTotal".to_string(),
                    "http://example.org/orderDate".to_string(),
                ],
                table_name: None,
                sample_size: 1000,
                validate_schema: true,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(
            coverage.covered_properties,
            vec!["http://example.org/orderTotal".to_string()]
        );
        assert_eq!(
            coverage.unmapped_properties,
            vec!["http://example.org/orderDate".to_string()]
        );
        assert_eq!(
            coverage.missing_properties,
            vec!["http://example.org/orderDate".to_string()]
        );
        assert_eq!(coverage.coverage_ratio, 0.5);
    }

    #[tokio::test]
    async fn test_plan_goal_sql_with_stored_bindings_and_parameters() {
        let state = create_plan_test_state();

        // Seed stored binding used by planning.
        upsert_ontology_bindings(
            State(state.clone()),
            Json(UpsertOntologyBindingsRequest {
                source_id: "source-a".to_string(),
                entity_uri: "http://example.org/Order".to_string(),
                updated_by: "tester".to_string(),
                bindings: vec![OntologyBindingInput {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    table: "orders".to_string(),
                    column: "total".to_string(),
                    sql_dialect: SqlDialect::Postgresql,
                    confidence: 0.95,
                    provenance: None,
                }],
            }),
        )
        .await
        .unwrap();

        let response = plan_goal_sql(
            State(state),
            Json(PlanGoalSqlRequest {
                source_id: "source-a".to_string(),
                table_name: None,
                sample_size: 1000,
                entity_uri: "http://example.org/Order".to_string(),
                required_properties: vec!["http://example.org/orderTotal".to_string()],
                filters: vec![GoalSqlFilter {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    value: "42".to_string(),
                }],
                binding_strategy: GoalBindingStrategy::Stored,
                sql_dialect: SqlDialect::Oracle,
                include_explain_plan: true,
                bindings: vec![],
                limit: Some(5),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response.binding_strategy, GoalBindingStrategy::Stored);
        assert_eq!(response.sql_dialect, SqlDialect::Oracle);
        assert!(response.sql.contains("FETCH FIRST 5 ROWS ONLY"));
        assert!(response.sql.contains(":p1"));
        assert!(response.explain_sql.is_some());
        assert!(response.explain_metadata.is_some());
        assert_eq!(response.parameters.len(), 1);
        assert_eq!(response.parameters[0].value, "42");
        assert_eq!(response.parameters[0].data_type.as_deref(), Some("DECIMAL"));
    }

    #[tokio::test]
    async fn test_plan_goal_sql_returns_db2_explain_metadata() {
        let state = create_plan_test_state();

        let response = plan_goal_sql(
            State(state),
            Json(PlanGoalSqlRequest {
                source_id: "source-a".to_string(),
                table_name: None,
                sample_size: 1000,
                entity_uri: "http://example.org/Order".to_string(),
                required_properties: vec!["http://example.org/orderTotal".to_string()],
                filters: vec![],
                binding_strategy: GoalBindingStrategy::Inline,
                sql_dialect: SqlDialect::Db2,
                include_explain_plan: true,
                bindings: vec![GoalSqlBinding {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    table: "orders".to_string(),
                    column: "total".to_string(),
                    confidence: 0.99,
                }],
                limit: Some(3),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(response.explain_sql.is_some());
        let metadata = response.explain_metadata.expect("metadata");
        assert_eq!(metadata.mode, "db2_explain_tables");
        assert!(metadata.follow_up_query.is_some());
    }

    #[tokio::test]
    async fn test_plan_goal_sql_returns_saphana_explain_metadata() {
        let state = create_plan_test_state();

        let response = plan_goal_sql(
            State(state),
            Json(PlanGoalSqlRequest {
                source_id: "source-a".to_string(),
                table_name: None,
                sample_size: 1000,
                entity_uri: "http://example.org/Order".to_string(),
                required_properties: vec!["http://example.org/orderTotal".to_string()],
                filters: vec![GoalSqlFilter {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    value: "99.9".to_string(),
                }],
                binding_strategy: GoalBindingStrategy::Inline,
                sql_dialect: SqlDialect::Saphana,
                include_explain_plan: true,
                bindings: vec![GoalSqlBinding {
                    ontology_uri: "http://example.org/orderTotal".to_string(),
                    table: "orders".to_string(),
                    column: "total".to_string(),
                    confidence: 0.99,
                }],
                limit: Some(3),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(response.explain_sql.is_some());
        assert!(response.sql.contains('?'));
        let metadata = response.explain_metadata.expect("metadata");
        assert_eq!(metadata.mode, "statement_name_plan_table");
        assert!(metadata
            .follow_up_query
            .unwrap_or_default()
            .contains("EXPLAIN_PLAN_TABLE"));
    }

    #[tokio::test]
    async fn test_load_to_database_databricks_external_submission_path() {
        let state = create_test_api_state();

        let create_response = create_unified_session(
            State(state.clone()),
            Json(CreateUnifiedSessionRequest {
                source_session_ids: vec!["seed_session_001".to_string()],
                target_database: TargetDatabaseConfig {
                    datasource_id: "target-databricks".to_string(),
                    schema: "main".to_string(),
                    tables: HashMap::new(),
                },
                created_by: "tester".to_string(),
            }),
        )
        .await
        .expect("unified session create")
        .0;

        assert_eq!(
            create_response.status,
            crate::mapping::multi_source::UnifiedSessionStatus::ReadyToLoad
        );

        let load_response = load_to_database(
            State(state.clone()),
            Path(create_response.id),
            Json(LoadToDatabaseRequest {
                database_type: DatabaseType::Databricks,
                connection_config: DatabaseConnectionConfig {
                    host: "https://adb-123.azuredatabricks.net".to_string(),
                    port: 443,
                    database: "lakehouse".to_string(),
                    username: "svc_graphica".to_string(),
                    password: "test_token".to_string(),
                    ssl_mode: Some("require".to_string()),
                    pool_size: Some(4),
                },
                batch_size: 500,
                create_tables: true,
                validate_data: true,
            }),
        )
        .await
        .expect("load queued")
        .0;

        assert_eq!(load_response.status, LoadJobStatus::Queued);
        assert!(load_response.load_job_id.starts_with("loadjob_"));
        assert!(load_response.message.to_lowercase().contains("external"));
        assert!(load_response.message.to_lowercase().contains("databricks"));

        // Background task runs asynchronously; poll until submission is persisted.
        let mut latest_status = LoadJobStatus::Queued;
        let mut latest_external_run_id = None;
        for _ in 0..20 {
            let status_response = get_load_job_status(
                State(state.clone()),
                Path(load_response.load_job_id.clone()),
            )
            .await
            .expect("load status")
            .0;

            latest_external_run_id = status_response.external_run_id.clone();
            latest_status = status_response.status.clone();

            if matches!(
                latest_status,
                LoadJobStatus::Submitted | LoadJobStatus::Completed | LoadJobStatus::Failed
            ) {
                assert_eq!(status_response.database_type.as_deref(), Some("databricks"));
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert_eq!(latest_status, LoadJobStatus::Submitted);
        assert!(latest_external_run_id.is_some());
    }

    #[tokio::test]
    async fn test_get_load_job_status_not_found() {
        let state = create_test_api_state();

        let result = get_load_job_status(State(state), Path("missing_job".to_string())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_external_load_job_callback_completes_external_job() {
        let state = create_test_api_state();

        let create_response = create_unified_session(
            State(state.clone()),
            Json(CreateUnifiedSessionRequest {
                source_session_ids: vec!["seed_session_001".to_string()],
                target_database: TargetDatabaseConfig {
                    datasource_id: "target-databricks".to_string(),
                    schema: "main".to_string(),
                    tables: HashMap::new(),
                },
                created_by: "tester".to_string(),
            }),
        )
        .await
        .expect("unified session create")
        .0;

        let load_response = load_to_database(
            State(state.clone()),
            Path(create_response.id.clone()),
            Json(LoadToDatabaseRequest {
                database_type: DatabaseType::Databricks,
                connection_config: DatabaseConnectionConfig {
                    host: "https://adb-123.azuredatabricks.net".to_string(),
                    port: 443,
                    database: "lakehouse".to_string(),
                    username: "svc_graphica".to_string(),
                    password: "test_token".to_string(),
                    ssl_mode: Some("require".to_string()),
                    pool_size: Some(4),
                },
                batch_size: 500,
                create_tables: true,
                validate_data: true,
            }),
        )
        .await
        .expect("load queued")
        .0;

        // Wait until background submit path persists Submitted.
        let mut ready_for_callback = false;
        for _ in 0..20 {
            let status_response = get_load_job_status(
                State(state.clone()),
                Path(load_response.load_job_id.clone()),
            )
            .await
            .expect("status")
            .0;
            if status_response.status == LoadJobStatus::Submitted {
                ready_for_callback = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(ready_for_callback);

        let callback_response = external_load_job_callback(
            State(state.clone()),
            Path(load_response.load_job_id.clone()),
            Json(ExternalLoadJobCallbackRequest {
                status: ExternalLoadJobCallbackStatus::Completed,
                external_run_id: Some("stmt_456".to_string()),
                message: Some("Statement finished successfully".to_string()),
                progress: Some(LoadProgress {
                    total_rows: 1000,
                    rows_processed: 1000,
                    rows_succeeded: 1000,
                    rows_failed: 0,
                    percentage_complete: 100.0,
                }),
            }),
        )
        .await
        .expect("callback")
        .0;

        assert_eq!(callback_response.status, LoadJobStatus::Completed);

        let status_after = get_load_job_status(
            State(state.clone()),
            Path(load_response.load_job_id.clone()),
        )
        .await
        .expect("status after callback")
        .0;
        assert_eq!(status_after.status, LoadJobStatus::Completed);
        assert_eq!(status_after.external_run_id.as_deref(), Some("stmt_456"));
        assert_eq!(status_after.progress.rows_succeeded, 1000);

        let session_after = get_unified_session(State(state), Path(create_response.id))
            .await
            .expect("session")
            .0;
        assert_eq!(
            session_after.status,
            crate::mapping::multi_source::UnifiedSessionStatus::Completed
        );
    }

    #[tokio::test]
    async fn test_external_load_job_callback_rejects_postgresql_jobs() {
        let state = create_test_api_state();

        let create_response = create_unified_session(
            State(state.clone()),
            Json(CreateUnifiedSessionRequest {
                source_session_ids: vec!["seed_session_001".to_string()],
                target_database: TargetDatabaseConfig {
                    datasource_id: "target-postgres".to_string(),
                    schema: "public".to_string(),
                    tables: HashMap::new(),
                },
                created_by: "tester".to_string(),
            }),
        )
        .await
        .expect("create unified session")
        .0;

        let coordinator = state
            .unified_mapping_coordinator
            .as_ref()
            .expect("coordinator");
        let job = coordinator
            .create_load_job(&create_response.id, "postgresql")
            .expect("create load job");

        let result = external_load_job_callback(
            State(state),
            Path(job.id),
            Json(ExternalLoadJobCallbackRequest {
                status: ExternalLoadJobCallbackStatus::Completed,
                external_run_id: Some("ignored".to_string()),
                message: None,
                progress: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }
}
