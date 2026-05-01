//! # REST API Implementation
//!
//! Versioned REST API using Axum with real storage integration.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

// Import ApiState from parent api module (defined in api/mod.rs)
use super::ApiState;

// Import DTOs from refactored modules
use super::dto::{
    // Common DTOs
    ApiError,
    // Audit DTOs
    AuditQueryRequest,
    // Connector DTOs
    ConfigFieldResponse,
    ConnectorCapabilitiesResponse,
    ConnectorListResponse,
    ConnectorMetadataResponse,
    ConnectorOperationResponse,
    ConnectorStatisticsResponse,
    CredentialFieldResponse,
};

// Note: Validators are imported locally where needed

// Import handlers from refactored modules
use super::handlers::{
    // Temporal handlers
    analyze_temporal_chains,
    // Fusion handlers
    approve_fusion_candidate,
    // Dataset import handlers
    batch_import_datasources,
    bulk_export_manual_mappings,
    bulk_import_manual_mappings,
    // Rule handlers
    clear_rule_cache_handler,
    clear_temporal_cache,
    compact_temporal_indexes,
    // Manual mapping handlers
    create_manual_mapping,
    // Quality handlers
    create_rule,
    create_temporal_checkpoint,
    delete_manual_mapping,
    // Model registry handlers
    delete_model_handler as delete_registry_model_handler,
    execute_rule_handler,
    // Entities handlers
    get_attribute_timeseries,
    get_dataset,
    get_entity,
    get_entity_attributes,
    get_entity_lineage,
    get_import_status,
    get_manual_mapping,
    // Models handlers
    get_model,
    get_model_handler as get_registry_model_handler,
    // Lineage handlers
    get_model_lineage_rdf,
    get_model_training_data_as_of,
    // SPARQL handlers (moved to governance module)
    get_rdf_auto_save_stats,
    get_rule,
    get_scorecard,
    get_temporal_statistics,
    get_temporal_summary,
    // WAL handlers
    get_wal_operations,
    get_wal_status,
    // Health handlers
    health_check,
    import_dataset,
    import_from_datasource,
    list_datasets,
    list_entities,
    list_fusion_candidates,
    list_imports,
    list_models_handler as list_registry_models_handler,
    list_violations,
    liveness_check,
    load_rule_handler,
    metrics_endpoint,
    propose_fusion_candidates,
    query_lineage,
    readiness_check,
    record_predictions,
    register_model,
    register_model_handler as register_registry_model_handler,
    reject_fusion_candidate,
    resolve_entity_fusion,
    reverse_entity_fusion,
    simulate_change,
    storage_health_check,
    suggest_manual_mappings,
    trigger_wal_replay,
    unload_rule_handler,
    update_manual_mapping,
    update_model_handler as update_registry_model_handler,
    write_lineage_events,
};

// Note: Workflow handlers are now in separate router modules:
// - /api/workflow/ (new refactored API)
// - /workflows/api/ (modern + legacy routers)

