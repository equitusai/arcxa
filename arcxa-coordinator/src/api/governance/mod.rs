//! Governance API Module
//!
//! Provides SPARQL query execution and RDF store management.

pub mod handlers;
pub mod openapi;

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;

/// Create governance API router with Swagger UI
///
/// Interactive API documentation is available at:
/// - `/api/v1/governance/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/governance/swagger-ui")
                .url(
                    "/governance/api-docs/openapi.json",
                    openapi::GovernanceApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // SPARQL query endpoint
        .route("/governance/sparql", post(handlers::sparql_query))
        // RDF statistics endpoints
        .route("/governance/stats", get(handlers::get_rdf_stats))
        .route(
            "/governance/auto-save-stats",
            get(handlers::get_rdf_auto_save_stats),
        )
        .route("/governance/save", post(handlers::trigger_rdf_save))
}
