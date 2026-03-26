//! Unified Mapping API Module
//!
//! REST API for unified mapping operations that consolidate multiple source
//! CSV mapping sessions into a single unified mapping targeting a normalized
//! relational database schema (PostgreSQL, IBM DB2, Oracle, or Databricks).
//!
//! ## Endpoints
//!
//! ### AI/ML Field Mapping
//! - `POST /api/v1/mapping/suggest` - Suggest field mappings using multi-dimensional similarity analysis
//! - `POST /api/v1/mapping/plan-sql` - Generate goal-driven SQL from ontology property bindings
//! - `POST /api/v1/mapping/bindings` - Upsert ontology→physical bindings (versioned)
//! - `GET /api/v1/mapping/bindings` - List current ontology→physical bindings
//! - `GET /api/v1/mapping/bindings/history` - View binding version history
//! - `POST /api/v1/mapping/bindings/coverage` - Diff ontology requirements vs physical coverage
//!
//! ### Session Management
//! - `POST /api/v1/mapping/unified-sessions` - Create new unified session
//! - `GET /api/v1/mapping/unified-sessions` - List all unified sessions
//! - `GET /api/v1/mapping/unified-sessions/:id` - Get unified session by ID
//! - `PUT /api/v1/mapping/unified-sessions/:id` - Update unified session
//! - `DELETE /api/v1/mapping/unified-sessions/:id` - Delete unified session
//!
//! ### Conflict Resolution
//! - `POST /api/v1/mapping/unified-sessions/:id/resolve-conflicts` - Resolve field mapping conflicts
//!
//! ### Database Loading
//! - `POST /api/v1/mapping/unified-sessions/:id/load` - Load data to target database
//! - `GET /api/v1/mapping/load-jobs/:job_id` - Get load job status
//! - `POST /api/v1/mapping/load-jobs/:job_id/callback` - Report external executor status (DB2 only)
//!
//! ### Statistics
//! - `GET /api/v1/mapping/unified-sessions/statistics` - Get global statistics
//!
//! ## Workflow
//!
//! 1. Create multiple source mapping sessions (CSV → Ontology)
//! 2. Create unified session from source sessions
//! 3. Review and resolve any detected conflicts
//! 4. Load data to target database (PostgreSQL, Databricks, DB2, or Oracle)
//! 5. Monitor load job progress
//!
//! ## Conflict Resolution Strategies
//!
//! - **NoConflict**: Single source, no conflict
//! - **UsePrimary**: Select value from designated primary source
//! - **Merge**: Concatenate values from all sources
//! - **Coalesce**: Use first non-null value (ordered by confidence)
//! - **CustomRule**: Apply user-defined transformation rule

pub mod field_similarity;
pub mod handlers;
mod internal_load;
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
pub use field_similarity::*;
pub use handlers::*;
pub use openapi::UnifiedMappingApiDoc;
pub use types::*;

/// Create unified mapping API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/mapping/swagger-ui`
pub fn create_unified_mapping_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/mapping/swagger-ui")
                .url(
                    "/mapping/api-docs/openapi.json",
                    UnifiedMappingApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Field similarity and mapping suggestions (AI/ML)
        .route(
            "/mapping/suggest",
            post(field_similarity::suggest_field_mappings),
        )
        .route("/mapping/plan-sql", post(handlers::plan_goal_sql))
        .route(
            "/mapping/bindings",
            post(handlers::upsert_ontology_bindings).get(handlers::list_ontology_bindings),
        )
        .route("/mapping/bindings/history", get(handlers::binding_history))
        .route(
            "/mapping/bindings/coverage",
            post(handlers::binding_coverage),
        )
        // Session management routes
        .route(
            "/mapping/unified-sessions",
            post(handlers::create_unified_session),
        )
        .route(
            "/mapping/unified-sessions",
            get(handlers::list_unified_sessions),
        )
        .route(
            "/mapping/unified-sessions/:id",
            get(handlers::get_unified_session),
        )
        .route(
            "/mapping/unified-sessions/:id",
            put(handlers::update_unified_session),
        )
        .route(
            "/mapping/unified-sessions/:id",
            delete(handlers::delete_unified_session),
        )
        // Conflict resolution routes
        .route(
            "/mapping/unified-sessions/:id/resolve-conflicts",
            post(handlers::resolve_conflicts),
        )
        // Database loading routes
        .route(
            "/mapping/unified-sessions/:id/load",
            post(handlers::load_to_database),
        )
        .route(
            "/mapping/load-jobs/:job_id",
            get(handlers::get_load_job_status),
        )
        .route(
            "/mapping/load-jobs/:job_id/callback",
            post(handlers::external_load_job_callback),
        )
        // Statistics routes
        .route(
            "/mapping/unified-sessions/statistics",
            get(handlers::get_global_statistics),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_router_creation() {
        // Verify router can be created
        let router = create_unified_mapping_router();

        // Router is created successfully if we get here
        assert!(true);
    }
}