/// Build the REST API router
pub fn build_router(state: ApiState) -> Router {
    let auth_config = state.auth_config.clone();
    let auth_config_for_admin = state.auth_config.clone();

    // Clone metrics registry for middleware
    let metrics_registry = state.metrics_registry.clone();

    Router::new()
        // Public routes (no auth required)
        .route("/health", get(health_check))
        .route("/health/live", get(liveness_check))
        .route("/health/ready", get(readiness_check))
        .route("/health/storage", get(storage_health_check))
        .route("/ready", get(readiness_check))
        .route("/openapi.yaml", get(get_openapi_spec))
        // Note: API documentation uses modular Swagger UIs per module (see api/openapi.rs for URLs)
        // Raft coordination endpoints (no auth - for coordinator-to-coordinator communication)
        .merge(raft_coordination_routes(state.clone()))
        // Metrics endpoint (requires auth)
        .merge(
            Router::new()
                .route("/metrics", get(metrics_endpoint))
                .layer(axum::middleware::from_fn_with_state(
                    auth_config.clone(),
                    crate::api::auth::auth_middleware,
                )),
        )
        // Auth endpoints (public login, admin-only token generation)
        .route("/auth/login", post(login_handler))
        .route("/auth/setup", post(setup_admin_handler)) // Initial admin setup
        .route(
            "/auth/token",
            post(generate_token_handler).layer(axum::middleware::from_fn_with_state(
                auth_config_for_admin.clone(),
                standalone_admin_middleware,
            )),
        )
        .route(
            "/auth/users",
            post(create_user_handler).layer(axum::middleware::from_fn_with_state(
                auth_config_for_admin,
                standalone_admin_middleware,
            )),
        )
        // Protected routes (auth required)
        .nest(
            "/api/v1",
            v1_routes().layer(axum::middleware::from_fn_with_state(
                auth_config,
                crate::api::auth::auth_middleware,
            )),
        )
        // Static file serving for frontend (SPA)
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(ServeDir::new("static").append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        // Observability middleware layers
        .layer(axum::middleware::from_fn(
            crate::observability::middleware::request_id_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            metrics_registry,
            crate::observability::middleware::metrics_middleware,
        ))
        .with_state(Arc::new(state))
}

/// V1 API routes with role-based access control
fn v1_routes() -> Router<Arc<ApiState>> {
    Router::new()
        // Read-only routes (all authenticated users)
        .merge(read_only_routes())
        // Write routes (Admin, Operator, Service)
        .merge(write_routes())
        // Admin-only routes
        .merge(admin_only_routes())
        // Workflow routes (modularized)
        .merge(crate::api::workflow::create_workflow_router())
        // Data source catalog routes
        .merge(crate::api::datasources::create_router())
        // Custom ontology management routes
        .merge(crate::api::ontology::create_router())
        // Governance API routes (SPARQL and RDF store management)
        .merge(crate::api::governance::create_router())
        // Systems-of-Systems validation API (cross-system compatibility and integration)
        .merge(crate::api::sos_validation::create_router())
        // Field lineage and golden record routes
        .merge(crate::api::field_lineage::create_router())
        // ETL loader routes
        .merge(crate::api::loader::create_loader_router())
        // Lineage query routes (Sprint 1.9: W3C PROV query API)
        .merge(crate::api::lineage::create_lineage_router())
        // Source profiling routes (Stage 1: DCAT/VoID RDF metadata)
        .merge(crate::api::profiling::create_router())
        // R2RML mapping routes (Stage 2: Semantic mapping)
        .merge(crate::api::r2rml::create_router())
        // DDL generation routes (Stage 3: SHACL to SQL)
        .merge(crate::api::ddl::router())
        // Ontology-driven DDL routes (GAP-002 Phase 3: Semantic DDL generation)
        .merge(crate::api::ontology_ddl::create_router())
        // Unified mapping routes (CSV-to-DB consolidation)
        .merge(crate::api::unified_mapping::create_unified_mapping_router())
        // File Library routes (Enterprise CSV/TSV/Excel file management)
        .merge(crate::api::file_library::create_router())
        // Schema API routes (Phase 2: Cross-source mapping and type conversion)
        .merge(crate::api::schema_api::create_router())
        // GDPR compliance routes (Article 17: Right to Erasure)
        .merge(crate::api::gdpr::create_router())
        // Migration evidence graph routes (IBM RISE / SAP explainability)
        .merge(crate::api::migration_evidence::create_router())
}

/// Read-only routes - accessible by all authenticated users (Viewer, Operator, Admin, Service)
fn read_only_routes() -> Router<Arc<ApiState>> {
    Router::new()
        // NOTE: Lineage routes are now registered via lineage::create_lineage_router()
        // to avoid conflicts with the modular lineage API
        // Quality queries (read-only)
        .route("/quality/dataset/:dataset/scorecard", get(get_scorecard))
        .route("/quality/violations", get(list_violations))
        .route("/quality/rules/:id", get(get_rule))
        // Model queries (read-only)
        .route("/models/:id", get(get_model))
        .route(
            "/models/:model_id/training-data/as-of",
            get(get_model_training_data_as_of),
        )
        // Dataset queries (read-only)
        .route("/datasets", get(list_datasets))
        .route("/datasets/:id", get(get_dataset))
        // Dataset import status (read-only)
        .route("/datasets/imports", get(list_imports))
        .route("/datasets/imports/:import_id", get(get_import_status))
        // Entity queries (read-only)
        .route("/entities", get(list_entities))
        .route("/entities/:id", get(get_entity))
        .route("/entities/:id/attributes", get(get_entity_attributes))
        .route("/entities/:id/lineage", get(get_entity_lineage))
        .route(
            "/entities/:id/attributes/:name/timeseries",
            get(get_attribute_timeseries),
        )
        // Governance/RDF queries (read-only) - now handled by governance module
        .route("/governance/model/:id/lineage", get(get_model_lineage_rdf))
        // Fusion queries (read-only)
        .route("/fusion/candidates", get(list_fusion_candidates))
        // Temporal stats (read-only)
        .route("/admin/temporal/analyze", get(analyze_temporal_chains))
        .route("/admin/temporal/stats", get(get_temporal_statistics))
        .route("/admin/temporal/summary", get(get_temporal_summary))
        // Connector registry (read-only)
        .route("/connectors", get(list_connectors_handler))
        .route(
            "/connectors/statistics",
            get(get_connector_statistics_handler),
        )
        .route("/connectors/:id", get(get_connector_handler))
        // WAL monitoring (read-only)
        .route("/admin/wal/status", get(get_wal_status))
        .route("/admin/wal/operations", get(get_wal_operations))
        // RDF auto-save monitoring (read-only)
        .route("/admin/rdf/stats", get(get_rdf_auto_save_stats))
        // Cluster admin queries (read-only)
        .route(
            "/cluster/topology",
            get(crate::api::cluster_admin::get_cluster_topology),
        )
        .route(
            "/cluster/stats",
            get(crate::api::cluster_admin::get_cluster_stats),
        )
        .route(
            "/cluster/health",
            get(crate::api::cluster_admin::get_cluster_health),
        )
        .route(
            "/cluster/config",
            get(crate::api::cluster_admin::get_cluster_config),
        )
        .route(
            "/cluster/shards/:shard_id",
            get(crate::api::cluster_admin::get_shard_detail),
        )
        .route(
            "/cluster/replication/config",
            get(crate::api::cluster_admin::get_replication_config),
        )
        .route(
            "/cluster/metadata",
            get(crate::api::cluster_admin::get_cluster_metadata),
        )
        // Workflow queries - handled by workflow module
        // Model registry queries (read-only)
        .route("/orchestration/models", get(list_registry_models_handler))
        .route(
            "/orchestration/models/:model_id",
            get(get_registry_model_handler),
        )
        // Monitoring stats (read-only)
        .route("/orchestration/cache/stats", get(get_cache_stats_handler))
        .route(
            "/orchestration/circuit-breaker/:model_id",
            get(get_circuit_breaker_status_handler),
        )
        // Field mapping queries (read-only)
        .route(
            "/mapping/health",
            get(crate::api::handlers::mapping_health_check),
        )
        .route(
            "/mapping/fields/:field_id/candidates",
            get(crate::api::handlers::get_candidates),
        )
        .route(
            "/mapping/sessions/:session_id",
            get(crate::api::handlers::get_session),
        )
        // Manual mapping queries (read-only)
        .route("/mapping/manual/:id", get(get_manual_mapping))
        .route("/mapping/manual/export", get(bulk_export_manual_mappings))
}

/// Write routes - accessible by Admin, Operator, and Service roles
fn write_routes() -> Router<Arc<ApiState>> {
    Router::new()
        // Lineage operations (write)
        .route("/lineage/query", post(query_lineage))
        .route("/lineage/events", post(write_lineage_events))
        .route("/lineage/simulate", post(simulate_change))
        // Quality operations (write)
        .route("/quality/rules", post(create_rule))
        // Model operations (write)
        .route("/models", post(register_model))
        .route("/models/:model_id/predictions", post(record_predictions))
        // Dataset import operations (write)
        .route("/datasets/import", post(import_dataset))
        .route("/datasets/import-datasource", post(import_from_datasource))
        .route("/datasets/import-batch", post(batch_import_datasources))
        // Governance operations (write) - now handled by governance module
        // Entity fusion operations (write)
        .route("/fusion/propose", post(propose_fusion_candidates))
        .route(
            "/fusion/candidates/:id/approve",
            post(approve_fusion_candidate),
        )
        .route(
            "/fusion/candidates/:id/reject",
            post(reject_fusion_candidate),
        )
        .route("/fusion/resolve", post(resolve_entity_fusion))
        .route("/fusion/:id/reverse", post(reverse_entity_fusion))
        // Workflow operations - handled by workflow module
        // Model registry operations (write)
        .route(
            "/orchestration/models",
            post(register_registry_model_handler),
        )
        // Rule operations (write)
        .route("/orchestration/rules/:rule_id", post(load_rule_handler))
        .route(
            "/orchestration/rules/:rule_id/execute",
            post(execute_rule_handler),
        )
        // Connector registry operations (write)
        .route("/connectors/:id/enable", post(enable_connector_handler))
        .route("/connectors/:id/disable", post(disable_connector_handler))
        // Field mapping operations (write)
        .route(
            "/mapping/analyze",
            post(crate::api::handlers::analyze_schema),
        )
        .route(
            "/mapping/feedback",
            post(crate::api::handlers::record_feedback),
        )
        // Mapping session workflow (write)
        .route(
            "/datasources/:source_id/analyze-for-mapping",
            post(crate::api::handlers::analyze_for_mapping),
        )
        .route(
            "/datasets/:dataset_id/analyze-for-mapping",
            post(crate::api::handlers::analyze_dataset_for_mapping),
        )
        .route(
            "/mapping/sessions/:session_id/review",
            post(crate::api::handlers::review_mappings),
        )
        .route(
            "/mapping/sessions/:session_id/apply",
            post(crate::api::handlers::apply_mappings),
        )
        .route(
            "/mapping/sessions/:session_id/import",
            post(crate::api::handlers::import_from_mappings),
        )
        // Manual mapping operations (write)
        .route("/mapping/manual", post(create_manual_mapping))
        .route(
            "/mapping/manual/:id",
            axum::routing::put(update_manual_mapping),
        )
        .route(
            "/mapping/manual/:id",
            axum::routing::delete(delete_manual_mapping),
        )
        .route("/mapping/manual/suggest", post(suggest_manual_mappings))
        .route("/mapping/manual/import", post(bulk_import_manual_mappings))
        // Secret management operations (write - requires additional admin middleware)
        // Note: Secret routes are added separately to admin_only_routes for stricter access control
        // Apply write middleware to all routes in this group
        .route_layer(axum::middleware::from_fn(require_write_middleware))
}

/// Admin-only routes - accessible only by Admin role
fn admin_only_routes() -> Router<Arc<ApiState>> {
    Router::new()
        // Datasource RDF sync (admin backfill utility)
        .route(
            "/admin/datasources/sync-to-rdf",
            post(crate::api::datasources::handlers::sync_datasources_to_rdf),
        )
        // Workflow management - handled by workflow module
        // Model management (destructive)
        .route(
            "/orchestration/models/:model_id",
            axum::routing::put(update_registry_model_handler),
        )
        .route(
            "/orchestration/models/:model_id",
            axum::routing::delete(delete_registry_model_handler),
        )
        // Rule management (destructive)
        .route(
            "/orchestration/rules/:rule_id",
            axum::routing::delete(unload_rule_handler),
        )
        .route(
            "/orchestration/rules/cache/clear",
            post(clear_rule_cache_handler),
        )
        // Cache management (admin)
        .route(
            "/orchestration/cache/clear",
            post(clear_model_cache_handler),
        )
        // RDF persistence management (admin) - now handled by governance module
        // Circuit breaker management (admin)
        .route(
            "/orchestration/circuit-breaker/:model_id/reset",
            post(reset_circuit_breaker_handler),
        )
        // Temporal admin operations
        .route(
            "/admin/temporal/checkpoint",
            post(create_temporal_checkpoint),
        )
        .route("/admin/temporal/compact", post(compact_temporal_indexes))
        .route("/admin/temporal/cache/clear", post(clear_temporal_cache))
        // WAL admin operations
        .route("/admin/wal/replay", post(trigger_wal_replay))
        // Audit log operations (admin-only - security/compliance)
        .route("/admin/audit/query", post(query_audit_logs_handler))
        .route("/admin/audit/export", post(export_audit_logs_handler))
        // Cluster admin operations (admin-only - scaling operations)
        .route(
            "/cluster/scale-out",
            post(crate::api::cluster_admin::scale_out_cluster),
        )
        // Secret management API (admin-only - sensitive credential operations)
        .merge(crate::api::secrets::create_router())
        // Raft coordination monitoring (admin-only)
        .route("/kafka/raft/state", get(raft_state_handler))
        .route("/kafka/raft/log", get(raft_log_handler))
        // Apply admin middleware to all routes in this group
        .route_layer(axum::middleware::from_fn(require_admin_middleware))
}

/// Raft coordination routes for distributed replay
/// These routes are used by coordinators to communicate with each other
fn raft_coordination_routes(state: ApiState) -> Router<Arc<ApiState>> {
    // Only create routes if replay coordinator is configured
    if state.replay_coordinator.is_some() {
        Router::new()
            .route("/kafka/raft/vote", post(raft_vote_handler))
            .route("/kafka/raft/heartbeat", post(raft_heartbeat_handler))
    } else {
        // Return empty router if Raft is not configured
        Router::new()
    }
}

/// Handler for Raft vote requests (wraps kafka_raft handler)
async fn raft_vote_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<crate::api::kafka_raft::VoteRequest>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, String)> {
    if let Some(coordinator) = &state.replay_coordinator {
        let raft_state = Arc::new(crate::api::kafka_raft::RaftApiState {
            coordinator: coordinator.clone(),
        });
        Ok(crate::api::kafka_raft::handle_vote_request(State(raft_state), Json(request)).await)
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Raft coordination is not enabled".to_string(),
        ))
    }
}

