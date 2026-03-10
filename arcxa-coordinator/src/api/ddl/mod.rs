//! DDL API Module
//!
//! HTTP API for DDL generation from SHACL constraints.

pub mod executor;
pub mod handlers;
pub mod openapi;
pub mod types;

use crate::api::ApiState;
use axum::{routing::post, Router};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

pub use openapi::DdlApiDoc;

/// Create DDL API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/ddl/swagger-ui`
pub fn router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/ddl/swagger-ui")
                .url("/ddl/api-docs/openapi.json", DdlApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        .route("/ddl/generate", post(handlers::generate_ddl))
        .route("/ddl/migrate", post(handlers::generate_migration))
        .route("/ddl/validate", post(handlers::validate_ddl))
        .route("/ddl/shapes", post(handlers::list_shapes))
        .route("/ddl/execute", post(handlers::execute_ddl))
}
