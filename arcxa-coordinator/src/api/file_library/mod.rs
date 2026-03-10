//! File Library API Module
//!
//! Enterprise-scale file management system for CSV/TSV/Excel files.
//!
//! Features:
//! - Bulk file upload and import
//! - Hierarchical folder organization
//! - Tag-based categorization
//! - Advanced search and filtering
//! - Schema detection and caching
//! - PII detection and governance
//! - Lineage tracking and impact analysis

pub mod handlers;
pub mod lineage;
pub mod mapping;
pub mod migration;
pub mod openapi;
pub mod scanner;
pub mod storage;
pub mod storage_rocksdb;
pub mod storage_trait;
pub mod types;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

pub use handlers::*;
pub use lineage::*;
pub use mapping::*;
pub use openapi::FileLibraryApiDoc;
pub use scanner::*;
pub use storage::*;
pub use types::*;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

/// Create File Library router with all endpoints
///
/// All routes are nested under `/file-library` prefix, which is then
/// mounted under `/api/v1` in the main router, resulting in:
/// - `/api/v1/file-library/files`
/// - `/api/v1/file-library/folders`
/// - etc.
///
/// Interactive API documentation is available at:
/// - `/api/v1/file-library/swagger-ui`
pub fn create_router() -> Router<Arc<crate::api::ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/file-library/swagger-ui")
                .url(
                    "/file-library/api-docs/openapi.json",
                    FileLibraryApiDoc::openapi(),
                )
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // File operations
        .route("/file-library/files", get(handlers::list_files))
        .route("/file-library/files", post(handlers::create_file))
        .route("/file-library/files/:id", get(handlers::get_file))
        .route("/file-library/files/:id", put(handlers::update_file))
        .route("/file-library/files/:id", delete(handlers::delete_file))
        .route("/file-library/files/:id/scan", post(handlers::scan_file))
        .route(
            "/file-library/files/:id/validate-registration",
            get(handlers::validate_file_for_registration),
        )
        .route(
            "/file-library/files/:id/download",
            get(handlers::download_file),
        )
        .route(
            "/file-library/files/:id/preview",
            get(handlers::preview_file),
        )
        // Field mapping
        .route(
            "/file-library/files/:source_id/suggest-mappings/:target_id",
            post(mapping::suggest_csv_mappings),
        )
        // Bulk operations
        .route(
            "/file-library/files/bulk-upload",
            post(handlers::bulk_upload),
        )
        .route(
            "/file-library/files/bulk-update",
            put(handlers::bulk_update),
        )
        .route(
            "/file-library/files/bulk-delete",
            delete(handlers::bulk_delete),
        )
        .route("/file-library/files/bulk-scan", post(handlers::bulk_scan))
        .route("/file-library/jobs/:job_id", get(handlers::get_job_status))
        .route(
            "/file-library/scan-jobs/:job_id",
            get(handlers::get_job_status),
        ) // Alias for bulk-scan jobs
        // Folder operations
        .route("/file-library/folders", get(handlers::list_folders))
        .route("/file-library/folders", post(handlers::create_folder))
        .route("/file-library/folders/:id", put(handlers::update_folder))
        .route("/file-library/folders/:id", delete(handlers::delete_folder))
        // Search & Tags
        .route("/file-library/tags", get(handlers::list_tags))
        .route("/file-library/search", post(handlers::search_files))
        // Lineage & Analytics
        .route(
            "/file-library/files/:id/lineage",
            get(handlers::get_file_lineage),
        )
        .route(
            "/file-library/files/:id/impact-analysis",
            get(handlers::get_impact_analysis),
        )
        .route(
            "/file-library/files/:id/usage-stats",
            get(handlers::get_file_usage_stats),
        )
        .route("/file-library/stats", get(handlers::get_library_stats))
        // Admin operations
        .route(
            "/file-library/admin/recover-files",
            post(handlers::recover_orphaned_files),
        )
}