/// Handler for Raft heartbeat requests (wraps kafka_raft handler)
async fn raft_heartbeat_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<crate::api::kafka_raft::HeartbeatRequest>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, String)> {
    if let Some(coordinator) = &state.replay_coordinator {
        let raft_state = Arc::new(crate::api::kafka_raft::RaftApiState {
            coordinator: coordinator.clone(),
        });
        Ok(crate::api::kafka_raft::handle_heartbeat(State(raft_state), Json(request)).await)
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Raft coordination is not enabled".to_string(),
        ))
    }
}

// ============================================================================
// Raft Coordination Admin Handlers
// ============================================================================

/// Get current Raft state (admin monitoring endpoint)
async fn raft_state_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, String)> {
    if let Some(coordinator) = &state.replay_coordinator {
        let raft_state = Arc::new(crate::api::kafka_raft::RaftApiState {
            coordinator: coordinator.clone(),
        });
        Ok(crate::api::kafka_raft::get_raft_state(State(raft_state)).await)
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Raft coordination is not enabled".to_string(),
        ))
    }
}

/// Get Raft log (admin monitoring endpoint)
async fn raft_log_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, String)> {
    if let Some(coordinator) = &state.replay_coordinator {
        let raft_state = Arc::new(crate::api::kafka_raft::RaftApiState {
            coordinator: coordinator.clone(),
        });
        Ok(crate::api::kafka_raft::get_raft_log(State(raft_state)).await)
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Raft coordination is not enabled".to_string(),
        ))
    }
}

// ============================================================================
// Role-Based Access Control Middleware
// ============================================================================

/// Middleware to require write permissions (Admin, Operator, or Service roles)
/// Used for routes under /api/v1/* where auth_middleware has already run
async fn require_write_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    use crate::api::auth::Claims;

    // Extract claims from request extensions (placed by auth_middleware)
    let claims = request
        .extensions()
        .get::<Claims>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check if role has write permission
    if !claims.role.can_write() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}

/// Middleware to require admin permissions for routes under /api/v1/*
/// Assumes auth_middleware has already run and placed Claims in request extensions
async fn require_admin_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    use crate::api::auth::Claims;

    // Extract claims from request extensions (placed by auth_middleware)
    let claims = request
        .extensions()
        .get::<Claims>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check if role is admin
    if !claims.role.can_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}

