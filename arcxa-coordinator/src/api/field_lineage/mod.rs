//! Field Lineage API Module
//!
//! REST API endpoints for field-level provenance, golden record creation,
//! and voting-based conflict resolution.

pub mod handlers;
pub mod openapi;
pub mod types;

use crate::api::ApiState;
use axum::{
    routing::{get, post},
    Router,
};
pub use openapi::FieldLineageApiDoc;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

/// Create field lineage router
///
/// Interactive API documentation is available at:
/// - `/api/v1/field-lineage/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/field-lineage/swagger-ui")
                .url(
                    "/field-lineage/api-docs/openapi.json",
                    FieldLineageApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Field lineage queries
        .route(
            "/entities/:entity_id/fields/:field_name/lineage",
            get(handlers::get_field_lineage),
        )
        .route(
            "/entities/:entity_id/fields/:field_name/history",
            get(handlers::get_field_history),
        )
        // Golden record operations
        .route(
            "/entities/:entity_id/resolved-entity",
            post(handlers::create_resolved_entity),
        )
        .route(
            "/entities/:entity_id/resolved-entity",
            get(handlers::get_resolved_entity),
        )
        // Conflict management
        .route(
            "/conflicts/requiring-review",
            get(handlers::list_conflicts_requiring_review),
        )
        .route(
            "/entities/:entity_id/fields/:field_name/resolve",
            post(handlers::resolve_field_conflict),
        )
        // Cache metrics
        .route(
            "/resolved-entities/cache/metrics",
            get(handlers::get_cache_metrics),
        )
}
