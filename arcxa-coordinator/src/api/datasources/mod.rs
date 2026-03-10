//! Data Source Catalog API module
//!
//! REST API endpoints for managing data source configurations.

pub mod discovery;
pub mod discovery_sse;
pub mod handlers;
pub mod openapi;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::rate_limit::discovery_rate_limiter;
use crate::api::ApiState;
pub use openapi::DataSourceApiDoc;

/// Create data sources API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/datasources/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/datasources/swagger-ui")
                .url(
                    "/datasources/api-docs/openapi.json",
                    DataSourceApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // CRUD operations
        .route("/datasources", post(handlers::register_datasource))
        .route("/datasources", get(handlers::list_datasources))
        .route("/datasources/:id", get(handlers::get_datasource))
        .route("/datasources/:id", put(handlers::update_datasource))
        .route("/datasources/:id", delete(handlers::delete_datasource))
        // Operations
        .route("/datasources/test", post(handlers::test_connection))
        .route(
            "/datasources/:id/schema/infer",
            post(handlers::infer_schema),
        )
        .route(
            "/datasources/:id/schema/infer-enhanced",
            post(handlers::infer_schema_enhanced),
        )
        .route("/datasources/:id/query", post(handlers::execute_query))
        .route("/datasources/search", get(handlers::search_datasources))
        // Schema Discovery (Phase 1: Async with Progress Tracking)
        // Rate limited to prevent resource exhaustion (10 discoveries/minute per IP)
        .route(
            "/datasources/:id/discover",
            post(discovery::start_discovery).layer(discovery_rate_limiter()),
        )
        .route(
            "/datasources/:id/discovery/progress",
            get(discovery::get_discovery_progress),
        )
        .route(
            "/datasources/:id/discovery/result",
            get(discovery::get_discovery_result),
        )
        .route(
            "/datasources/:id/discovery",
            delete(discovery::cancel_discovery),
        )
        .route(
            "/datasources/:id/discovery/stream",
            get(discovery_sse::stream_discovery_progress),
        )
}