/// Standalone admin middleware for routes NOT under /api/v1/*
/// Performs full authentication and admin role check
async fn standalone_admin_middleware(
    State(config): State<Arc<crate::api::auth::AuthConfig>>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    use crate::api::auth::Claims;

    // Skip if auth disabled
    if !config.enabled {
        return Ok(next.run(request).await);
    }

    // Extract and validate token
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_str()
        .map_err(|_| StatusCode::UNAUTHORIZED)?
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = config
        .validate_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Check admin role
    if !claims.role.can_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Insert claims for handler
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

// ============================================================================
// OpenAPI Spec Handler
// ============================================================================

/// Get OpenAPI spec
async fn get_openapi_spec() -> (StatusCode, String) {
    (
        StatusCode::OK,
        crate::api::openapi::generate_openapi_index(),
    )
}

// Request/Response types

// ============================================================================
// Monitoring Handlers
// ============================================================================

async fn get_cache_stats_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Graceful degradation: return default stats if cache not enabled
    if let Some(ref cache) = state.model_cache {
        let stats = cache.stats().await;

        return Ok(Json(serde_json::json!({
            "enabled": true,
            "size": stats.size,
            "capacity": stats.capacity,
            "utilization": if stats.capacity > 0 {
                (stats.size as f64 / stats.capacity as f64) * 100.0
            } else {
                0.0
            },
        })));
    }

    // Model cache not enabled - return default empty stats
    tracing::debug!("Model cache not enabled, returning default stats");
    Ok(Json(serde_json::json!({
        "enabled": false,
        "size": 0,
        "capacity": 0,
        "utilization": 0.0,
    })))
}

async fn clear_model_cache_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cache = state.model_cache.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Model cache not available".to_string(),
        )
    })?;

    cache.clear().await;

    Ok(Json(serde_json::json!({
        "status": "cache cleared"
    })))
}

async fn get_circuit_breaker_status_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use graphica_core::reliability::{CircuitBreaker, CircuitBreakerConfig};

    let breakers = state.circuit_breakers.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Circuit breaker tracking not available".to_string(),
        )
    })?;

    // Get or create circuit breaker for this model
    let breaker = breakers
        .entry(model_id.clone())
        .or_insert_with(|| {
            Arc::new(CircuitBreaker::new(
                model_id.clone(),
                CircuitBreakerConfig::default(),
            ))
        })
        .clone();

    // Determine state using available methods
    let (state_str, description) = if breaker.is_open() {
        ("open", "Circuit is open, requests are blocked")
    } else if breaker.is_half_open() {
        (
            "half_open",
            "Circuit is half-open, testing if service recovered",
        )
    } else {
        ("closed", "Circuit is closed, requests are allowed")
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "state": state_str,
        "description": description,
        "consecutive_failures": breaker.consecutive_failures(),
    })))
}

async fn reset_circuit_breaker_handler(
    State(state): State<Arc<ApiState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use graphica_core::reliability::{CircuitBreaker, CircuitBreakerConfig};

    let breakers = state.circuit_breakers.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Circuit breaker tracking not available".to_string(),
        )
    })?;

    // Replace with new circuit breaker (effectively resetting it)
    let new_breaker = Arc::new(CircuitBreaker::new(
        model_id.clone(),
        CircuitBreakerConfig::default(),
    ));
    breakers.insert(model_id.clone(), new_breaker);

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "status": "reset",
        "message": "Circuit breaker successfully reset to closed state"
    })))
}

// ============================================================================
// Authentication Handlers
// ============================================================================

use crate::api::auth::Role;
use chrono::{DateTime, Utc};

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub role: Role,
}

/// Token generation request (for admins to create service tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerateTokenRequest {
    pub user_id: String,
    pub role: Role,
}

/// Login handler - validates credentials and issues JWT
async fn login_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    use crate::api::users::AuthenticationError;

    // Get user service
    let user_service = state.user_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "User service not available".to_string(),
        )
    })?;

    // Validate credentials using UserService
    let user = match user_service
        .validate_credentials(&request.username, &request.password)
        .await
    {
        Ok(user) => user,
        Err(e) => {
            // Audit log failed login
            if let Some(audit_logger) = &state.audit_logger {
                let reason = match &e {
                    AuthenticationError::InvalidCredentials => "Invalid credentials",
                    AuthenticationError::AccountLocked { .. } => "Account locked",
                    _ => "Authentication failed",
                };
                let _ = audit_logger
                    .log_login_failure(&request.username, reason, None, None)
                    .await;
            }

            return Err(match e {
                AuthenticationError::InvalidCredentials => (
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password".to_string(),
                ),
                AuthenticationError::AccountLocked { locked_until } => (
                    StatusCode::FORBIDDEN,
                    format!("Account locked until {}", locked_until),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Authentication failed".to_string(),
                ),
            });
        }
    };

    // Generate JWT token
    let token = state
        .auth_config
        .generate_token(&user.id, user.role.clone())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Token generation failed: {}", e),
            )
        })?;

    // Audit log successful login
    if let Some(audit_logger) = &state.audit_logger {
        let _ = audit_logger
            .log_login_success(&user.id, &user.username, user.role.clone(), None, None)
            .await;
    }

    let expires_at = Utc::now() + state.auth_config.token_expiry;

    Ok(Json(LoginResponse {
        token,
        expires_at,
        role: user.role,
    }))
}

/// Generate token handler - allows admins to create service account tokens
/// This endpoint is protected by require_admin_middleware in the router
async fn generate_token_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GenerateTokenRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    // Generate service account token
    // Only admins can access this endpoint (enforced by middleware)
    let token = state
        .auth_config
        .generate_token(&request.user_id, request.role.clone())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Token generation failed: {}", e),
            )
        })?;

    let expires_at = Utc::now() + state.auth_config.token_expiry;

    Ok(Json(LoginResponse {
        token,
        expires_at,
        role: request.role,
    }))
}

/// Setup admin handler - creates initial admin user (only works if no users exist)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupAdminRequest {
    /// Setup token (required) - printed to stdout on server startup
    pub setup_token: String,
    /// Admin password (required) - minimum 12 chars with complexity
    pub password: String,
}

