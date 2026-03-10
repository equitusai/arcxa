//! Custom ontology management API
//!
//! This module provides REST API endpoints for managing custom domain ontologies.
//!
//! ## Endpoints
//!
//! - `POST /api/v1/ontology` - Register a new custom ontology
//! - `GET /api/v1/ontology` - List all ontologies
//! - `GET /api/v1/ontology/:id` - Get ontology by ID
//! - `PUT /api/v1/ontology/:id` - Update an existing ontology
//! - `DELETE /api/v1/ontology/:id` - Delete/deactivate an ontology
//! - `POST /api/v1/ontology/merge` - Get merged ontology
//! - `POST /api/v1/ontology/validate` - Validate ontology syntax
//!
//! ## Usage Example
//!
//! ```bash
//! # Register a custom retail ontology
//! curl -X POST http://localhost:8080/api/v1/ontology \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "id": "retail_domain",
//!     "name": "Retail Domain Ontology",
//!     "description": "Ontology for retail business domain",
//!     "content": "@prefix retail: <http://example.com/retail#> .\nretail:Product a rdfs:Class .",
//!     "namespace": "http://example.com/retail#",
//!     "tags": ["retail", "e-commerce"]
//!   }'
//!
//! # Get merged ontology (base + extensions + all active custom)
//! curl -X POST http://localhost:8080/api/v1/ontology/merge \
//!   -H "Content-Type: application/json" \
//!   -d '{}'
//!
//! # List all ontologies
//! curl http://localhost:8080/api/v1/ontology?active_only=true
//! ```

pub mod handlers;
pub mod openapi;
pub mod tree_builder;
pub mod types;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;
pub use openapi::OntologyApiDoc;

/// Create ontology API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/ontology/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/ontology/swagger-ui")
                .url("/ontology/api-docs/openapi.json", OntologyApiDoc::openapi())
                .config(Config::new(["../api-docs/openapi.json"])),
        )
        // Register new ontology
        .route("/ontology", post(handlers::register_ontology))
        // List ontologies
        .route("/ontology", get(handlers::list_ontologies))
        // Get ontology by ID
        .route("/ontology/:id", get(handlers::get_ontology))
        // Get ontology tree structure
        .route("/ontology/:id/tree", get(handlers::get_ontology_tree))
        // Update ontology
        .route("/ontology/:id", put(handlers::update_ontology))
        // Activate ontology
        .route("/ontology/:id/activate", post(handlers::activate_ontology))
        // Deactivate ontology
        .route("/ontology/:id/deactivate", post(handlers::delete_ontology))
        // Delete ontology
        .route("/ontology/:id", delete(handlers::delete_ontology))
        // Get merged ontology
        .route("/ontology/merge", post(handlers::get_merged_ontology))
        // Validate ontology
        .route("/ontology/validate", post(handlers::validate_ontology))
}

// Note: Integration tests are in tests/ontology_api_test.rs
// Unit tests for handlers are covered by the catalog-level OntologyRegistry tests
