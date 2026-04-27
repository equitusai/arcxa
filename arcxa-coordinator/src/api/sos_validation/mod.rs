//! Systems-of-Systems (SoS) Validation API
//!
//! This module provides REST API endpoints for validating compatibility and integration
//! between systems in a Systems-of-Systems architecture.
//!
//! ## Key Features
//!
//! - **System Management**: Register and manage systems with their capabilities
//! - **Interface Definition**: Define system interfaces with schemas and coordinate systems
//! - **Data Contracts**: Create contracts between interfaces with SLA requirements
//! - **Cross-System Validation**: Validate compatibility, policies, and SLA compliance
//! - **Analytics**: Compatibility matrix, dependency graphs, what-if analysis
//!
//! ## Endpoints
//!
//! ### System Management
//! - `POST /api/v1/sos/systems` - Register a new system
//! - `GET /api/v1/sos/systems` - List all systems
//! - `GET /api/v1/sos/systems/{id}` - Get system by ID
//! - `PUT /api/v1/sos/systems/{id}` - Update system
//! - `DELETE /api/v1/sos/systems/{id}` - Delete system
//! - `GET /api/v1/sos/systems/{id}/interfaces` - List system interfaces
//!
//! ### Interface Definition
//! - `POST /api/v1/sos/interfaces` - Register an interface
//! - `GET /api/v1/sos/interfaces` - List all interfaces
//! - `GET /api/v1/sos/interfaces/{id}` - Get interface by ID
//! - `PUT /api/v1/sos/interfaces/{id}` - Update interface
//! - `DELETE /api/v1/sos/interfaces/{id}` - Delete interface
//! - `POST /api/v1/sos/interfaces/{id}/validate-schema` - Validate data against schema
//!
//! ### Data Contracts
//! - `POST /api/v1/sos/contracts` - Create a data contract
//! - `GET /api/v1/sos/contracts` - List all contracts
//! - `GET /api/v1/sos/contracts/lookup` - Get contract by provider/consumer interface pair
//! - `GET /api/v1/sos/contracts/{id}` - Get contract by ID
//! - `PUT /api/v1/sos/contracts/{id}` - Update contract
//! - `DELETE /api/v1/sos/contracts/{id}` - Delete contract
//! - `POST /api/v1/sos/contracts/{id}/approval-requests` - Open a contract approval request
//! - `GET /api/v1/sos/contracts/{id}/approval-requests` - List contract approval requests
//! - `GET /api/v1/sos/contracts/{id}/approval-requests/{request_id}` - Get contract approval request
//! - `POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/evidence` - Attach contract approval evidence
//! - `POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/approve` - Approve contract approval request
//! - `POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/reject` - Reject contract approval request
//! - `POST /api/v1/sos/contracts/{id}/approve` - Approve contract
//! - `POST /api/v1/sos/contracts/{id}/sign` - Sign contract
//!
//! ### Validation
//! - `POST /api/v1/sos/validate` - Execute validation
//! - `POST /api/v1/sos/validate/dry-run` - Dry-run validation
//! - `POST /api/v1/sos/policies` - Create a persisted policy
//! - `GET /api/v1/sos/policies` - List persisted policies
//! - `GET /api/v1/sos/policies/{id}` - Get policy by ID
//! - `PUT /api/v1/sos/policies/{id}` - Update policy
//! - `DELETE /api/v1/sos/policies/{id}` - Delete policy
//! - `POST /api/v1/sos/policies/{id}/validate` - Evaluate a persisted policy
//! - `POST /api/v1/sos/policies/{id}/validate/dry-run` - Dry-run a persisted policy
//! - `GET /api/v1/sos/validation-reports/{report_id}` - Get persisted validation report
//! - `GET /api/v1/sos/validation-history` - Get validation history for a normalized subject
//! - `GET /api/v1/sos/validation-lineage` - Traverse validation report lineage
//!
//! ### Analytics
//! - `GET /api/v1/sos/compatibility-matrix` - Get compatibility matrix
//! - `GET /api/v1/sos/dependency-graph` - Get dependency graph
//! - `POST /api/v1/sos/what-if` - What-if analysis
//! - `POST /api/v1/sos/reconcile` - Explicitly rerun SoS ontology/graph reconcile
//!
//! ## Usage Example
//!
//! ```bash
//! # Register a radar system
//! curl -X POST http://localhost:8080/api/v1/sos/systems \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "system_id": "radar-system-v1",
//!     "system_name": "AN/TPY-2 X-Band Radar",
//!     "system_type": "radar.ground_based",
//!     "vendor": "Raytheon",
//!     "version": "1.0",
//!     "classification": "SECRET",
//!     "deployment": {"location": "Alaska", "status": "operational"},
//!     "capabilities": {"range_km": 1000, "track_capacity": 100},
//!     "tags": ["radar", "missile_defense"]
//!   }'
//!
//! # Register a radar output interface
//! curl -X POST http://localhost:8080/api/v1/sos/interfaces \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "system_id": "radar-system-v1",
//!     "interface_id": "radar-target-interface",
//!     "interface_name": "Target Track Output",
//!     "direction": "outbound",
//!     "protocol": "REST",
//!     "data_format": "JSON",
//!     "schema": {"type": "object", "properties": {...}},
//!     "coordinate_system": "WGS84",
//!     "unit_system": "SI"
//!   }'
//!
//! # Validate interface compatibility
//! curl -X POST http://localhost:8080/api/v1/sos/validate \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "type": "interface_compatibility",
//!     "provider_interface_id": "radar-target-interface",
//!     "consumer_interface_id": "missile-target-interface"
//!   }'
//! ```

