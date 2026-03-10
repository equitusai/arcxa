//! Loader API Module
//!
//! REST API for ETL loader operations including job management, monitoring,
//! checkpoint control, and dead letter queue management.
//!
//! ## Endpoints
//!
//! ### Job Management
//! - `POST /api/v1/loader/jobs` - Create new ETL job
//! - `GET /api/v1/loader/jobs` - List all jobs
//! - `GET /api/v1/loader/jobs/:job_id` - Get job status
//! - `DELETE /api/v1/loader/jobs/:job_id` - Cancel job
//!
//! ### Job Control
//! - `POST /api/v1/loader/jobs/:job_id/resume` - Resume from checkpoint
//!
//! ### Checkpoint Management
//! - `GET /api/v1/loader/jobs/:job_id/checkpoint` - Get checkpoint status
//!
//! ### Dead Letter Queue
//! - `GET /api/v1/loader/jobs/:job_id/dlq` - Get DLQ statistics
//! - `GET /api/v1/loader/jobs/:job_id/dlq/rows` - Get failed rows
//! - `POST /api/v1/loader/jobs/:job_id/dlq/reprocess` - Reprocess failed rows
//!
//! ### Health
//! - `GET /api/v1/loader/health` - Loader subsystem health

pub mod handlers;
pub mod openapi;
pub mod types;

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;
pub use handlers::*;
pub use openapi::LoaderApiDoc;
pub use types::*;

/// Create loader API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/loader/swagger-ui`
pub fn create_loader_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/loader/swagger-ui")
                .url("/loader/api-docs/openapi.json", LoaderApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Job management routes
        .route("/loader/jobs", post(handlers::create_loader_job))
        .route("/loader/jobs", get(handlers::list_loader_jobs))
        .route("/loader/jobs/:job_id", get(handlers::get_job_status))
        .route("/loader/jobs/:job_id", delete(handlers::cancel_loader_job))
        // Job control routes
        .route(
            "/loader/jobs/:job_id/resume",
            post(handlers::resume_loader_job),
        )
        // Checkpoint routes
        .route(
            "/loader/jobs/:job_id/checkpoint",
            get(handlers::get_checkpoint_status),
        )
        // DLQ routes
        .route("/loader/jobs/:job_id/dlq", get(handlers::get_dlq_stats))
        .route("/loader/jobs/:job_id/dlq/rows", get(handlers::get_dlq_rows))
        .route(
            "/loader/jobs/:job_id/dlq/reprocess",
            post(handlers::reprocess_dlq_rows),
        )
        // Health routes
        .route("/loader/health", get(handlers::get_loader_health))
}

// TODO: Add integration tests once actual implementation is complete
// Tests would verify:
// - Job creation and validation
// - Status tracking and progress reporting
// - Checkpoint creation and resume
// - DLQ row capture and retrieval
// - Error handling and retry logic
