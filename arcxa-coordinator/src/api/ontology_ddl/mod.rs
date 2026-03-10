//! Ontology-Driven DDL API Module
//!
//! REST API endpoints for ontology-driven DDL generation (GAP-002 Phase 3).

pub mod handlers;
pub mod types;

pub use handlers::*;
pub use types::*;

use crate::api::ApiState;
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

/// Create the ontology-driven DDL router
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route(
            "/ontology-ddl/generate",
            post(handlers::generate_ontology_ddl_handler),
        )
        .route(
            "/ontology-ddl/analyze",
            post(handlers::analyze_ontology_mappings_handler),
        )
        .route(
            "/ontology-ddl/config/default",
            get(handlers::get_default_config_handler),
        )
}
