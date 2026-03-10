//! Lineage Query API
//!
//! REST API endpoints for querying W3C PROV-compliant lineage data from RDF store.
//!
//! ## Endpoints
//!
//! ### Record Lineage
//! - `GET /api/v1/lineage/record/:record_id` - Get lineage for a specific record
//! - `GET /api/v1/lineage/record/:record_id/graph` - Get lineage graph (upstream + downstream)
//!
//! ### Model Impact
//! - `GET /api/v1/lineage/model/:model_id/impact` - Get model impact analysis
//!
//! ### Run Lineage
//! - `GET /api/v1/lineage/run/:run_id` - Get lineage for a specific run
//!
//! ### Time-based Queries
//! - `POST /api/v1/lineage/time-range` - Query lineage by time range
//!
//! ### Row-Level Lineage (Query Endpoints - Production Safe)
//! - `GET /api/v1/lineage/row/:row_key` - Get lineage for a specific row
//! - `GET /api/v1/lineage/row/:row_key/journey` - Get complete row journey
//! - `GET /api/v1/lineage/batch/:batch_id` - Get lineage for all rows in a batch
//! - `GET /api/v1/lineage/job/:job_id/stats` - Get job statistics
//! - `GET /api/v1/lineage/job/:job_id/filtered` - Get filtered rows for a job
//!
//! ### Row-Level Lineage (Test Endpoints - ⚠️ DISABLED BY DEFAULT)
//! - `POST /api/v1/lineage/row/test` - Write single lineage event (TEST ONLY)
//! - `POST /api/v1/lineage/rows/batch/test` - Write batch of lineage events (TEST ONLY)
//!
//! **IMPORTANT**: Test write endpoints are disabled by default and require `ENABLE_TEST_LINEAGE_API=true`.
//! These should NEVER be enabled in production. Lineage should be written internally by ETL pipelines
//! using the `RowLevelLineageSink` trait.

pub mod column_handlers;
pub mod handlers;
pub mod openapi;
pub mod row_handlers;
pub mod schema_handlers;
pub mod types;

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;
pub use column_handlers::*;
pub use handlers::*;
pub use openapi::LineageApiDoc;
pub use row_handlers::*;
pub use schema_handlers::*;
pub use types::*;

/// Create lineage query API router with Swagger UI
pub fn create_lineage_router() -> Router<Arc<ApiState>> {
    let router = Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/lineage/swagger-ui")
                .url("/lineage/api-docs/openapi.json", LineageApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Record lineage routes
        .route(
            "/lineage/record/:record_id",
            get(handlers::get_record_lineage),
        )
        .route(
            "/lineage/record/:record_id/graph",
            get(handlers::get_lineage_graph),
        )
        // Model impact routes
        .route(
            "/lineage/model/:model_id/impact",
            get(handlers::get_model_impact),
        )
        // Run lineage routes
        .route("/lineage/run/:run_id", get(handlers::get_run_lineage))
        // Time-based query routes
        .route(
            "/lineage/time-range",
            post(handlers::query_lineage_by_time_range),
        )
        // Row-level lineage routes (READ-ONLY for production)
        .route("/lineage/row/:row_key", get(row_handlers::get_row_lineage))
        .route(
            "/lineage/row/:row_key/journey",
            get(row_handlers::get_row_journey),
        )
        .route(
            "/lineage/batch/:batch_id",
            get(row_handlers::get_batch_lineage),
        )
        .route(
            "/lineage/job/:job_id/stats",
            get(row_handlers::get_job_stats),
        )
        .route(
            "/lineage/job/:job_id/filtered",
            get(row_handlers::get_filtered_rows),
        )
        // Column-level lineage routes
        .route(
            "/lineage/column/:table/:column",
            get(column_handlers::get_column_lineage),
        )
        .route(
            "/lineage/column/:table/:column/graph",
            get(column_handlers::get_column_graph),
        )
        .route(
            "/lineage/column/:table/:column/derived",
            get(column_handlers::get_derived_columns),
        )
        .route(
            "/lineage/column/impact-analysis",
            post(column_handlers::analyze_column_impact),
        )
        // Schema evolution routes
        .route(
            "/lineage/schema/change",
            post(schema_handlers::record_schema_change),
        )
        .route(
            "/lineage/schema/datasource/:datasource_id/changes",
            get(schema_handlers::get_datasource_schema_changes),
        )
        .route(
            "/lineage/schema/datasource/:datasource_id/table/:table_name/changes",
            get(schema_handlers::get_table_schema_changes),
        )
        .route(
            "/lineage/schema/version",
            post(schema_handlers::save_schema_version),
        )
        .route(
            "/lineage/schema/datasource/:datasource_id/version/latest",
            get(schema_handlers::get_latest_schema_version),
        )
        .route(
            "/lineage/schema/drift/:source_version/:target_version",
            get(schema_handlers::analyze_schema_drift),
        )
        .route(
            "/lineage/schema/impact",
            post(schema_handlers::analyze_migration_impact),
        );

    // TEST ENDPOINTS (COMPILE-TIME GATED - only available with --features test-endpoints)
    // ⚠️ SECURITY: These endpoints allow external lineage injection and should NEVER be in production!
    // ⚠️ These are EXCLUDED from production builds unless explicitly compiled with test-endpoints feature
    #[cfg(feature = "test-endpoints")]
    let router = router
        .route(
            "/lineage/row/test",
            post(row_handlers::write_row_lineage_event_test),
        )
        .route(
            "/lineage/rows/batch/test",
            post(row_handlers::write_row_lineage_batch_test),
        )
        .route(
            "/lineage/flush/test",
            post(row_handlers::flush_lineage_buffer_test),
        );

    router
}