async fn setup_admin_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SetupAdminRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_service = state.user_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "User service not available".to_string(),
        )
    })?;

    // Validate setup token (prevents unauthorized admin creation)
    state
        .setup_token_manager
        .validate_and_consume(&request.setup_token)
        .await
        .map_err(|e| {
            tracing::warn!("Failed setup attempt with invalid token: {}", e);
            match e {
                crate::api::setup_token::SetupTokenError::AlreadyUsed => (
                    StatusCode::CONFLICT,
                    "Setup token already used. Admin user may already exist.".to_string(),
                ),
                crate::api::setup_token::SetupTokenError::Expired => (
                    StatusCode::UNAUTHORIZED,
                    "Setup token expired. Restart server to generate new token.".to_string(),
                ),
                crate::api::setup_token::SetupTokenError::Invalid => {
                    (StatusCode::UNAUTHORIZED, "Invalid setup token.".to_string())
                }
                crate::api::setup_token::SetupTokenError::NotAvailable => (
                    StatusCode::CONFLICT,
                    "Setup not available. Admin user may already exist.".to_string(),
                ),
            }
        })?;

    // Audit log setup token usage
    if let Some(audit_logger) = &state.audit_logger {
        let _ = audit_logger.log_setup_token_used(None, None).await;
    }

    // Create default admin user
    let user = user_service
        .create_default_admin(&request.password)
        .await
        .map_err(|e| {
            if e.to_string().contains("already exists") {
                (
                    StatusCode::CONFLICT,
                    "Admin user already exists".to_string(),
                )
            } else {
                (StatusCode::BAD_REQUEST, e.to_string())
            }
        })?;

    // Audit log admin creation
    if let Some(audit_logger) = &state.audit_logger {
        let _ = audit_logger
            .log_user_created(
                None,
                None,
                &user.id,
                &user.username,
                user.role.clone(),
                None,
            )
            .await;
    }

    tracing::info!("Admin user created successfully: {}", user.username);

    Ok(Json(serde_json::json!({
        "status": "created",
        "user_id": user.id,
        "username": user.username,
        "role": user.role,
        "message": "Admin user created successfully. Please login with /auth/login"
    })))
}

/// Create user handler - allows admins to create new users
async fn create_user_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<crate::api::users::CreateUserRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_service = state.user_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "User service not available".to_string(),
        )
    })?;

    let user = user_service.create_user(request).await.map_err(|e| {
        if e.to_string().contains("already exists") {
            (StatusCode::CONFLICT, "Username already exists".to_string())
        } else if e.to_string().contains("Password must") {
            (StatusCode::BAD_REQUEST, e.to_string())
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "User creation failed".to_string(),
            )
        }
    })?;

    Ok(Json(serde_json::json!({
        "status": "created",
        "user_id": user.id,
        "username": user.username,
        "role": user.role,
        "created_at": user.created_at
    })))
}
// ============================================================================
// Audit Log Handlers (Admin-Only)
// ============================================================================

/// Query audit logs (admin-only for compliance/security investigations)
async fn query_audit_logs_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<AuditQueryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use crate::api::audit::{AuditEventType, AuditFilter};

    let audit_logger = state.audit_logger.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Audit logging not available".to_string(),
        )
    })?;

    // Parse event types if provided
    let event_types = if let Some(event_type_strings) = request.event_types {
        let mut types = Vec::new();
        for s in event_type_strings {
            let event_type = serde_json::from_value::<AuditEventType>(serde_json::Value::String(s))
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid event type: {}", e),
                    )
                })?;
            types.push(event_type);
        }
        Some(types)
    } else {
        None
    };

    // Build filter
    let filter = AuditFilter {
        user_id: request.user_id,
        username: request.username,
        event_types,
        start_time: request.start_time,
        end_time: request.end_time,
        ip_address: None,
        result: None,
        limit: request.limit,
    };

    // Query audit sink
    // Note: This currently returns empty due to TODO in RocksAuditSink::query
    // Production implementation would perform RocksDB prefix scan
    let events = audit_logger.sink.query(filter).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query failed: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "events": events,
        "count": events.len(),
        "query_time": Utc::now(),
    })))
}

/// Export audit logs in SIEM format (JSON Lines)
async fn export_audit_logs_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<AuditQueryRequest>,
) -> Result<String, (StatusCode, String)> {
    use crate::api::audit::{AuditEventType, AuditFilter};

    let audit_logger = state.audit_logger.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Audit logging not available".to_string(),
        )
    })?;

    // Parse event types if provided
    let event_types = if let Some(event_type_strings) = request.event_types {
        let mut types = Vec::new();
        for s in event_type_strings {
            let event_type = serde_json::from_value::<AuditEventType>(serde_json::Value::String(s))
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid event type: {}", e),
                    )
                })?;
            types.push(event_type);
        }
        Some(types)
    } else {
        None
    };

    // Build filter
    let filter = AuditFilter {
        user_id: request.user_id,
        username: request.username,
        event_types,
        start_time: request.start_time,
        end_time: request.end_time,
        ip_address: None,
        result: None,
        limit: request.limit,
    };

    // Export in SIEM format (JSON Lines)
    let export_data = audit_logger.sink.export(filter).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Export failed: {}", e),
        )
    })?;

    Ok(export_data)
}

// ============================================================================
// Connector Registry Handlers
// ============================================================================

/// List all available connectors with metadata
async fn list_connectors_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ConnectorListResponse>, ApiError> {
    let registry = state.connector_registry.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Connector registry not available".to_string())
    })?;

    let registry_read = registry.read();
    let connectors: Vec<ConnectorMetadataResponse> = registry_read
        .list_enabled_connectors()
        .into_iter()
        .map(|meta| ConnectorMetadataResponse {
            id: meta.id.clone(),
            name: meta.name.clone(),
            version: meta.version.clone(),
            description: meta.description.clone(),
            source_type: meta.source_type.clone(),
            capabilities: ConnectorCapabilitiesResponse {
                supports_parameterized_queries: meta.capabilities.parameterized_queries,
                supports_schema_inference: meta.capabilities.schema_inference,
                supports_query_timeout: meta.capabilities.query_timeout,
                supports_streaming: meta.capabilities.streaming,
                supports_transactions: meta.capabilities.transactions,
                max_batch_size: meta.capabilities.max_batch_size.unwrap_or(10000),
            },
            required_credentials: meta
                .required_credentials
                .iter()
                .map(|cred| CredentialFieldResponse {
                    name: cred.name.clone(),
                    description: cred.description.clone(),
                    field_type: format!("{:?}", cred.field_type),
                    required: cred.required,
                    sensitive: cred.sensitive,
                })
                .collect(),
            optional_config: meta
                .optional_config
                .iter()
                .map(|conf| ConfigFieldResponse {
                    name: conf.name.clone(),
                    description: conf.description.clone(),
                    field_type: format!("{:?}", conf.field_type),
                    default_value: conf.default_value.clone(),
                    validation_regex: conf.validation_regex.clone(),
                })
                .collect(),
            tags: meta.tags.clone(),
            enabled: meta.enabled,
            registered_at: meta.registered_at,
        })
        .collect();

    Ok(Json(ConnectorListResponse { connectors }))
}

