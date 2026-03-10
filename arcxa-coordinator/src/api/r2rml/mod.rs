//! R2RML Mapping API
//!
//! REST API endpoints for managing R2RML mappings (Stage 2: Semantic Mapping).

pub mod handlers;
pub mod openapi;
pub mod types;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
pub use openapi::R2rmlApiDoc;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

/// Create the R2RML API router
///
/// This router is mounted under `/api/v1` in the main router.
/// Interactive API documentation is available at:
/// - `/api/v1/r2rml/swagger-ui`
pub fn create_router() -> Router<Arc<crate::api::ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/r2rml/swagger-ui")
                .url("/r2rml/api-docs/openapi.json", R2rmlApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Mapping CRUD operations
        .route("/mappings", post(handlers::create_mapping))
        .route("/mappings", get(handlers::list_mappings))
        .route("/mappings/:mapping_id", get(handlers::get_mapping))
        .route("/mappings/:mapping_id", put(handlers::update_mapping))
        .route("/mappings/:mapping_id", delete(handlers::delete_mapping))
        // Mapping execution
        .route(
            "/mappings/:mapping_id/execute",
            post(handlers::execute_mapping),
        )
        // Mapping suggestion from profile
        .route(
            "/mappings/suggest",
            post(handlers::suggest_mapping_from_profile),
        )
        // Mapping validation
        .route("/mappings/validate", post(handlers::validate_mapping))
}
