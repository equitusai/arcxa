//! Workflow orchestration API module
//!
//! Provides REST endpoints for workflow management, execution, and monitoring.

pub mod handlers;
pub(crate) mod materialization;
pub mod openapi;
pub mod types;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;
pub use openapi::WorkflowApiDoc;

/// Create workflow API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/workflows/swagger-ui`
pub fn create_workflow_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/workflows/swagger-ui")
                .url(
                    "/workflows/api-docs/openapi.json",
                    WorkflowApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Workflow CRUD operations
        .route("/workflows", post(handlers::register_workflow))
        .route("/workflows", get(handlers::list_workflows))
        .route(
            "/workflows/:id/details",
            get(handlers::get_workflow_details),
        )
        .route("/workflows/:id", get(handlers::get_workflow))
        .route("/workflows/:id", put(handlers::update_workflow))
        .route("/workflows/:id", delete(handlers::delete_workflow))
        // Workflow execution
        .route("/workflows/:id/execute", post(handlers::execute_workflow))
        .route(
            "/workflows/:id/execute-async",
            post(crate::workflows::api::handlers::execute_workflow_async),
        )
        .route(
            "/workflows/legacy/:id/execute",
            post(crate::workflows::api::handlers::execute_workflow_legacy),
        )
        .route(
            "/workflows/:id/route-stats",
            post(crate::workflows::api::handlers::get_route_stats),
        )
        // Workflow validation & testing
        .route(
            "/workflows/validate",
            post(handlers::validate_workflow_definition),
        )
        .route("/workflows/test-step", post(handlers::test_workflow_step))
        .route("/workflows/dry-run", post(handlers::dry_run_workflow))
        .route(
            "/workflows/:id/dry-run",
            post(crate::workflows::api::handlers::dry_run_workflow),
        )
        // Workflow scheduling (plural endpoints - v0.3.0+)
        .route("/workflows/:id/schedules", post(handlers::create_schedule))
        .route("/workflows/:id/schedules", get(handlers::list_schedules))
        .route(
            "/workflows/:id/schedules/:schedule_id",
            get(handlers::get_schedule),
        )
        .route(
            "/workflows/:id/schedules/:schedule_id",
            put(handlers::update_schedule),
        )
        .route(
            "/workflows/:id/schedules/:schedule_id",
            delete(handlers::delete_schedule),
        )
        // Execution history
        .route(
            "/workflows/:id/executions",
            get(handlers::list_workflow_executions),
        )
        // Progress monitoring (Phase 3)
        // IMPORTANT: More specific routes MUST come before general routes in Axum
        // Register these BEFORE the /workflows/:id/executions route
        .route(
            "/workflows/executions/active",
            get(handlers::get_active_executions),
        )
        .route(
            "/workflows/executions/:execution_id/progress",
            get(handlers::get_execution_progress),
        )
        .route(
            "/workflows/executions/:execution_id",
            delete(handlers::cancel_execution),
        )
        .route(
            "/workflows/:id/executions/progress",
            get(handlers::list_workflow_execution_progress),
        )
        // Modern execution tracking and approvals
        .route(
            "/executions/:id",
            get(crate::workflows::api::handlers::get_execution),
        )
        .route(
            "/executions/:id/logs",
            get(crate::workflows::api::handlers::get_execution_logs),
        )
        .route(
            "/executions",
            get(crate::workflows::api::handlers::list_executions),
        )
        .route(
            "/executions/:id/stop",
            post(crate::workflows::api::handlers::stop_execution),
        )
        .route(
            "/executions/:id/pause",
            post(crate::workflows::api::handlers::pause_execution),
        )
        .route(
            "/executions/:id/resume",
            post(crate::workflows::api::handlers::resume_execution),
        )
        .route(
            "/executions/:id/abort",
            post(crate::workflows::api::handlers::abort_execution),
        )
        .route(
            "/approvals",
            get(crate::workflows::api::approval_handlers::list_approvals),
        )
        .route(
            "/approvals/stats",
            get(crate::workflows::api::approval_handlers::get_approval_stats),
        )
        .route(
            "/approvals/:request_id",
            get(crate::workflows::api::approval_handlers::get_approval),
        )
        .route(
            "/approvals/:request_id/approve",
            post(crate::workflows::api::approval_handlers::approve_approval),
        )
        .route(
            "/approvals/:request_id/reject",
            post(crate::workflows::api::approval_handlers::reject_approval),
        )
        .route(
            "/approvals/:request_id/cancel",
            post(crate::workflows::api::approval_handlers::cancel_approval),
        )
        .route(
            "/schedule/preview",
            post(crate::workflows::api::handlers::preview_schedule),
        )
}

// Re-export commonly used types
pub use types::{
    ExecuteWorkflowRequest, ExecuteWorkflowResponse, RegisterWorkflowRequest,
    RegisterWorkflowResponse, ScheduleWorkflowRequest, ScheduleWorkflowResponse,
    UpdateScheduleRequest, WorkflowDetailsResponse, WorkflowScheduleInfo, WorkflowSummaryDto,
};