/// Get specific connector metadata by ID
async fn get_connector_handler(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectorMetadataResponse>, ApiError> {
    let registry = state.connector_registry.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Connector registry not available".to_string())
    })?;

    let registry_read = registry.read();
    let meta = registry_read
        .get_metadata(&id)
        .ok_or_else(|| ApiError::not_found(format!("Connector '{}' not found", id)))?;

    Ok(Json(ConnectorMetadataResponse {
        id: meta.id.clone(),
        name: meta.name.clone(),
        version: meta.version.clone(),
        description: meta.description.clone(),
        source_type: meta.source_type.clone(),
        capabilities: ConnectorCapabilitiesResponse {
            supports_parameterized_queries: meta.capabilities.parameterized_queries,
            supports_schema_inference: meta.capabilities.schema_inference,
            supports_query_timeout: meta.capabilities.query_timeout,
            supports_streaming: meta.capabilities.streaming,
            supports_transactions: meta.capabilities.transactions,
            max_batch_size: meta.capabilities.max_batch_size.unwrap_or(10000),
        },
        required_credentials: meta
            .required_credentials
            .iter()
            .map(|cred| CredentialFieldResponse {
                name: cred.name.clone(),
                description: cred.description.clone(),
                field_type: format!("{:?}", cred.field_type),
                required: cred.required,
                sensitive: cred.sensitive,
            })
            .collect(),
        optional_config: meta
            .optional_config
            .iter()
            .map(|conf| ConfigFieldResponse {
                name: conf.name.clone(),
                description: conf.description.clone(),
                field_type: format!("{:?}", conf.field_type),
                default_value: conf.default_value.clone(),
                validation_regex: conf.validation_regex.clone(),
            })
            .collect(),
        tags: meta.tags.clone(),
        enabled: meta.enabled,
        registered_at: meta.registered_at,
    }))
}

/// Get connector registry statistics
async fn get_connector_statistics_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ConnectorStatisticsResponse>, ApiError> {
    let registry = state.connector_registry.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Connector registry not available".to_string())
    })?;

    let registry_read = registry.read();
    let stats = registry_read.get_statistics();

    Ok(Json(ConnectorStatisticsResponse {
        total_count: stats.total_count,
        enabled_count: stats.enabled_count,
        disabled_count: stats.disabled_count,
        by_category: stats.by_category,
        total_usage: stats.total_usage as usize,
    }))
}

/// Enable a connector
async fn enable_connector_handler(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectorOperationResponse>, ApiError> {
    let registry = state.connector_registry.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Connector registry not available".to_string())
    })?;

    let mut registry_write = registry.write();
    registry_write
        .enable_connector(&id)
        .map_err(|e| ApiError::bad_request(format!("Failed to enable connector: {}", e)))?;

    Ok(Json(ConnectorOperationResponse {
        success: true,
        message: format!("Connector '{}' enabled successfully", id),
    }))
}

/// Disable a connector
async fn disable_connector_handler(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectorOperationResponse>, ApiError> {
    let registry = state.connector_registry.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Connector registry not available".to_string())
    })?;

    let mut registry_write = registry.write();
    registry_write
        .disable_connector(&id)
        .map_err(|e| ApiError::bad_request(format!("Failed to disable connector: {}", e)))?;

    Ok(Json(ConnectorOperationResponse {
        success: true,
        message: format!("Connector '{}' disabled successfully", id),
    }))
}

