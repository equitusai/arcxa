mod gateway;
mod handlers;
mod types;

pub use gateway::{
    MigrationEvidenceEventBusConfig, MigrationEvidenceGateway, MigrationEvidenceGatewayConfig,
    MigrationEvidenceRemoteGatewayConfig,
};
pub use types::*;

use axum::{routing::{get, post}, Router};
use std::sync::Arc;

use super::ApiState;

pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/migration-evidence/connectors", post(handlers::upsert_connector))
        .route("/migration-evidence/connectors/:id/runs", post(handlers::run_connector))
        .route("/migration-evidence/values/explain", get(handlers::explain_value))
        .route(
            "/migration-evidence/objects/:id/evidence-packet",
            get(handlers::get_evidence_packet),
        )
        .route(
            "/migration-evidence/objects/:id/controls",
            get(handlers::get_object_controls),
        )
        .route(
            "/migration-evidence/programs/:id/exceptions",
            get(handlers::get_program_exceptions),
        )
        .route(
            "/migration-evidence/programs/:id/approvals",
            get(handlers::get_program_approvals),
        )
        .route(
            "/migration-evidence/runtime/status",
            get(handlers::get_runtime_status),
        )
        .route(
            "/migration-evidence/runtime/rebuild",
            post(handlers::rebuild_read_models),
        )
}
