//! Profiling API Module
//!
//! REST API endpoints for source profiling and RDF metadata management.

pub mod handlers;
pub mod types;

pub use handlers::{get_profile, list_profiles, profile_dataset};
pub use types::{GetProfileResponse, ProfileDatasetRequest, ProfileDatasetResponse};

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

/// Create profiling API router
pub fn create_router() -> Router<Arc<crate::api::ApiState>> {
    Router::new()
        // POST /api/v1/profiling/profile - Profile a CSV/Parquet dataset
        .route("/profiling/profile", post(handlers::profile_dataset))
        // GET /api/v1/profiling/profiles - List all profiled datasets
        .route("/profiling/profiles", get(handlers::list_profiles))
        // GET /api/v1/profiling/profiles/:dataset_id - Get specific profile
        .route(
            "/profiling/profiles/:dataset_id",
            get(handlers::get_profile),
        )
}