// ============================================================================
// Unit Tests
// ============================================================================

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod fusion_validation_tests {
    use super::*;
    use crate::api::dto::{
        FusionCandidate, FusionCandidateQuery, FusionResolveRequest, FusionResolveResponse,
        ProposeFusionRequest, ReverseFusionRequest, ReverseFusionResponse, ReviewCandidateRequest,
    };
    use crate::api::handlers::{calculate_match_confidence, format_fusion_candidate_triples};
    use crate::api::validators::{validate_entity_count, validate_match_rule};

    // ============================================================================
    // Test: validate_match_rule
    // ============================================================================

    #[test]
    fn test_validate_match_rule_valid_email() {
        let result = validate_match_rule("email");
        assert!(result.is_ok(), "Email should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_valid_phone() {
        let result = validate_match_rule("phone");
        assert!(result.is_ok(), "Phone should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_valid_ssn() {
        let result = validate_match_rule("ssn");
        assert!(result.is_ok(), "SSN should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_valid_name() {
        let result = validate_match_rule("name");
        assert!(result.is_ok(), "Name should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_valid_address() {
        let result = validate_match_rule("address");
        assert!(result.is_ok(), "Address should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_valid_tax_id() {
        let result = validate_match_rule("tax_id");
        assert!(result.is_ok(), "Tax ID should be a valid match rule");
    }

    #[test]
    fn test_validate_match_rule_invalid() {
        let result = validate_match_rule("invalid_rule");
        assert!(result.is_err(), "Invalid rule should return error");

        let err = result.unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::BAD_REQUEST,
            "Expected BadRequest error"
        );
        assert!(
            err.message().contains("Unsupported match rule"),
            "Error should mention unsupported rule"
        );
        assert!(
            err.message().contains("email"),
            "Error should list supported rules"
        );
    }

    #[test]
    fn test_validate_match_rule_empty() {
        let result = validate_match_rule("");
        assert!(result.is_err(), "Empty rule should return error");
    }

    #[test]
    fn test_validate_match_rule_case_sensitive() {
        let result = validate_match_rule("EMAIL");
        assert!(result.is_err(), "Match rules should be case-sensitive");
    }

    // ============================================================================
    // Test: validate_entity_count
    // ============================================================================

    #[test]
    fn test_validate_entity_count_minimum() {
        let mut entity1 = serde_json::Map::new();
        entity1.insert(
            "id".to_string(),
            serde_json::Value::String("e1".to_string()),
        );

        let mut entity2 = serde_json::Map::new();
        entity2.insert(
            "id".to_string(),
            serde_json::Value::String("e2".to_string()),
        );

        let entities = vec![entity1, entity2];
        let result = validate_entity_count(&entities);
        assert!(result.is_ok(), "2 entities should be valid (minimum)");
    }

    #[test]
    fn test_validate_entity_count_too_few_zero() {
        let entities: Vec<serde_json::Map<String, serde_json::Value>> = vec![];
        let result = validate_entity_count(&entities);
        assert!(result.is_err(), "0 entities should be invalid");

        let err = result.unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::BAD_REQUEST,
            "Expected BadRequest error"
        );
        assert!(
            err.message().contains("at least 2 entities"),
            "Error should mention minimum"
        );
    }

    #[test]
    fn test_validate_entity_count_too_few_one() {
        let mut entity1 = serde_json::Map::new();
        entity1.insert(
            "id".to_string(),
            serde_json::Value::String("e1".to_string()),
        );

        let entities = vec![entity1];
        let result = validate_entity_count(&entities);
        assert!(result.is_err(), "1 entity should be invalid");

        let err = result.unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::BAD_REQUEST,
            "Expected BadRequest error"
        );
        assert!(
            err.message().contains("at least 2 entities"),
            "Error should mention minimum"
        );
        assert!(
            err.message().contains("got 1"),
            "Error should show actual count"
        );
    }

    #[test]
    fn test_validate_entity_count_maximum() {
        let mut entities = Vec::new();
        for i in 0..100 {
            let mut entity = serde_json::Map::new();
            entity.insert(
                "id".to_string(),
                serde_json::Value::String(format!("e{}", i)),
            );
            entities.push(entity);
        }

        let result = validate_entity_count(&entities);
        assert!(result.is_ok(), "100 entities should be valid (maximum)");
    }

    #[test]
    fn test_validate_entity_count_too_many() {
        let mut entities = Vec::new();
        for i in 0..101 {
            let mut entity = serde_json::Map::new();
            entity.insert(
                "id".to_string(),
                serde_json::Value::String(format!("e{}", i)),
            );
            entities.push(entity);
        }

        let result = validate_entity_count(&entities);
        assert!(result.is_err(), "101 entities should be invalid");

        let err = result.unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::BAD_REQUEST,
            "Expected BadRequest error"
        );
        assert!(
            err.message().contains("maximum 100 entities"),
            "Error should mention maximum"
        );
        assert!(
            err.message().contains("got 101"),
            "Error should show actual count"
        );
    }

    #[test]
    fn test_validate_entity_count_mid_range() {
        let mut entities = Vec::new();
        for i in 0..50 {
            let mut entity = serde_json::Map::new();
            entity.insert(
                "id".to_string(),
                serde_json::Value::String(format!("e{}", i)),
            );
            entities.push(entity);
        }

        let result = validate_entity_count(&entities);
        assert!(result.is_ok(), "50 entities should be valid");
    }

    // ============================================================================
    // Test: calculate_match_confidence
    // ============================================================================

    #[test]
    fn test_calculate_match_confidence_email() {
        let entities = vec![];
        let confidence = calculate_match_confidence("email", &entities);
        assert_eq!(confidence, 0.95, "Email match should have 0.95 confidence");
    }

    #[test]
    fn test_calculate_match_confidence_phone() {
        let entities = vec![];
        let confidence = calculate_match_confidence("phone", &entities);
        assert_eq!(confidence, 0.90, "Phone match should have 0.90 confidence");
    }

    #[test]
    fn test_calculate_match_confidence_ssn() {
        let entities = vec![];
        let confidence = calculate_match_confidence("ssn", &entities);
        assert_eq!(confidence, 0.99, "SSN match should have 0.99 confidence");
    }

    #[test]
    fn test_calculate_match_confidence_name() {
        let entities = vec![];
        let confidence = calculate_match_confidence("name", &entities);
        assert_eq!(confidence, 0.70, "Name match should have 0.70 confidence");
    }

    #[test]
    fn test_calculate_match_confidence_address() {
        let entities = vec![];
        let confidence = calculate_match_confidence("address", &entities);
        assert_eq!(
            confidence, 0.75,
            "Address match should have 0.75 confidence"
        );
    }

    #[test]
    fn test_calculate_match_confidence_tax_id() {
        let entities = vec![];
        let confidence = calculate_match_confidence("tax_id", &entities);
        assert_eq!(confidence, 0.98, "Tax ID match should have 0.98 confidence");
    }

    #[test]
    fn test_calculate_match_confidence_unknown() {
        let entities = vec![];
        let confidence = calculate_match_confidence("unknown_rule", &entities);
        assert_eq!(
            confidence, 0.80,
            "Unknown rules should have default 0.80 confidence"
        );
    }

    // ============================================================================
    // Test: format_fusion_candidate_triples
    // ============================================================================

    #[test]
    fn test_format_fusion_candidate_triples_basic() {
        let mut entity1 = serde_json::Map::new();
        entity1.insert(
            "id".to_string(),
            serde_json::Value::String("cust_001".to_string()),
        );
        entity1.insert(
            "email".to_string(),
            serde_json::Value::String("john@example.com".to_string()),
        );

        let mut entity2 = serde_json::Map::new();
        entity2.insert(
            "id".to_string(),
            serde_json::Value::String("cust_002".to_string()),
        );
        entity2.insert(
            "email".to_string(),
            serde_json::Value::String("john@example.com".to_string()),
        );

        let entities = vec![entity1, entity2];
        let triples =
            format_fusion_candidate_triples("cand_123", &entities, "email", "john@example.com");

        // Verify triples contain expected URIs and values
        assert!(
            triples.contains("cand_123"),
            "Triples should contain candidate ID"
        );
        assert!(
            triples.contains("/FusionCandidate"),
            "Triples should declare candidate type"
        );
        assert!(
            triples.contains("/matchRule"),
            "Triples should include match rule"
        );
        assert!(
            triples.contains("email"),
            "Triples should include rule value"
        );
        assert!(
            triples.contains("/matchValue"),
            "Triples should include match value"
        );
        assert!(
            triples.contains("john@example.com"),
            "Triples should include email value"
        );
        assert!(
            triples.contains("cust_001"),
            "Triples should reference first entity"
        );
        assert!(
            triples.contains("cust_002"),
            "Triples should reference second entity"
        );
        assert!(
            triples.contains("/hasEntity"),
            "Triples should link to entities"
        );
        assert!(
            triples.contains("/proposedAt"),
            "Triples should include timestamp"
        );
        assert!(triples.contains("/status"), "Triples should include status");
        assert!(triples.contains("proposed"), "Status should be 'proposed'");
    }

    #[test]
    fn test_format_fusion_candidate_triples_multiple_entities() {
        let mut entities = Vec::new();
        for i in 1..=5 {
            let mut entity = serde_json::Map::new();
            entity.insert(
                "id".to_string(),
                serde_json::Value::String(format!("cust_{:03}", i)),
            );
            entity.insert(
                "phone".to_string(),
                serde_json::Value::String("+1-555-0100".to_string()),
            );
            entities.push(entity);
        }

        let triples =
            format_fusion_candidate_triples("cand_456", &entities, "phone", "+1-555-0100");

        // Verify all entities are referenced
        for i in 1..=5 {
            let entity_id = format!("cust_{:03}", i);
            assert!(
                triples.contains(&entity_id),
                "Triples should reference entity {}",
                entity_id
            );
        }

        // Count how many hasEntity relationships exist
        let has_entity_count = triples.matches("/hasEntity").count();
        assert_eq!(has_entity_count, 5, "Should have 5 hasEntity relationships");
    }

    #[test]
    fn test_format_fusion_candidate_triples_special_characters() {
        let mut entity = serde_json::Map::new();
        entity.insert(
            "id".to_string(),
            serde_json::Value::String("cust_999".to_string()),
        );
        entity.insert(
            "email".to_string(),
            serde_json::Value::String("test+tag@example.com".to_string()),
        );

        let entities = vec![entity];
        let triples = format_fusion_candidate_triples(
            "cand_special",
            &entities,
            "email",
            "test+tag@example.com",
        );

        // Should handle special characters in email
        assert!(
            triples.contains("test+tag@example.com"),
            "Should preserve special characters"
        );
    }

    // ============================================================================
    // Integration Tests: Request/Response Types
    // ============================================================================

    #[test]
    fn test_propose_fusion_request_deserialization() {
        let json = r#"{
            "dataset": "customers",
            "rule": "email",
            "min_confidence": 0.8
        }"#;

        let request: Result<ProposeFusionRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize valid request");

        let req = request.unwrap();
        assert_eq!(req.dataset, "customers");
        assert_eq!(req.rule, "email");
        assert_eq!(req.min_confidence, Some(0.8));
    }

    #[test]
    fn test_propose_fusion_request_optional_confidence() {
        let json = r#"{
            "dataset": "customers",
            "rule": "email"
        }"#;

        let request: Result<ProposeFusionRequest, _> = serde_json::from_str(json);
        assert!(
            request.is_ok(),
            "Should deserialize request without min_confidence"
        );

        let req = request.unwrap();
        assert_eq!(req.min_confidence, None);
    }

    #[test]
    fn test_fusion_candidate_serialization() {
        let mut entity = serde_json::Map::new();
        entity.insert(
            "id".to_string(),
            serde_json::Value::String("cust_001".to_string()),
        );

        let candidate = FusionCandidate {
            candidate_id: "cand_123".to_string(),
            entities: vec![entity],
            match_rule: "email".to_string(),
            match_value: "test@example.com".to_string(),
            confidence: 0.95,
            proposed_at: "2024-01-01T00:00:00Z".to_string(),
            status: "proposed".to_string(),
        };

        let json = serde_json::to_string(&candidate);
        assert!(json.is_ok(), "Should serialize fusion candidate");

        let json_str = json.unwrap();
        assert!(json_str.contains("cand_123"));
        assert!(json_str.contains("email"));
        assert!(json_str.contains("0.95"));
        assert!(json_str.contains("proposed"));
    }

    #[test]
    fn test_review_candidate_request_with_notes() {
        let json = r#"{
            "reviewer": "analyst_1",
            "notes": "Verified - same person"
        }"#;

        let request: Result<ReviewCandidateRequest, _> = serde_json::from_str(json);
        assert!(
            request.is_ok(),
            "Should deserialize review request with notes"
        );

        let req = request.unwrap();
        assert_eq!(req.reviewer, Some("analyst_1".to_string()));
        assert_eq!(req.notes, Some("Verified - same person".to_string()));
    }

    #[test]
    fn test_review_candidate_request_optional_fields() {
        let json = r#"{}"#;

        let request: Result<ReviewCandidateRequest, _> = serde_json::from_str(json);
        assert!(
            request.is_ok(),
            "Should deserialize review request with no fields"
        );

        let req = request.unwrap();
        assert_eq!(req.reviewer, None);
        assert_eq!(req.notes, None);
    }

    #[test]
    fn test_fusion_resolve_request_deserialization() {
        let json = r#"{
            "entities": [
                {"id": "cust_001", "email": "john@example.com"},
                {"id": "cust_002", "email": "john@example.com"}
            ],
            "rule": "email",
            "confidence": 0.95
        }"#;

        let request: Result<FusionResolveRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize fusion resolve request");

        let req = request.unwrap();
        assert_eq!(req.entities.len(), 2);
        assert_eq!(req.rule, "email");
        assert_eq!(req.confidence, Some(0.95));
    }

    #[test]
    fn test_fusion_resolve_request_optional_confidence() {
        let json = r#"{
            "entities": [
                {"id": "cust_001"},
                {"id": "cust_002"}
            ],
            "rule": "email"
        }"#;

        let request: Result<FusionResolveRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize without confidence");

        let req = request.unwrap();
        assert_eq!(req.confidence, None);
    }

    #[test]
    fn test_reverse_fusion_request_deserialization() {
        let json = r#"{
            "reason": "Incorrect match - different people"
        }"#;

        let request: Result<ReverseFusionRequest, _> = serde_json::from_str(json);
        assert!(request.is_ok(), "Should deserialize reverse request");

        let req = request.unwrap();
        assert_eq!(
            req.reason,
            Some("Incorrect match - different people".to_string())
        );
    }

    #[test]
    fn test_fusion_candidate_query_default_values() {
        let query = FusionCandidateQuery {
            status: None,
            limit: None,
        };

        assert!(query.status.is_none(), "Status should be optional");
        assert!(query.limit.is_none(), "Limit should be optional");
    }

    #[test]
    fn test_fusion_candidate_query_custom_values() {
        let query = FusionCandidateQuery {
            status: Some("approved".to_string()),
            limit: Some(50),
        };

        assert_eq!(query.status.unwrap(), "approved");
        assert_eq!(query.limit.unwrap(), 50);
    }

    #[test]
    fn test_entity_with_missing_id() {
        let mut entity = serde_json::Map::new();
        entity.insert(
            "email".to_string(),
            serde_json::Value::String("test@example.com".to_string()),
        );
        // No "id" field

        let entities = vec![entity.clone(), entity];

        // This would be caught by the validation in resolve_entity_fusion
        let has_id = entities.iter().all(|e| e.contains_key("id"));
        assert!(!has_id, "Should detect missing ID");
    }

    #[test]
    fn test_fusion_response_serialization() {
        let response = FusionResolveResponse {
            fusion_id: "fus_123".to_string(),
            merged_entity_id: "cust_001".to_string(),
            source_entity_ids: vec!["cust_002".to_string(), "cust_003".to_string()],
            rule: "email".to_string(),
            confidence: 0.95,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&response);
        assert!(json.is_ok(), "Should serialize fusion response");

        let json_str = json.unwrap();
        assert!(json_str.contains("fus_123"));
        assert!(json_str.contains("cust_001"));
        assert!(json_str.contains("cust_002"));
        assert!(json_str.contains("cust_003"));
    }

    #[test]
    fn test_reverse_fusion_response_serialization() {
        let response = ReverseFusionResponse {
            fusion_id: "fus_123".to_string(),
            reversed: true,
            reversed_at: "2024-01-02T00:00:00Z".to_string(),
            reason: Some("Incorrect match".to_string()),
        };

        let json = serde_json::to_string(&response);
        assert!(json.is_ok(), "Should serialize reverse response");

        let json_str = json.unwrap();
        assert!(json_str.contains("fus_123"));
        assert!(json_str.contains("true"));
        assert!(json_str.contains("Incorrect match"));
    }
}