pub mod handlers;
pub mod openapi;
pub mod types;

// Submodules for implementation (to be created)
pub(crate) mod contract_governance;
pub(crate) mod contract_signature;
pub mod integration;
pub(crate) mod policy_attestation;
pub mod storage;
pub mod validators;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::ApiState;
pub use openapi::SosValidationApiDoc;
pub use policy_attestation::PolicyAttestationSigningMaterial;

/// Create SoS validation API router
///
/// Interactive API documentation is available at:
/// - `/api/v1/sos/swagger-ui`
pub fn create_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Swagger UI for interactive API documentation
        .merge(
            SwaggerUi::new("/sos/swagger-ui")
                .url(
                    "/api/v1/sos/swagger-ui/api-docs/openapi.json",
                    SosValidationApiDoc::openapi(),
                )
                .config(Config::new([
                    "/api/v1/sos/swagger-ui/api-docs/openapi.json",
                ])),
        )
        // ===== System Management Routes =====
        .route("/sos/systems", post(handlers::register_system))
        .route("/sos/systems", get(handlers::list_systems))
        .route("/sos/systems/:id", get(handlers::get_system))
        .route("/sos/systems/:id", put(handlers::update_system))
        .route("/sos/systems/:id", delete(handlers::delete_system))
        .route(
            "/sos/systems/:id/interfaces",
            get(handlers::list_system_interfaces),
        )
        // ===== Interface Definition Routes =====
        .route("/sos/interfaces", post(handlers::register_interface))
        .route("/sos/interfaces", get(handlers::list_interfaces))
        .route("/sos/interfaces/:id", get(handlers::get_interface))
        .route("/sos/interfaces/:id", put(handlers::update_interface))
        .route("/sos/interfaces/:id", delete(handlers::delete_interface))
        .route(
            "/sos/interfaces/:id/validate-schema",
            post(handlers::validate_interface_schema),
        )
        // ===== Data Contract Routes =====
        .route("/sos/contracts", post(handlers::create_contract))
        .route("/sos/contracts", get(handlers::list_contracts))
        .route("/sos/contracts/lookup", get(handlers::lookup_contract))
        .route(
            "/sos/contracts/signing-key",
            get(handlers::get_contract_signing_key_status),
        )
        .route(
            "/sos/contracts/signing-key/rotate",
            post(handlers::rotate_contract_signing_key),
        )
        .route("/sos/contracts/:id", get(handlers::get_contract))
        .route("/sos/contracts/:id", put(handlers::update_contract))
        .route("/sos/contracts/:id", delete(handlers::delete_contract))
        .route(
            "/sos/contracts/:id/signatures",
            get(handlers::list_contract_signatures),
        )
        .route(
            "/sos/contracts/:id/approval-requests",
            post(handlers::create_contract_approval_request),
        )
        .route(
            "/sos/contracts/:id/approval-requests",
            get(handlers::list_contract_approval_requests),
        )
        .route(
            "/sos/contracts/:id/approval-requests/:request_id",
            get(handlers::get_contract_approval_request),
        )
        .route(
            "/sos/contracts/:id/approval-requests/:request_id/evidence",
            post(handlers::add_contract_approval_evidence),
        )
        .route(
            "/sos/contracts/:id/approval-requests/:request_id/approve",
            post(handlers::approve_contract_approval_request),
        )
        .route(
            "/sos/contracts/:id/approval-requests/:request_id/reject",
            post(handlers::reject_contract_approval_request),
        )
        .route(
            "/sos/contracts/:id/approve",
            post(handlers::approve_contract),
        )
        .route("/sos/contracts/:id/sign", post(handlers::sign_contract))
        // ===== Validation Routes =====
        .route("/sos/validate", post(handlers::validate))
        .route("/sos/validate/dry-run", post(handlers::validate_dry_run))
        .route("/sos/policies", post(handlers::create_policy))
        .route("/sos/policies", get(handlers::list_policies))
        .route(
            "/sos/policies/signing-key",
            get(handlers::get_policy_signing_key_status),
        )
        .route(
            "/sos/policies/signing-key/rotate",
            post(handlers::rotate_policy_signing_key),
        )
        .route("/sos/policies/:id", get(handlers::get_policy))
        .route("/sos/policies/:id", put(handlers::update_policy))
        .route("/sos/policies/:id", delete(handlers::delete_policy))
        .route(
            "/sos/policies/:id/attestations",
            get(handlers::list_policy_attestations),
        )
        .route(
            "/sos/policies/:id/approval-requests",
            post(handlers::create_policy_approval_request),
        )
        .route(
            "/sos/policies/:id/approval-requests",
            get(handlers::list_policy_approval_requests),
        )
        .route(
            "/sos/policies/:id/approval-requests/:request_id",
            get(handlers::get_policy_approval_request),
        )
        .route(
            "/sos/policies/:id/approval-requests/:request_id/evidence",
            post(handlers::add_policy_approval_evidence),
        )
        .route(
            "/sos/policies/:id/approval-requests/:request_id/approve",
            post(handlers::approve_policy_approval_request),
        )
        .route(
            "/sos/policies/:id/approval-requests/:request_id/reject",
            post(handlers::reject_policy_approval_request),
        )
        .route("/sos/policies/:id/approve", post(handlers::approve_policy))
        .route("/sos/policies/:id/reject", post(handlers::reject_policy))
        .route(
            "/sos/policies/:id/validate",
            post(handlers::validate_policy),
        )
        .route(
            "/sos/policies/:id/validate/dry-run",
            post(handlers::validate_policy_dry_run),
        )
        .route(
            "/sos/validation-reports/:report_id",
            get(handlers::get_validation_report),
        )
        .route(
            "/sos/validation-history",
            get(handlers::get_validation_history),
        )
        .route(
            "/sos/validation-lineage",
            get(handlers::get_validation_lineage),
        )
        // ===== Analytics Routes =====
        .route(
            "/sos/compatibility-matrix",
            get(handlers::get_compatibility_matrix),
        )
        .route("/sos/dependency-graph", get(handlers::get_dependency_graph))
        .route("/sos/what-if", post(handlers::what_if_analysis))
        .route("/sos/reconcile", post(handlers::reconcile_sos_runtime))
}
