//! GDPR API Module
//!
//! REST API endpoints for GDPR compliance:
//! - Article 17: Right to Erasure
//! - Article 20: Right to Data Portability
//!
//! ## Erasure Endpoints (Article 17)
//!
//! ### Tenant-Level Erasure
//! - `POST /api/v1/gdpr/tenants/{tenant_id}/erase` - Erase all data for a tenant
//! - `GET /api/v1/gdpr/tenants/{tenant_id}/count` - Count records for a tenant
//! - `GET /api/v1/gdpr/tenants/{tenant_id}/verify` - Verify erasure completed
//!
//! ### User-Level Erasure (Enhanced)
//! - `POST /api/v1/gdpr/users/{user_id}/erase` - Erase/anonymize all data for a user
//! - `GET /api/v1/gdpr/users/{user_id}/count` - Count records for a user
//! - `GET /api/v1/gdpr/users/{user_id}/legal-holds` - Check legal holds for a user
//!
//! ## Export Endpoints (Article 20)
//!
//! - `POST /api/v1/gdpr/exports` - Request a data export
//! - `GET /api/v1/gdpr/exports/{job_id}` - Get export status
//! - `GET /api/v1/gdpr/exports` - List user's exports
//! - `GET /api/v1/gdpr/exports/{job_id}/download` - Download export file
//! - `DELETE /api/v1/gdpr/exports/{job_id}` - Cancel export job
//!
//! ## Security
//!
//! These endpoints should be heavily protected as they perform sensitive
//! data operations. Ensure proper authentication, authorization,
//! and audit logging are in place.

pub mod export_handlers;
pub mod handlers;
pub mod openapi;
pub mod types;

use crate::api::ApiState;
use axum::{
    routing::{delete, get, post},
    Router,
};
pub use openapi::GdprApiDoc;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

/// Create GDPR router
///
/// Interactive API documentation is available at:
/// - `/api/v1/gdpr/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/gdpr/swagger-ui")
                .url("/gdpr/api-docs/openapi.json", GdprApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // GDPR Article 17: Right to Erasure - Tenant Level
        .route(
            "/tenants/:tenant_id/erase",
            post(handlers::erase_tenant_data),
        )
        .route(
            "/tenants/:tenant_id/count",
            get(handlers::count_tenant_data),
        )
        .route("/tenants/:tenant_id/verify", get(handlers::verify_erasure))
        // GDPR Article 17: Right to Erasure - User Level (Enhanced)
        .route("/users/:user_id/erase", post(handlers::erase_user_data))
        .route("/users/:user_id/count", get(handlers::count_user_data))
        .route(
            "/users/:user_id/legal-holds",
            get(handlers::check_legal_holds),
        )
        // GDPR Article 20: Right to Data Portability
        .route(
            "/exports",
            post(export_handlers::request_export).get(export_handlers::list_user_exports),
        )
        .route(
            "/exports/:job_id",
            get(export_handlers::get_export_status).delete(export_handlers::cancel_export),
        )
        .route(
            "/exports/:job_id/download",
            get(export_handlers::download_export),
        )
}
