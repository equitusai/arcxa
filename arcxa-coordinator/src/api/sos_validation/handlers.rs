//! REST API handlers for Systems-of-Systems (SoS) validation
//!
//! This module implements HTTP handlers for:
//! - System registration and management
//! - Interface definition and schema validation
//! - Data contract creation and approval
//! - Cross-system validation (compatibility, policies, SLAs)
//! - Analytics (compatibility matrix, dependency graphs)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use std::{collections::HashMap, sync::Arc};

use super::contract_governance::{
    effective_contract_lifecycle_state, set_contract_draft, set_contract_signed,
    CONTRACT_LIFECYCLE_SIGNED,
};
use super::contract_signature::{
    build_contract_signature_record, contract_signing_key_public_material,
};
use super::integration::{
    ensure_interface_ontology_assets, reconcile_sos_ontology_assets, SosValidationService,
    SosValidationServiceError, ValidationExecutionOptions,
};
use super::policy_attestation::{
    policy_signing_key_public_material, PolicyAttestationSigningMaterial,
    POLICY_ATTESTATION_DEFAULT_TRUST_MODE, POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY, POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
};
use super::types::*;
use crate::api::{auth::Claims, ApiState};
use ed25519_dalek::SigningKey;
use graphica_core::secrets::{get_secret_by_ref, put_secret_by_ref, SecretMetadata, SecretValue};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;

// ============================================================================
// System Management Handlers
// ============================================================================

/// Register a new system in the SoS catalog
///
/// POST /api/v1/sos/systems
#[utoipa::path(
    post,
    path = "/api/v1/sos/systems",
    request_body = RegisterSystemRequest,
    responses(
        (status = 200, description = "System registered successfully", body = SystemResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 409, description = "System ID already exists", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn register_system(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterSystemRequest>,
) -> Result<Json<SystemResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Validate request fields
    if request.system_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "system_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.system_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "system_name cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // 2. Check if system_id already exists
    match storage_manager.get_system(&request.system_id) {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(SosErrorResponse {
                    error: "SYSTEM_EXISTS".to_string(),
                    message: format!("System with ID '{}' already exists", request.system_id),
                    details: None,
                }),
            ));
        }
        Ok(None) => {
            // System doesn't exist, we can proceed
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check system existence: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // 3. Create System entity and store in RocksDB
    use super::storage::System;
    use chrono::Utc;

    let now = Utc::now();
    let system = System {
        system_id: request.system_id.clone(),
        system_name: request.system_name.clone(),
        system_type: request.system_type.clone(),
        vendor: request.vendor.clone(),
        version: request.version.clone(),
        classification: request.classification.clone(),
        description: request.description.clone(),
        deployment: request.deployment.clone(),
        capabilities: request.capabilities.clone(),
        tags: request.tags.clone(),
        active: true,
        created_at: now,
        updated_at: now,
    };

    if let Err(e) = storage_manager.put_system(&system) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to store system: {}", e),
                details: None,
            }),
        ));
    }

    project_system_upsert(&state, &system)?;

    // 5. Return SystemResponse
    Ok(Json(SystemResponse {
        system_id: system.system_id,
        system_name: system.system_name,
        system_type: system.system_type,
        vendor: system.vendor,
        version: system.version,
        classification: system.classification,
        description: system.description,
        deployment: system.deployment,
        capabilities: system.capabilities,
        tags: system.tags,
        active: system.active,
        created_at: system.created_at.to_rfc3339(),
        updated_at: system.updated_at.to_rfc3339(),
    }))
}

/// List all systems with optional filters
///
/// GET /api/v1/sos/systems
#[utoipa::path(
    get,
    path = "/api/v1/sos/systems",
    params(ListSystemsQuery),
    responses(
        (status = 200, description = "Systems retrieved successfully", body = ListSystemsResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn list_systems(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListSystemsQuery>,
) -> Result<Json<ListSystemsResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Query RocksDB systems column family
    let systems = if let Some(ref system_type) = query.system_type {
        // Use optimized secondary index for system_type queries
        match storage_manager.list_systems_by_type(system_type, query.limit + query.offset) {
            Ok(systems) => systems,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SosErrorResponse {
                        error: "DATABASE_ERROR".to_string(),
                        message: format!("Failed to query systems by type: {}", e),
                        details: None,
                    }),
                ));
            }
        }
    } else {
        // Full scan with pagination
        match storage_manager.list_all_systems(query.offset, query.limit) {
            Ok(systems) => systems,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SosErrorResponse {
                        error: "DATABASE_ERROR".to_string(),
                        message: format!("Failed to query systems: {}", e),
                        details: None,
                    }),
                ));
            }
        }
    };

    // 2. Apply additional filters (vendor, classification, tags, active)
    let mut filtered_systems: Vec<_> = systems
        .into_iter()
        .filter(|system| {
            // Filter by vendor if specified
            if let Some(ref vendor) = query.vendor {
                if &system.vendor != vendor {
                    return false;
                }
            }

            // Filter by classification if specified
            if let Some(ref classification) = query.classification {
                if &system.classification != classification {
                    return false;
                }
            }

            // Filter by active status if specified
            if let Some(active) = query.active {
                if system.active != active {
                    return false;
                }
            }

            // Filter by tags if specified
            if let Some(ref tags_str) = query.tags {
                let required_tags: Vec<&str> = tags_str.split(',').map(|t| t.trim()).collect();
                if !required_tags
                    .iter()
                    .all(|tag| system.tags.iter().any(|t| t == tag))
                {
                    return false;
                }
            }

            true
        })
        .collect();

    // 3. Apply pagination (if not already applied by list_systems_by_type)
    let total = filtered_systems.len();
    if query.system_type.is_none() {
        // Pagination already applied in list_all_systems
    } else {
        // Need to apply offset/limit to filtered results
        filtered_systems = filtered_systems
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();
    }

    // 4. Convert to SystemResponse
    let response_systems: Vec<SystemResponse> = filtered_systems
        .into_iter()
        .map(|system| SystemResponse {
            system_id: system.system_id,
            system_name: system.system_name,
            system_type: system.system_type,
            vendor: system.vendor,
            version: system.version,
            classification: system.classification,
            description: system.description,
            deployment: system.deployment,
            capabilities: system.capabilities,
            tags: system.tags,
            active: system.active,
            created_at: system.created_at.to_rfc3339(),
            updated_at: system.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ListSystemsResponse {
        systems: response_systems,
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}

/// Get a specific system by ID
///
/// GET /api/v1/sos/systems/{id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/systems/{id}",
    params(
        ("id" = String, Path, description = "System ID")
    ),
    responses(
        (status = 200, description = "System retrieved successfully", body = SystemResponse),
        (status = 404, description = "System not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn get_system(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<SystemResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Query RocksDB for system_id
    match storage_manager.get_system(&id) {
        Ok(Some(system)) => {
            // 3. Return SystemResponse
            Ok(Json(SystemResponse {
                system_id: system.system_id,
                system_name: system.system_name,
                system_type: system.system_type,
                vendor: system.vendor,
                version: system.version,
                classification: system.classification,
                description: system.description,
                deployment: system.deployment,
                capabilities: system.capabilities,
                tags: system.tags,
                active: system.active,
                created_at: system.created_at.to_rfc3339(),
                updated_at: system.updated_at.to_rfc3339(),
            }))
        }
        Ok(None) => {
            // 2. Return 404 if not found
            Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "SYSTEM_NOT_FOUND".to_string(),
                    message: format!("System with ID '{}' not found", id),
                    details: None,
                }),
            ))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve system: {}", e),
                details: None,
            }),
        )),
    }
}

/// Update an existing system
///
/// PUT /api/v1/sos/systems/{id}
#[utoipa::path(
    put,
    path = "/api/v1/sos/systems/{id}",
    params(
        ("id" = String, Path, description = "System ID")
    ),
    request_body = UpdateSystemRequest,
    responses(
        (status = 200, description = "System updated successfully", body = SystemResponse),
        (status = 404, description = "System not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn update_system(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSystemRequest>,
) -> Result<Json<SystemResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Validate system exists and retrieve it
    let mut system = match storage_manager.get_system(&id) {
        Ok(Some(sys)) => sys,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "SYSTEM_NOT_FOUND".to_string(),
                    message: format!("System with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve system: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // 2. Apply updates (only non-None fields - partial update support)
    use chrono::Utc;

    if let Some(system_name) = request.system_name {
        system.system_name = system_name;
    }

    if let Some(version) = request.version {
        system.version = version;
    }

    if let Some(classification) = request.classification {
        system.classification = classification;
    }

    if let Some(description) = request.description {
        system.description = Some(description);
    }

    if let Some(deployment) = request.deployment {
        system.deployment = deployment;
    }

    if let Some(capabilities) = request.capabilities {
        system.capabilities = capabilities;
    }

    if let Some(tags) = request.tags {
        system.tags = tags;
    }

    if let Some(active) = request.active {
        system.active = active;
    }

    // Always update the updated_at timestamp
    system.updated_at = Utc::now();

    // 3. Store updated system in RocksDB
    if let Err(e) = storage_manager.put_system(&system) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to update system: {}", e),
                details: None,
            }),
        ));
    }

    project_system_upsert(&state, &system)?;

    // 4. Return updated SystemResponse
    Ok(Json(SystemResponse {
        system_id: system.system_id,
        system_name: system.system_name,
        system_type: system.system_type,
        vendor: system.vendor,
        version: system.version,
        classification: system.classification,
        description: system.description,
        deployment: system.deployment,
        capabilities: system.capabilities,
        tags: system.tags,
        active: system.active,
        created_at: system.created_at.to_rfc3339(),
        updated_at: system.updated_at.to_rfc3339(),
    }))
}

/// Delete a system
///
/// DELETE /api/v1/sos/systems/{id}
#[utoipa::path(
    delete,
    path = "/api/v1/sos/systems/{id}",
    params(
        ("id" = String, Path, description = "System ID")
    ),
    responses(
        (status = 204, description = "System deleted successfully"),
        (status = 404, description = "System not found", body = SosErrorResponse),
        (status = 409, description = "System has active contracts", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn delete_system(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Verify system exists (and get its type/vendor for deletion)
    let system = match storage_manager.get_system(&id) {
        Ok(Some(sys)) => sys,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "SYSTEM_NOT_FOUND".to_string(),
                    message: format!("System with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve system: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // Check for dependent interfaces (prevent deletion if system has interfaces)
    match storage_manager.list_interfaces_by_system(&id) {
        Ok(interfaces) => {
            if !interfaces.is_empty() {
                return Err((
                    StatusCode::CONFLICT,
                    Json(SosErrorResponse {
                        error: "SYSTEM_HAS_INTERFACES".to_string(),
                        message: format!(
                            "Cannot delete system '{}': system has {} interface(s). Delete interfaces first.",
                            id,
                            interfaces.len()
                        ),
                        details: Some(serde_json::json!({
                            "interface_count": interfaces.len(),
                            "interface_ids": interfaces.iter().map(|i| &i.interface_id).collect::<Vec<_>>(),
                        })),
                    }),
                ));
            }
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check interfaces: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // Delete system from RocksDB
    match storage_manager.delete_system(&system.system_id, &system.system_type, &system.vendor) {
        Ok(_) => {
            project_system_delete(&state, &system.system_id)?;

            // Return 204 No Content (successful deletion)
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to delete system: {}", e),
                details: None,
            }),
        )),
    }
}

/// List all interfaces for a system
///
/// GET /api/v1/sos/systems/{id}/interfaces
#[utoipa::path(
    get,
    path = "/api/v1/sos/systems/{id}/interfaces",
    params(
        ("id" = String, Path, description = "System ID")
    ),
    responses(
        (status = 200, description = "Interfaces retrieved successfully", body = Vec<InterfaceResponse>),
        (status = 404, description = "System not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - System Management"
)]
pub async fn list_system_interfaces(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<InterfaceResponse>>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Verify system exists
    match storage_manager.get_system(&id) {
        Ok(Some(_)) => {
            // System exists, proceed
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "SYSTEM_NOT_FOUND".to_string(),
                    message: format!("System with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check system existence: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // Query interfaces for this system
    match storage_manager.list_interfaces_by_system(&id) {
        Ok(interfaces) => {
            let response_interfaces: Vec<InterfaceResponse> = interfaces
                .into_iter()
                .map(|interface| InterfaceResponse {
                    system_id: interface.system_id,
                    interface: SystemInterface {
                        interface_id: interface.interface_id,
                        interface_name: interface.interface_name,
                        direction: interface.direction,
                        protocol: interface.protocol,
                        data_format: interface.data_format,
                        schema: interface.schema,
                        coordinate_system: interface.coordinate_system,
                        unit_system: interface.unit_system,
                        metadata: interface.metadata,
                    },
                    created_at: interface.created_at.to_rfc3339(),
                    updated_at: interface.updated_at.to_rfc3339(),
                })
                .collect();

            Ok(Json(response_interfaces))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve interfaces: {}", e),
                details: None,
            }),
        )),
    }
}

// ============================================================================
// Interface Definition Handlers
// ============================================================================

/// Register a system interface
///
/// POST /api/v1/sos/interfaces
#[utoipa::path(
    post,
    path = "/api/v1/sos/interfaces",
    request_body = RegisterInterfaceRequest,
    responses(
        (status = 200, description = "Interface registered successfully", body = InterfaceResponse),
        (status = 400, description = "Invalid request or schema", body = SosErrorResponse),
        (status = 404, description = "System not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn register_interface(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterInterfaceRequest>,
) -> Result<Json<InterfaceResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Interface;
    use super::validators::{validate_data_format, validate_direction, validate_protocol};
    use chrono::Utc;

    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Validate required fields
    if request.interface.interface_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "interface_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.interface.interface_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "interface_name cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.system_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "system_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // 2. Validate system exists
    match storage_manager.get_system(&request.system_id) {
        Ok(Some(_)) => {
            // System exists, proceed
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "SYSTEM_NOT_FOUND".to_string(),
                    message: format!("System with ID '{}' not found", request.system_id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check system existence: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // 3. Check if interface_id already exists
    match storage_manager.get_interface(&request.interface.interface_id) {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(SosErrorResponse {
                    error: "INTERFACE_EXISTS".to_string(),
                    message: format!(
                        "Interface with ID '{}' already exists",
                        request.interface.interface_id
                    ),
                    details: None,
                }),
            ));
        }
        Ok(None) => {
            // Interface doesn't exist, we can proceed
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check interface existence: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // 4. Validate enum fields
    if let Err(e) = validate_direction(&request.interface.direction) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_DIRECTION".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    if let Err(e) = validate_protocol(&request.interface.protocol) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_PROTOCOL".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    if let Err(e) = validate_data_format(&request.interface.data_format) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_DATA_FORMAT".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    // 5. Create Interface entity and store in RocksDB
    let now = Utc::now();
    let mut interface = Interface {
        interface_id: request.interface.interface_id.clone(),
        system_id: request.system_id.clone(),
        interface_name: request.interface.interface_name.clone(),
        direction: request.interface.direction.clone(),
        protocol: request.interface.protocol.clone(),
        data_format: request.interface.data_format.clone(),
        schema: request.interface.schema.clone(),
        unit_system: request.interface.unit_system.clone(),
        coordinate_system: request.interface.coordinate_system.clone(),
        metadata: request.interface.metadata.clone(),
        created_at: now,
        updated_at: now,
    };

    if let Err(error) = ensure_interface_ontology_assets(
        &mut interface,
        state.persisted_ontology_registry.as_deref(),
    )
    .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "ONTOLOGY_REGISTRATION_ERROR".to_string(),
                message: format!("Failed to register interface ontology assets: {}", error),
                details: None,
            }),
        ));
    }

    if let Err(e) = storage_manager.put_interface(&interface) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to store interface: {}", e),
                details: None,
            }),
        ));
    }

    project_interface_upsert(&state, &interface)?;

    // 7. Return InterfaceResponse
    Ok(Json(InterfaceResponse {
        system_id: interface.system_id,
        interface: SystemInterface {
            interface_id: interface.interface_id,
            interface_name: interface.interface_name,
            direction: interface.direction,
            protocol: interface.protocol,
            data_format: interface.data_format,
            schema: interface.schema,
            coordinate_system: interface.coordinate_system,
            unit_system: interface.unit_system,
            metadata: interface.metadata,
        },
        created_at: interface.created_at.to_rfc3339(),
        updated_at: interface.updated_at.to_rfc3339(),
    }))
}

/// List all interfaces
///
/// GET /api/v1/sos/interfaces
#[utoipa::path(
    get,
    path = "/api/v1/sos/interfaces",
    responses(
        (status = 200, description = "Interfaces retrieved successfully", body = Vec<InterfaceResponse>),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn list_interfaces(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<Vec<InterfaceResponse>>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Query all interfaces with default pagination
    // TODO: Add query parameters for offset/limit/filtering
    let offset = 0;
    let limit = 100;

    match storage_manager.list_all_interfaces(offset, limit) {
        Ok(interfaces) => {
            let response_interfaces: Vec<InterfaceResponse> = interfaces
                .into_iter()
                .map(|interface| InterfaceResponse {
                    system_id: interface.system_id,
                    interface: SystemInterface {
                        interface_id: interface.interface_id,
                        interface_name: interface.interface_name,
                        direction: interface.direction,
                        protocol: interface.protocol,
                        data_format: interface.data_format,
                        schema: interface.schema,
                        coordinate_system: interface.coordinate_system,
                        unit_system: interface.unit_system,
                        metadata: interface.metadata,
                    },
                    created_at: interface.created_at.to_rfc3339(),
                    updated_at: interface.updated_at.to_rfc3339(),
                })
                .collect();

            Ok(Json(response_interfaces))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve interfaces: {}", e),
                details: None,
            }),
        )),
    }
}

/// Get a specific interface
///
/// GET /api/v1/sos/interfaces/{id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/interfaces/{id}",
    params(
        ("id" = String, Path, description = "Interface ID")
    ),
    responses(
        (status = 200, description = "Interface retrieved successfully", body = InterfaceResponse),
        (status = 404, description = "Interface not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn get_interface(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<InterfaceResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Query RocksDB for interface_id
    match storage_manager.get_interface(&id) {
        Ok(Some(interface)) => Ok(Json(InterfaceResponse {
            system_id: interface.system_id,
            interface: SystemInterface {
                interface_id: interface.interface_id,
                interface_name: interface.interface_name,
                direction: interface.direction,
                protocol: interface.protocol,
                data_format: interface.data_format,
                schema: interface.schema,
                coordinate_system: interface.coordinate_system,
                unit_system: interface.unit_system,
                metadata: interface.metadata,
            },
            created_at: interface.created_at.to_rfc3339(),
            updated_at: interface.updated_at.to_rfc3339(),
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(SosErrorResponse {
                error: "INTERFACE_NOT_FOUND".to_string(),
                message: format!("Interface with ID '{}' not found", id),
                details: None,
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve interface: {}", e),
                details: None,
            }),
        )),
    }
}

/// Update an interface
///
/// PUT /api/v1/sos/interfaces/{id}
#[utoipa::path(
    put,
    path = "/api/v1/sos/interfaces/{id}",
    params(
        ("id" = String, Path, description = "Interface ID")
    ),
    request_body = UpdateInterfaceRequest,
    responses(
        (status = 200, description = "Interface updated successfully", body = InterfaceResponse),
        (status = 404, description = "Interface not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn update_interface(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateInterfaceRequest>,
) -> Result<Json<InterfaceResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::validators::validate_direction;
    use chrono::Utc;

    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Validate interface exists and retrieve it
    let mut interface = match storage_manager.get_interface(&id) {
        Ok(Some(iface)) => iface,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "INTERFACE_NOT_FOUND".to_string(),
                    message: format!("Interface with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve interface: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // 2. Validate and apply updates (only non-None fields - partial update support)
    if let Some(interface_name) = request.interface_name {
        interface.interface_name = interface_name;
    }

    if let Some(direction) = request.direction {
        // Validate direction
        if let Err(e) = validate_direction(&direction) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(SosErrorResponse {
                    error: "INVALID_DIRECTION".to_string(),
                    message: e.to_string(),
                    details: None,
                }),
            ));
        }
        interface.direction = direction;
    }

    if let Some(schema) = request.schema {
        interface.schema = schema;
    }

    if let Some(coordinate_system) = request.coordinate_system {
        interface.coordinate_system = Some(coordinate_system);
    }

    if let Some(unit_system) = request.unit_system {
        interface.unit_system = Some(unit_system);
    }

    if let Some(metadata) = request.metadata {
        interface.metadata = metadata;
    }

    // Always update the updated_at timestamp
    interface.updated_at = Utc::now();

    if let Err(error) = ensure_interface_ontology_assets(
        &mut interface,
        state.persisted_ontology_registry.as_deref(),
    )
    .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "ONTOLOGY_REGISTRATION_ERROR".to_string(),
                message: format!("Failed to refresh interface ontology assets: {}", error),
                details: None,
            }),
        ));
    }

    // 3. Store updated interface in RocksDB
    if let Err(e) = storage_manager.put_interface(&interface) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to update interface: {}", e),
                details: None,
            }),
        ));
    }

    project_interface_upsert(&state, &interface)?;

    // 4. Return updated InterfaceResponse
    Ok(Json(InterfaceResponse {
        system_id: interface.system_id,
        interface: SystemInterface {
            interface_id: interface.interface_id,
            interface_name: interface.interface_name,
            direction: interface.direction,
            protocol: interface.protocol,
            data_format: interface.data_format,
            schema: interface.schema,
            coordinate_system: interface.coordinate_system,
            unit_system: interface.unit_system,
            metadata: interface.metadata,
        },
        created_at: interface.created_at.to_rfc3339(),
        updated_at: interface.updated_at.to_rfc3339(),
    }))
}

/// Delete an interface
///
/// DELETE /api/v1/sos/interfaces/{id}
#[utoipa::path(
    delete,
    path = "/api/v1/sos/interfaces/{id}",
    params(
        ("id" = String, Path, description = "Interface ID")
    ),
    responses(
        (status = 204, description = "Interface deleted successfully"),
        (status = 404, description = "Interface not found", body = SosErrorResponse),
        (status = 409, description = "Interface has active contracts", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn delete_interface(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // 1. Verify interface exists (and get its system_id for deletion)
    let interface = match storage_manager.get_interface(&id) {
        Ok(Some(iface)) => iface,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "INTERFACE_NOT_FOUND".to_string(),
                    message: format!("Interface with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve interface: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // 2. Check for dependent contracts (Phase 3)
    // Check if interface is used as provider in any contracts
    match storage_manager.list_contracts_by_provider(&id) {
        Ok(contracts) if !contracts.is_empty() => {
            return Err((
                StatusCode::CONFLICT,
                Json(SosErrorResponse {
                    error: "INTERFACE_HAS_CONTRACTS".to_string(),
                    message: format!(
                        "Cannot delete interface '{}' - it is used as provider in {} contract(s)",
                        id,
                        contracts.len()
                    ),
                    details: Some(serde_json::json!({
                        "contract_count": contracts.len(),
                        "role": "provider"
                    })),
                }),
            ));
        }
        Ok(_) => {
            // No provider contracts, continue checking
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check provider contracts: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // Check if interface is used as consumer in any contracts
    match storage_manager.list_contracts_by_consumer(&id) {
        Ok(contracts) if !contracts.is_empty() => {
            return Err((
                StatusCode::CONFLICT,
                Json(SosErrorResponse {
                    error: "INTERFACE_HAS_CONTRACTS".to_string(),
                    message: format!(
                        "Cannot delete interface '{}' - it is used as consumer in {} contract(s)",
                        id,
                        contracts.len()
                    ),
                    details: Some(serde_json::json!({
                        "contract_count": contracts.len(),
                        "role": "consumer"
                    })),
                }),
            ));
        }
        Ok(_) => {
            // No consumer contracts, safe to delete
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check consumer contracts: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // 3. Delete interface from RocksDB
    match storage_manager.delete_interface(&interface.interface_id, &interface.system_id) {
        Ok(_) => {
            project_interface_delete(&state, &interface.interface_id)?;

            // Return 204 No Content (successful deletion)
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to delete interface: {}", e),
                details: None,
            }),
        )),
    }
}

/// Validate an interface schema
///
/// POST /api/v1/sos/interfaces/{id}/validate-schema
#[utoipa::path(
    post,
    path = "/api/v1/sos/interfaces/{id}/validate-schema",
    params(
        ("id" = String, Path, description = "Interface ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Schema validation result", body = ValidationResponse),
        (status = 404, description = "Interface not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Interface Definition"
)]
pub async fn validate_interface_schema(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(data): Json<serde_json::Value>,
) -> Result<Json<ValidationResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .validate_interface_schema_payload(&id, data, true)
        .map(Json)
        .map_err(sos_validation_error_response)
}

// ============================================================================
// Data Contract Handlers
// ============================================================================

/// Create a data contract between interfaces
///
/// POST /api/v1/sos/contracts
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts",
    request_body = CreateDataContractRequest,
    responses(
        (status = 200, description = "Contract created successfully", body = DataContractResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Interface not found", body = SosErrorResponse),
        (status = 409, description = "Contract ID already exists", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn create_contract(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<CreateDataContractRequest>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Contract;
    use super::validators::{validate_contract_transformation_rules, validate_sla_metrics};
    use chrono::Utc;

    // ========================================================================
    // STEP 1: Get storage manager from state
    // ========================================================================
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // ========================================================================
    // STEP 2: Validate required fields
    // ========================================================================
    if request.contract_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.contract_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_name cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.provider_interface_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "provider_interface_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if request.consumer_interface_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "consumer_interface_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // ========================================================================
    // STEP 3: Validate provider interface exists
    // ========================================================================
    let provider_interface = match storage_manager.get_interface(&request.provider_interface_id) {
        Ok(Some(iface)) => iface,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "INTERFACE_NOT_FOUND".to_string(),
                    message: format!(
                        "Provider interface with ID '{}' not found",
                        request.provider_interface_id
                    ),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check provider interface existence: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // ========================================================================
    // STEP 3b: Validate consumer interface exists
    // ========================================================================
    let consumer_interface = match storage_manager.get_interface(&request.consumer_interface_id) {
        Ok(Some(iface)) => iface,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "INTERFACE_NOT_FOUND".to_string(),
                    message: format!(
                        "Consumer interface with ID '{}' not found",
                        request.consumer_interface_id
                    ),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check consumer interface existence: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // ========================================================================
    // STEP 4: Validate interface directions
    // Provider MUST be "Provider", Consumer MUST be "Consumer"
    // ========================================================================
    if provider_interface.direction != "Provider" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_DIRECTION".to_string(),
                message: format!(
                    "Provider interface must have direction 'Provider', got '{}'",
                    provider_interface.direction
                ),
                details: None,
            }),
        ));
    }

    if consumer_interface.direction != "Consumer" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_DIRECTION".to_string(),
                message: format!(
                    "Consumer interface must have direction 'Consumer', got '{}'",
                    consumer_interface.direction
                ),
                details: None,
            }),
        ));
    }

    // ========================================================================
    // STEP 5: Check if contract_id already exists
    // ========================================================================
    match storage_manager.get_contract(&request.contract_id) {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(SosErrorResponse {
                    error: "CONTRACT_EXISTS".to_string(),
                    message: format!("Contract with ID '{}' already exists", request.contract_id),
                    details: None,
                }),
            ));
        }
        Ok(None) => {
            // Contract doesn't exist, we can proceed
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to check contract existence: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // ========================================================================
    // STEP 6: Validate SLA metrics
    // Convert from request format to storage format, then validate
    // ========================================================================
    let sla_metrics: Vec<super::storage::SlaMetric> = request
        .sla_metrics
        .iter()
        .map(|m| super::storage::SlaMetric {
            name: m.name.clone(),
            value: m.value,
            operator: m.operator.clone(),
            unit: m.unit.clone(),
        })
        .collect();

    if let Err(e) = validate_sla_metrics(&sla_metrics) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_SLA_METRICS".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    let transformation_report =
        validate_contract_transformation_rules(&request.transformation_rules);
    if !transformation_report.valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_TRANSFORMATION_RULES".to_string(),
                message: transformation_report.issues.join("; "),
                details: None,
            }),
        ));
    }

    // ========================================================================
    // STEP 7: Create Contract entity and store in RocksDB
    // ========================================================================
    let now = Utc::now();
    let actor = request_actor(claims.as_ref());
    let contract = Contract {
        contract_id: request.contract_id.clone(),
        revision: 1,
        contract_name: request.contract_name.clone(),
        provider_interface_id: request.provider_interface_id.clone(),
        consumer_interface_id: request.consumer_interface_id.clone(),
        sla_metrics: sla_metrics,
        transformation_rules: request.transformation_rules.clone(),
        description: request.description.clone(),
        tags: request.tags.clone(),
        approved: false, // New contracts start as unapproved
        signed: false,   // New contracts start as unsigned
        lifecycle_state: Some("draft".to_string()),
        approval_status: Some("pending".to_string()),
        approval_requested_by: None,
        approval_requested_at: None,
        approved_by: None,
        approved_at: None,
        rejected_by: None,
        rejected_at: None,
        rejection_reason: None,
        signed_by: None,
        signed_at: None,
        created_by: actor.clone(),
        updated_by: actor,
        superseded_by_revision: None,
        created_at: now,
        updated_at: now,
    };

    if let Err(e) = storage_manager.put_contract(&contract) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to store contract: {}", e),
                details: None,
            }),
        ));
    }

    project_contract_upsert(&state, &contract)?;

    // ========================================================================
    // STEP 9: Return DataContractResponse
    // ========================================================================
    let service = sos_validation_service(&state)?;
    Ok(Json(data_contract_response(&service, contract)?))
}

/// List all contracts
///
/// GET /api/v1/sos/contracts
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts",
    responses(
        (status = 200, description = "Contracts retrieved successfully", body = Vec<DataContractResponse>),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn list_contracts(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<Vec<DataContractResponse>>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Default pagination: offset=0, limit=100
    let offset = 0;
    let limit = 100;

    // Retrieve all contracts from storage
    match storage_manager.list_all_contracts(offset, limit) {
        Ok(contracts) => {
            let service = sos_validation_service(&state)?;
            let responses = contracts
                .into_iter()
                .map(|contract| data_contract_response(&service, contract))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Json(responses))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve contracts: {}", e),
                details: None,
            }),
        )),
    }
}

/// Get a specific contract
///
/// GET /api/v1/sos/contracts/{id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/{id}",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 200, description = "Contract retrieved successfully", body = DataContractResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn get_contract(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Validate contract_id is not empty
    if id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // Retrieve contract from storage
    match storage_manager.get_contract(&id) {
        Ok(Some(contract)) => {
            let service = sos_validation_service(&state)?;
            Ok(Json(data_contract_response(&service, contract)?))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(SosErrorResponse {
                error: "CONTRACT_NOT_FOUND".to_string(),
                message: format!("Contract with ID '{}' not found", id),
                details: None,
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve contract: {}", e),
                details: None,
            }),
        )),
    }
}

/// Look up a contract by provider/consumer interface pair.
///
/// GET /api/v1/sos/contracts/lookup
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/lookup",
    params(ContractLookupQuery),
    responses(
        (status = 200, description = "Contract retrieved successfully", body = DataContractResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract not found for interface pair", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn lookup_contract(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ContractLookupQuery>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    if query.provider_interface_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "provider_interface_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    if query.consumer_interface_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "consumer_interface_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    match storage_manager
        .get_contract_by_interface_pair(&query.provider_interface_id, &query.consumer_interface_id)
    {
        Ok(Some(contract)) => {
            let service = sos_validation_service(&state)?;
            Ok(Json(data_contract_response(&service, contract)?))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(SosErrorResponse {
                error: "CONTRACT_NOT_FOUND".to_string(),
                message: format!(
                    "No contract found for provider interface '{}' and consumer interface '{}'",
                    query.provider_interface_id, query.consumer_interface_id
                ),
                details: None,
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to retrieve contract: {}", e),
                details: None,
            }),
        )),
    }
}

/// Update a contract
///
/// PUT /api/v1/sos/contracts/{id}
#[utoipa::path(
    put,
    path = "/api/v1/sos/contracts/{id}",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    request_body = UpdateDataContractRequest,
    responses(
        (status = 200, description = "Contract updated successfully", body = DataContractResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 409, description = "Contract is signed and cannot be modified", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn update_contract(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateDataContractRequest>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::validators::{validate_contract_transformation_rules, validate_sla_metrics};
    use chrono::Utc;

    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    if request.approved.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "Contract approval status cannot be changed through update; use /approve"
                    .to_string(),
                details: None,
            }),
        ));
    }

    // Validate contract_id is not empty
    if id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // Retrieve existing contract
    let mut contract = match storage_manager.get_contract(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "CONTRACT_NOT_FOUND".to_string(),
                    message: format!("Contract with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve contract: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // Signed revisions are immutable; a deeper revision flow for signed artifacts
    // should be explicit rather than silently mutating the currently signed contract.
    if effective_contract_lifecycle_state(&contract) == CONTRACT_LIFECYCLE_SIGNED {
        return Err((
            StatusCode::CONFLICT,
            Json(SosErrorResponse {
                error: "CONTRACT_SIGNED".to_string(),
                message: "Signed contracts cannot be modified".to_string(),
                details: None,
            }),
        ));
    }

    let actor = request_actor(claims.as_ref());
    let mut semantic_changes = false;

    // Update fields if provided
    if let Some(name) = request.contract_name {
        if !name.is_empty() {
            semantic_changes |= contract.contract_name != name;
            contract.contract_name = name;
        }
    }

    if let Some(sla_metrics) = request.sla_metrics {
        // Convert to storage format and validate
        let storage_metrics: Vec<super::storage::SlaMetric> = sla_metrics
            .iter()
            .map(|m| super::storage::SlaMetric {
                name: m.name.clone(),
                value: m.value,
                operator: m.operator.clone(),
                unit: m.unit.clone(),
            })
            .collect();

        if let Err(e) = validate_sla_metrics(&storage_metrics) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(SosErrorResponse {
                    error: "INVALID_SLA_METRICS".to_string(),
                    message: e.to_string(),
                    details: None,
                }),
            ));
        }

        semantic_changes |= contract.sla_metrics != storage_metrics;
        contract.sla_metrics = storage_metrics;
    }

    if let Some(transformation_rules) = request.transformation_rules {
        let transformation_report = validate_contract_transformation_rules(&transformation_rules);
        if !transformation_report.valid {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(SosErrorResponse {
                    error: "INVALID_TRANSFORMATION_RULES".to_string(),
                    message: transformation_report.issues.join("; "),
                    details: None,
                }),
            ));
        }

        semantic_changes |= contract.transformation_rules != transformation_rules;
        contract.transformation_rules = transformation_rules;
    }

    if let Some(description) = request.description {
        semantic_changes |= contract.description != Some(description.clone());
        contract.description = Some(description);
    }

    if let Some(tags) = request.tags {
        contract.tags = tags;
    }

    let now = Utc::now();
    if semantic_changes {
        contract.revision = contract.revision.saturating_add(1);
        contract.superseded_by_revision = None;
        set_contract_draft(&mut contract);
    }

    contract.updated_by = actor;
    contract.updated_at = now;

    // Store updated contract
    if let Err(e) = storage_manager.put_contract(&contract) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to update contract: {}", e),
                details: None,
            }),
        ));
    }

    project_contract_upsert(&state, &contract)?;

    let service = sos_validation_service(&state)?;
    Ok(Json(data_contract_response(&service, contract)?))
}

/// Delete a contract
///
/// DELETE /api/v1/sos/contracts/{id}
#[utoipa::path(
    delete,
    path = "/api/v1/sos/contracts/{id}",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 204, description = "Contract deleted successfully"),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 409, description = "Signed contracts cannot be deleted", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn delete_contract(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<SosErrorResponse>)> {
    // Get storage manager from state
    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Validate contract_id is not empty
    if id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // Retrieve existing contract to get interface IDs and check if signed
    let contract = match storage_manager.get_contract(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "CONTRACT_NOT_FOUND".to_string(),
                    message: format!("Contract with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve contract: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // Check if contract is signed - signed contracts cannot be deleted
    if effective_contract_lifecycle_state(&contract) == CONTRACT_LIFECYCLE_SIGNED {
        return Err((
            StatusCode::CONFLICT,
            Json(SosErrorResponse {
                error: "CONTRACT_SIGNED".to_string(),
                message: "Signed contracts cannot be deleted".to_string(),
                details: None,
            }),
        ));
    }

    // Delete contract (includes index cleanup)
    if let Err(e) = storage_manager.delete_contract(
        &contract.contract_id,
        &contract.provider_interface_id,
        &contract.consumer_interface_id,
    ) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to delete contract: {}", e),
                details: None,
            }),
        ));
    }

    project_contract_delete(&state, &contract.contract_id)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Create a first-class approval request for a persisted contract revision.
///
/// POST /api/v1/sos/contracts/{id}/approval-requests
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/approval-requests",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    request_body = CreateSosContractApprovalRequest,
    responses(
        (status = 200, description = "Contract approval request created", body = SosContractApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn create_contract_approval_request(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<CreateSosContractApprovalRequest>,
) -> Result<Json<SosContractApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .create_contract_approval_request(&id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// List approval requests for a persisted contract.
///
/// GET /api/v1/sos/contracts/{id}/approval-requests
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/{id}/approval-requests",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ListContractApprovalRequestsQuery
    ),
    responses(
        (status = 200, description = "Contract approval requests listed", body = ListContractApprovalRequestsResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn list_contract_approval_requests(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<ListContractApprovalRequestsQuery>,
) -> Result<Json<ListContractApprovalRequestsResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .list_contract_approval_requests(&id, query)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Get one approval request for a persisted contract.
///
/// GET /api/v1/sos/contracts/{id}/approval-requests/{request_id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/{id}/approval-requests/{request_id}",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ("request_id" = String, Path, description = "Contract approval request ID")
    ),
    responses(
        (status = 200, description = "Contract approval request retrieved", body = SosContractApprovalRequestResponse),
        (status = 404, description = "Contract or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn get_contract_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
) -> Result<Json<SosContractApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .get_contract_approval_request(&id, &request_id)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Attach approval evidence to a contract approval request.
///
/// POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/evidence
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/approval-requests/{request_id}/evidence",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ("request_id" = String, Path, description = "Contract approval request ID")
    ),
    request_body = AddSosContractApprovalEvidenceRequest,
    responses(
        (status = 200, description = "Contract approval evidence attached", body = SosContractApprovalEvidenceResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract, approval request, or validation report not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn add_contract_approval_evidence(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<AddSosContractApprovalEvidenceRequest>,
) -> Result<Json<SosContractApprovalEvidenceResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .add_contract_approval_evidence(&id, &request_id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Approve a specific contract approval request.
///
/// POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/approve
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/approval-requests/{request_id}/approve",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ("request_id" = String, Path, description = "Contract approval request ID")
    ),
    request_body = ApproveSosContractApprovalRequest,
    responses(
        (status = 200, description = "Contract approval request approved", body = SosContractApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn approve_contract_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<ApproveSosContractApprovalRequest>,
) -> Result<Json<SosContractApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .approve_contract_approval_request(&id, &request_id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Reject a specific contract approval request.
///
/// POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/reject
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/approval-requests/{request_id}/reject",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ("request_id" = String, Path, description = "Contract approval request ID")
    ),
    request_body = RejectSosContractApprovalRequest,
    responses(
        (status = 200, description = "Contract approval request rejected", body = SosContractApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Contract or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn reject_contract_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<RejectSosContractApprovalRequest>,
) -> Result<Json<SosContractApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .reject_contract_approval_request(&id, &request_id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Approve a contract
///
/// POST /api/v1/sos/contracts/{id}/approve
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/approve",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 200, description = "Contract approved successfully", body = DataContractResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn approve_contract(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Path(id): Path<String>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    if id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    let actor = request_actor(claims.as_ref());
    let service = sos_validation_service(&state)?;
    service
        .approve_contract(&id, &actor)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// List signature attestations for all persisted revisions of one contract.
///
/// GET /api/v1/sos/contracts/{id}/signatures
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/{id}/signatures",
    params(
        ("id" = String, Path, description = "Contract ID"),
        ListContractSignaturesQuery
    ),
    responses(
        (status = 200, description = "Contract signature history", body = ListContractSignaturesResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn list_contract_signatures(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<ListContractSignaturesQuery>,
) -> Result<Json<ListContractSignaturesResponse>, (StatusCode, Json<SosErrorResponse>)> {
    if id.is_empty() {
        return Err(contract_signing_key_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "contract_id cannot be empty",
        ));
    }

    let service = sos_validation_service(&state)?;
    service
        .list_contract_signatures(&id, query.limit)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Inspect the currently configured SoS contract signing key.
///
/// GET /api/v1/sos/contracts/signing-key
#[utoipa::path(
    get,
    path = "/api/v1/sos/contracts/signing-key",
    responses(
        (status = 200, description = "Contract signing-key status", body = SosContractSigningKeyStatusResponse),
        (status = 404, description = "No signing key is configured yet", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn get_contract_signing_key_status(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<SosContractSigningKeyStatusResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let resolved = resolve_contract_signing_key(&state, false).await?;
    Ok(Json(contract_signing_key_status_response(&resolved)))
}

/// Rotate the managed SoS contract signing key in the configured secret store.
///
/// POST /api/v1/sos/contracts/signing-key/rotate
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/signing-key/rotate",
    request_body = RotateSosContractSigningKeyRequest,
    responses(
        (status = 200, description = "Contract signing key rotated", body = RotateSosContractSigningKeyResponse),
        (status = 409, description = "Rotation is not supported for the current signing-key source", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn rotate_contract_signing_key(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<RotateSosContractSigningKeyRequest>,
) -> Result<Json<RotateSosContractSigningKeyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    const SIGNING_KEY_REF_ENV: &str = "GRAPHICA_SOS_CONTRACT_SIGNING_KEY_REF";
    const DEFAULT_SIGNING_KEY_REF: &str = "sos/contracts/signing-key";

    let registry = state.secret_store_registry.as_ref().ok_or_else(|| {
        contract_signing_key_error(
            StatusCode::CONFLICT,
            "SIGNATURE_CONFIGURATION_ERROR",
            "Contract signing-key rotation requires a writable default secret store",
        )
    })?;
    let store = registry.default().ok_or_else(|| {
        contract_signing_key_error(
            StatusCode::CONFLICT,
            "SIGNATURE_CONFIGURATION_ERROR",
            "Contract signing-key rotation requires a writable default secret store",
        )
    })?;

    let actor = request_actor(claims.as_ref());
    let signing_key_ref =
        std::env::var(SIGNING_KEY_REF_ENV).unwrap_or_else(|_| DEFAULT_SIGNING_KEY_REF.to_string());
    let previous = resolve_contract_signing_key(&state, false).await.ok();

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let metadata = contract_signing_secret_metadata(
        previous
            .as_ref()
            .map(|resolved| resolved.secret_metadata.clone()),
        &actor,
        request.reason.as_deref(),
        previous
            .as_ref()
            .map(|resolved| resolved.key_fingerprint.as_str()),
    );

    let version = put_secret_by_ref(
        store.as_ref(),
        &signing_key_ref,
        SecretValue::Binary(seed.to_vec()),
        Some(metadata),
    )
    .await
    .map_err(|error| {
        contract_signing_key_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SIGNATURE_CONFIGURATION_ERROR",
            format!(
                "Failed to rotate contract signing key in secret store '{}': {}",
                signing_key_ref, error
            ),
        )
    })?;

    let resolved = resolve_contract_signing_key(&state, false).await?;
    Ok(Json(RotateSosContractSigningKeyResponse {
        signing_key_ref,
        previous_signing_key_version: previous
            .as_ref()
            .and_then(|resolved| resolved.signing_key_version.clone()),
        previous_key_fingerprint: previous
            .as_ref()
            .map(|resolved| resolved.key_fingerprint.clone()),
        current_signing_key_version: resolved.signing_key_version.unwrap_or(version),
        current_key_fingerprint: resolved.key_fingerprint,
        current_public_key: resolved.public_key,
        rotated_by: actor,
        rotated_at: chrono::Utc::now().to_rfc3339(),
        metadata: resolved.secret_metadata.custom,
    }))
}

/// Sign a contract (cryptographic signature)
///
/// POST /api/v1/sos/contracts/{id}/sign
#[utoipa::path(
    post,
    path = "/api/v1/sos/contracts/{id}/sign",
    params(
        ("id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 200, description = "Contract signed successfully", body = DataContractResponse),
        (status = 400, description = "Contract must be approved before signing", body = SosErrorResponse),
        (status = 404, description = "Contract not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Data Contracts"
)]
pub async fn sign_contract(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Path(id): Path<String>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use chrono::Utc;

    if id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "contract_id cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    let storage_manager = match &state.sos_storage_manager {
        Some(mgr) => mgr,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SosErrorResponse {
                    error: "SERVICE_UNAVAILABLE".to_string(),
                    message: "SoS validation service is not enabled".to_string(),
                    details: None,
                }),
            ));
        }
    };
    let service = sos_validation_service(&state)?;

    let mut contract = match storage_manager.get_contract(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(SosErrorResponse {
                    error: "CONTRACT_NOT_FOUND".to_string(),
                    message: format!("Contract with ID '{}' not found", id),
                    details: None,
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve contract: {}", e),
                    details: None,
                }),
            ));
        }
    };

    if !contract.approved {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SosErrorResponse {
                error: "CONTRACT_NOT_APPROVED".to_string(),
                message: "Contract must be approved before it can be signed".to_string(),
                details: None,
            }),
        ));
    }

    let existing_signature = storage_manager
        .get_contract_signature(&contract.contract_id, contract.revision)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve contract signature: {}", e),
                    details: None,
                }),
            )
        })?;
    if existing_signature.is_some() {
        project_contract_upsert(&state, &contract)?;
        return Ok(Json(data_contract_response(&service, contract)?));
    }

    let actor = request_actor(claims.as_ref());
    let (approval_request, evidence) =
        current_contract_approval_context(storage_manager.as_ref(), &contract).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SosErrorResponse {
                    error: "DATABASE_ERROR".to_string(),
                    message: format!("Failed to retrieve contract approval context: {}", e),
                    details: None,
                }),
            )
        })?;
    let signing_key = resolve_contract_signing_key(&state, true).await?;

    let signing_policy_refs = if contract.signed {
        Vec::new()
    } else {
        service
            .evaluate_contract_governance_policies(
                "contract_signing",
                &contract,
                approval_request.as_ref(),
                &evidence,
                [
                    (
                        "contract_signing_actor".to_string(),
                        Value::String(actor.clone()),
                    ),
                    (
                        "contract_signing_key_source".to_string(),
                        Value::String(signing_key.signing_key_source.clone()),
                    ),
                    (
                        "contract_signing_key_fingerprint".to_string(),
                        Value::String(signing_key.key_fingerprint.clone()),
                    ),
                ]
                .into_iter()
                .chain(
                    signing_key
                        .signing_key_ref
                        .as_ref()
                        .map(|value| {
                            (
                                "contract_signing_key_ref".to_string(),
                                Value::String(value.clone()),
                            )
                        })
                        .into_iter(),
                )
                .chain(
                    signing_key
                        .signing_key_version
                        .as_ref()
                        .map(|value| {
                            (
                                "contract_signing_key_version".to_string(),
                                Value::String(value.clone()),
                            )
                        })
                        .into_iter(),
                )
                .collect(),
            )
            .map_err(sos_validation_error_response)?
    };

    if !contract.signed {
        let now = Utc::now();
        set_contract_signed(&mut contract, &actor, now);
        contract.updated_by = actor.clone();
        contract.updated_at = now;
    }

    let mut policy_refs = approval_request
        .as_ref()
        .and_then(|request| request.metadata.get("policy_refs"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    policy_refs.extend(signing_policy_refs);
    let signature = build_contract_signature_record(
        &contract,
        &signing_key.signing_key,
        signing_key.signing_key_ref.as_deref(),
        signing_key.signing_key_version.as_deref(),
        &signing_key.signing_key_source,
        approval_request
            .as_ref()
            .map(|request| request.request_id.clone()),
        evidence
            .iter()
            .map(|record| record.evidence_id.clone())
            .collect(),
        policy_refs,
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "SIGNATURE_ERROR".to_string(),
                message: format!("Failed to build contract signature: {}", error),
                details: None,
            }),
        )
    })?;

    if let Err(e) = storage_manager.put_contract(&contract) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to sign contract: {}", e),
                details: None,
            }),
        ));
    }
    if let Err(e) = storage_manager.put_contract_signature(&signature) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to persist contract signature: {}", e),
                details: None,
            }),
        ));
    }

    project_contract_upsert(&state, &contract)?;

    Ok(Json(data_contract_response(&service, contract)?))
}

// ============================================================================
// Validation Handlers
// ============================================================================

/// Execute SoS validation
///
/// POST /api/v1/sos/validate
#[utoipa::path(
    post,
    path = "/api/v1/sos/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Validation completed", body = ValidationResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Resource not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn validate(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ValidateRequest>,
) -> Result<Json<ValidationResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .validate_request(request, ValidationExecutionOptions::persisted())
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Dry-run validation (no state changes)
///
/// POST /api/v1/sos/validate/dry-run
#[utoipa::path(
    post,
    path = "/api/v1/sos/validate/dry-run",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Dry-run validation completed", body = ValidationResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn validate_dry_run(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ValidateRequest>,
) -> Result<Json<ValidationResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .validate_request(request, ValidationExecutionOptions::dry_run())
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Create a persisted SoS policy.
///
/// POST /api/v1/sos/policies
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies",
    request_body = CreateSosPolicyRequest,
    responses(
        (status = 200, description = "Policy created", body = SosPolicyResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Referenced SoS entity not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn create_policy(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateSosPolicyRequest>,
) -> Result<Json<SosPolicyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .create_policy(request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// List persisted SoS policies.
///
/// GET /api/v1/sos/policies
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies",
    params(ListPoliciesQuery),
    responses(
        (status = 200, description = "Policies listed", body = ListPoliciesResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn list_policies(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListPoliciesQuery>,
) -> Result<Json<ListPoliciesResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .list_policies(query)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Get a persisted SoS policy by ID.
///
/// GET /api/v1/sos/policies/{id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies/{id}",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    responses(
        (status = 200, description = "Policy retrieved", body = SosPolicyResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<SosPolicyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .get_policy(&id)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Update a persisted SoS policy.
///
/// PUT /api/v1/sos/policies/{id}
#[utoipa::path(
    put,
    path = "/api/v1/sos/policies/{id}",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = UpdateSosPolicyRequest,
    responses(
        (status = 200, description = "Policy updated", body = SosPolicyResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn update_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSosPolicyRequest>,
) -> Result<Json<SosPolicyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .update_policy(&id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Delete a persisted SoS policy.
///
/// DELETE /api/v1/sos/policies/{id}
#[utoipa::path(
    delete,
    path = "/api/v1/sos/policies/{id}",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    responses(
        (status = 204, description = "Policy deleted"),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn delete_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .delete_policy(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(sos_validation_error_response)
}

/// Create a first-class approval request for a persisted SoS policy revision.
///
/// POST /api/v1/sos/policies/{id}/approval-requests
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/approval-requests",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = CreateSosPolicyApprovalRequest,
    responses(
        (status = 200, description = "Policy approval request created", body = SosPolicyApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn create_policy_approval_request(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<CreateSosPolicyApprovalRequest>,
) -> Result<Json<SosPolicyApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .create_policy_approval_request(&id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// List approval requests for a persisted SoS policy.
///
/// GET /api/v1/sos/policies/{id}/approval-requests
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies/{id}/approval-requests",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ListPolicyApprovalRequestsQuery
    ),
    responses(
        (status = 200, description = "Policy approval requests listed", body = ListPolicyApprovalRequestsResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn list_policy_approval_requests(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<ListPolicyApprovalRequestsQuery>,
) -> Result<Json<ListPolicyApprovalRequestsResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .list_policy_approval_requests(&id, query)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Get one approval request for a persisted SoS policy.
///
/// GET /api/v1/sos/policies/{id}/approval-requests/{request_id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies/{id}/approval-requests/{request_id}",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ("request_id" = String, Path, description = "Policy approval request ID")
    ),
    responses(
        (status = 200, description = "Policy approval request retrieved", body = SosPolicyApprovalRequestResponse),
        (status = 404, description = "Policy or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_policy_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
) -> Result<Json<SosPolicyApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .get_policy_approval_request(&id, &request_id)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Attach rollout evidence to a policy approval request.
///
/// POST /api/v1/sos/policies/{id}/approval-requests/{request_id}/evidence
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/approval-requests/{request_id}/evidence",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ("request_id" = String, Path, description = "Policy approval request ID")
    ),
    request_body = AddSosPolicyApprovalEvidenceRequest,
    responses(
        (status = 200, description = "Policy approval evidence attached", body = SosPolicyApprovalEvidenceResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy, approval request, or validation report not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn add_policy_approval_evidence(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<AddSosPolicyApprovalEvidenceRequest>,
) -> Result<Json<SosPolicyApprovalEvidenceResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .add_policy_approval_evidence(&id, &request_id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Approve a specific policy approval request.
///
/// POST /api/v1/sos/policies/{id}/approval-requests/{request_id}/approve
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/approval-requests/{request_id}/approve",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ("request_id" = String, Path, description = "Policy approval request ID")
    ),
    request_body = ApproveSosPolicyApprovalRequest,
    responses(
        (status = 200, description = "Policy approval request approved", body = SosPolicyApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn approve_policy_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<ApproveSosPolicyApprovalRequest>,
) -> Result<Json<SosPolicyApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    let signing_material = resolve_policy_signing_key(&state, true)
        .await?
        .signing_material();
    service
        .approve_policy_approval_request_with_attestation(
            &id,
            &request_id,
            request,
            signing_material,
        )
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Reject a specific policy approval request.
///
/// POST /api/v1/sos/policies/{id}/approval-requests/{request_id}/reject
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/approval-requests/{request_id}/reject",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ("request_id" = String, Path, description = "Policy approval request ID")
    ),
    request_body = RejectSosPolicyApprovalRequest,
    responses(
        (status = 200, description = "Policy approval request rejected", body = SosPolicyApprovalRequestResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy or approval request not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn reject_policy_approval_request(
    State(state): State<Arc<ApiState>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(request): Json<RejectSosPolicyApprovalRequest>,
) -> Result<Json<SosPolicyApprovalRequestResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .reject_policy_approval_request(&id, &request_id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Approve a persisted SoS policy revision for rollout.
///
/// POST /api/v1/sos/policies/{id}/approve
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/approve",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = ApproveSosPolicyRequest,
    responses(
        (status = 200, description = "Policy approved", body = SosPolicyResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn approve_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<ApproveSosPolicyRequest>,
) -> Result<Json<SosPolicyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    let signing_material = resolve_policy_signing_key(&state, true)
        .await?
        .signing_material();
    service
        .approve_policy_with_attestation(&id, request, signing_material)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Reject a persisted SoS policy revision for rollout.
///
/// POST /api/v1/sos/policies/{id}/reject
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/reject",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = RejectSosPolicyRequest,
    responses(
        (status = 200, description = "Policy rejected", body = SosPolicyResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn reject_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<RejectSosPolicyRequest>,
) -> Result<Json<SosPolicyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .reject_policy(&id, request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// List approval attestations for all persisted revisions of one policy.
///
/// GET /api/v1/sos/policies/{id}/attestations
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies/{id}/attestations",
    params(
        ("id" = String, Path, description = "Policy ID"),
        ListPolicyAttestationsQuery
    ),
    responses(
        (status = 200, description = "Policy attestation history", body = ListPolicyAttestationsResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn list_policy_attestations(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<ListPolicyAttestationsQuery>,
) -> Result<Json<ListPolicyAttestationsResponse>, (StatusCode, Json<SosErrorResponse>)> {
    if id.is_empty() {
        return Err(policy_signing_key_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "policy_id cannot be empty",
        ));
    }

    let service = sos_validation_service(&state)?;
    service
        .list_policy_attestations(&id, query.limit)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Inspect the currently configured SoS policy signing key.
///
/// GET /api/v1/sos/policies/signing-key
#[utoipa::path(
    get,
    path = "/api/v1/sos/policies/signing-key",
    responses(
        (status = 200, description = "Policy signing-key status", body = SosPolicySigningKeyStatusResponse),
        (status = 404, description = "No signing key is configured yet", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_policy_signing_key_status(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<SosPolicySigningKeyStatusResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let resolved = resolve_policy_signing_key(&state, false).await?;
    Ok(Json(policy_signing_key_status_response(&resolved)))
}

/// Rotate the managed SoS policy signing key in the configured secret store.
///
/// POST /api/v1/sos/policies/signing-key/rotate
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/signing-key/rotate",
    request_body = RotateSosPolicySigningKeyRequest,
    responses(
        (status = 200, description = "Policy signing key rotated", body = RotateSosPolicySigningKeyResponse),
        (status = 400, description = "Invalid trust configuration", body = SosErrorResponse),
        (status = 409, description = "Rotation is not supported for the current signing-key source", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
        (status = 503, description = "Service unavailable", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn rotate_policy_signing_key(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<RotateSosPolicySigningKeyRequest>,
) -> Result<Json<RotateSosPolicySigningKeyResponse>, (StatusCode, Json<SosErrorResponse>)> {
    const SIGNING_KEY_REF_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_KEY_REF";
    const DEFAULT_SIGNING_KEY_REF: &str = "sos/policies/signing-key";

    let registry = state.secret_store_registry.as_ref().ok_or_else(|| {
        policy_signing_key_error(
            StatusCode::CONFLICT,
            "SIGNATURE_CONFIGURATION_ERROR",
            "Policy signing-key rotation requires a writable default secret store",
        )
    })?;
    let store = registry.default().ok_or_else(|| {
        policy_signing_key_error(
            StatusCode::CONFLICT,
            "SIGNATURE_CONFIGURATION_ERROR",
            "Policy signing-key rotation requires a writable default secret store",
        )
    })?;

    let actor = request_actor(claims.as_ref());
    let signing_key_ref =
        std::env::var(SIGNING_KEY_REF_ENV).unwrap_or_else(|_| DEFAULT_SIGNING_KEY_REF.to_string());
    let previous = resolve_policy_signing_key(&state, false).await.ok();

    let mut trust_settings = match previous.as_ref() {
        Some(resolved) => resolved.trust_settings.clone(),
        None => resolve_policy_signing_trust_settings(None)?,
    };
    apply_policy_trust_overrides(&mut trust_settings, &request)?;

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let metadata = policy_signing_secret_metadata(
        previous
            .as_ref()
            .map(|resolved| resolved.secret_metadata.clone()),
        &actor,
        request.reason.as_deref(),
        previous
            .as_ref()
            .map(|resolved| resolved.key_fingerprint.as_str()),
        &trust_settings,
    );

    let version = put_secret_by_ref(
        store.as_ref(),
        &signing_key_ref,
        SecretValue::Binary(seed.to_vec()),
        Some(metadata),
    )
    .await
    .map_err(|error| {
        policy_signing_key_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SIGNATURE_CONFIGURATION_ERROR",
            format!(
                "Failed to rotate policy signing key in secret store '{}': {}",
                signing_key_ref, error
            ),
        )
    })?;

    let resolved = resolve_policy_signing_key(&state, false).await?;
    Ok(Json(RotateSosPolicySigningKeyResponse {
        signing_key_ref,
        previous_signing_key_version: previous
            .as_ref()
            .and_then(|resolved| resolved.signing_key_version.clone()),
        previous_key_fingerprint: previous
            .as_ref()
            .map(|resolved| resolved.key_fingerprint.clone()),
        current_signing_key_version: resolved.signing_key_version.clone().unwrap_or(version),
        current_key_fingerprint: resolved.key_fingerprint.clone(),
        current_public_key: resolved.public_key.clone(),
        rotated_by: actor,
        rotated_at: chrono::Utc::now().to_rfc3339(),
        trust_mode: resolved.trust_settings.trust_mode.clone(),
        trust_provider: resolved.trust_settings.trust_provider.clone(),
        external_key_ref: resolved.trust_settings.external_key_ref.clone(),
        trust_attestation_ref: resolved.trust_settings.trust_attestation_ref.clone(),
        metadata: resolved.secret_metadata.custom.clone(),
    }))
}

/// Evaluate a persisted SoS policy and persist the report.
///
/// POST /api/v1/sos/policies/{id}/validate
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/validate",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = EvaluatePolicyRequest,
    responses(
        (status = 200, description = "Policy evaluated", body = ValidationResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn validate_policy(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<EvaluatePolicyRequest>,
) -> Result<Json<ValidationResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .evaluate_policy_by_id(&id, request, ValidationExecutionOptions::persisted())
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Evaluate a persisted SoS policy without persisting a report.
///
/// POST /api/v1/sos/policies/{id}/validate/dry-run
#[utoipa::path(
    post,
    path = "/api/v1/sos/policies/{id}/validate/dry-run",
    params(
        ("id" = String, Path, description = "Policy ID")
    ),
    request_body = EvaluatePolicyRequest,
    responses(
        (status = 200, description = "Policy evaluated in dry-run mode", body = ValidationResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 404, description = "Policy not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn validate_policy_dry_run(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<EvaluatePolicyRequest>,
) -> Result<Json<ValidationResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .evaluate_policy_by_id(&id, request, ValidationExecutionOptions::dry_run())
        .map(Json)
        .map_err(sos_validation_error_response)
}

// ============================================================================
// Analytics Handlers
// ============================================================================

/// Get compatibility matrix for all interface pairs
///
/// GET /api/v1/sos/compatibility-matrix
#[utoipa::path(
    get,
    path = "/api/v1/sos/compatibility-matrix",
    params(CompatibilityMatrixQuery),
    responses(
        (status = 200, description = "Compatibility matrix generated", body = CompatibilityMatrixResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn get_compatibility_matrix(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<CompatibilityMatrixQuery>,
) -> Result<Json<CompatibilityMatrixResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .build_compatibility_matrix_with_query(query)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Get system dependency graph
///
/// GET /api/v1/sos/dependency-graph
#[utoipa::path(
    get,
    path = "/api/v1/sos/dependency-graph",
    params(DependencyGraphQuery),
    responses(
        (status = 200, description = "Dependency graph generated", body = DependencyGraphResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn get_dependency_graph(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<DependencyGraphQuery>,
) -> Result<Json<DependencyGraphResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .build_dependency_graph_with_query(query)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// What-if analysis for system changes
///
/// POST /api/v1/sos/what-if
#[utoipa::path(
    post,
    path = "/api/v1/sos/what-if",
    request_body = WhatIfRequest,
    responses(
        (status = 200, description = "What-if analysis completed", body = WhatIfResponse),
        (status = 400, description = "Invalid request", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn what_if_analysis(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<WhatIfRequest>,
) -> Result<Json<WhatIfResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .run_what_if_analysis(request)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Explicitly rerun SoS ontology/graph reconcile for operator recovery workflows.
///
/// POST /api/v1/sos/reconcile
#[utoipa::path(
    post,
    path = "/api/v1/sos/reconcile",
    request_body = ReconcileSosRequest,
    responses(
        (status = 200, description = "SoS reconcile completed", body = ReconcileSosResponse),
        (status = 401, description = "Authentication required", body = SosErrorResponse),
        (status = 403, description = "Admin access required", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn reconcile_sos_runtime(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<ReconcileSosRequest>,
) -> Result<Json<ReconcileSosResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let actor = require_admin_claims(claims.as_ref())?.sub.clone();
    let service = sos_validation_service(&state)?;
    let storage_manager = state.sos_storage_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SosErrorResponse {
                error: "service_unavailable".to_string(),
                message: "SoS validation service is not enabled".to_string(),
                details: None,
            }),
        )
    })?;

    let started_at = Utc::now();
    let started = std::time::Instant::now();
    let ontology_registry_available = state.persisted_ontology_registry.is_some();
    let mut ontology_sync_performed = false;

    if request.include_ontology_sync {
        if let Some(registry) = state.persisted_ontology_registry.as_ref() {
            reconcile_sos_ontology_assets(storage_manager.as_ref(), registry)
                .await
                .map_err(|error| {
                    sos_validation_error_response(SosValidationServiceError::Internal(
                        error.to_string(),
                    ))
                })?;
            ontology_sync_performed = true;
        }
    }

    service
        .reconcile_graphs()
        .map_err(sos_validation_error_response)?;

    let system_count = storage_manager
        .list_all_systems(0, usize::MAX)
        .map_err(map_storage_error_response)?
        .len();
    let interface_count = storage_manager
        .list_all_interfaces(0, usize::MAX)
        .map_err(map_storage_error_response)?
        .len();
    let contract_count = storage_manager
        .list_all_contracts(0, usize::MAX)
        .map_err(map_storage_error_response)?
        .len();
    let policy_count = storage_manager
        .list_all_policies(0, usize::MAX)
        .map_err(map_storage_error_response)?
        .len();

    Ok(Json(ReconcileSosResponse {
        triggered_by: actor,
        include_ontology_sync: request.include_ontology_sync,
        ontology_registry_available,
        ontology_sync_performed,
        graph_reconcile_performed: true,
        system_count,
        interface_count,
        contract_count,
        policy_count,
        started_at: started_at.to_rfc3339(),
        completed_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
    }))
}

/// Get a persisted validation report by ID.
///
/// GET /api/v1/sos/validation-reports/{report_id}
#[utoipa::path(
    get,
    path = "/api/v1/sos/validation-reports/{report_id}",
    params(
        ("report_id" = String, Path, description = "Persisted validation report ID")
    ),
    responses(
        (status = 200, description = "Validation report retrieved", body = ValidationReportResponse),
        (status = 404, description = "Validation report not found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_validation_report(
    State(state): State<Arc<ApiState>>,
    Path(report_id): Path<String>,
) -> Result<Json<ValidationReportResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    service
        .get_validation_report(&report_id)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Get newest-first validation history for a normalized validation subject.
///
/// GET /api/v1/sos/validation-history
#[utoipa::path(
    get,
    path = "/api/v1/sos/validation-history",
    params(ValidationHistoryQuery),
    responses(
        (status = 200, description = "Validation history retrieved", body = ValidationHistoryResponse),
        (status = 404, description = "No validation history found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_validation_history(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ValidationHistoryQuery>,
) -> Result<Json<ValidationHistoryResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    let limit = normalize_validation_limit(query.limit);

    service
        .get_validation_history(&query.subject_key, query.subject_type.as_deref(), limit)
        .map(Json)
        .map_err(sos_validation_error_response)
}

/// Traverse persisted validation lineage for a normalized validation subject.
///
/// GET /api/v1/sos/validation-lineage
#[utoipa::path(
    get,
    path = "/api/v1/sos/validation-lineage",
    params(ValidationLineageQuery),
    responses(
        (status = 200, description = "Validation lineage retrieved", body = ValidationLineageResponse),
        (status = 404, description = "No validation lineage found", body = SosErrorResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Validation"
)]
pub async fn get_validation_lineage(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ValidationLineageQuery>,
) -> Result<Json<ValidationLineageResponse>, (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(&state)?;
    let limit = normalize_validation_limit(query.limit);

    service
        .get_validation_lineage(&query.subject_key, query.subject_type.as_deref(), limit)
        .map(Json)
        .map_err(sos_validation_error_response)
}

fn normalize_validation_limit(limit: usize) -> usize {
    if limit == 0 {
        50
    } else {
        limit
    }
}

fn sos_validation_service(
    state: &Arc<ApiState>,
) -> Result<SosValidationService, (StatusCode, Json<SosErrorResponse>)> {
    SosValidationService::from_api_state(state).map_err(sos_validation_error_response)
}

fn project_system_upsert(
    state: &Arc<ApiState>,
    system: &super::storage::System,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_system_upsert(system)
        .map_err(sos_validation_error_response)
}

fn project_system_delete(
    state: &Arc<ApiState>,
    system_id: &str,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_system_delete(system_id)
        .map_err(sos_validation_error_response)
}

fn project_interface_upsert(
    state: &Arc<ApiState>,
    interface: &super::storage::Interface,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_interface_upsert(interface)
        .map_err(sos_validation_error_response)
}

fn project_interface_delete(
    state: &Arc<ApiState>,
    interface_id: &str,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_interface_delete(interface_id)
        .map_err(sos_validation_error_response)
}

fn project_contract_upsert(
    state: &Arc<ApiState>,
    contract: &super::storage::Contract,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_contract_upsert(contract)
        .map_err(sos_validation_error_response)
}

fn project_contract_delete(
    state: &Arc<ApiState>,
    contract_id: &str,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    let service = sos_validation_service(state)?;
    service
        .project_contract_delete(contract_id)
        .map_err(sos_validation_error_response)
}

fn sos_validation_error_response(
    error: SosValidationServiceError,
) -> (StatusCode, Json<SosErrorResponse>) {
    let (status, code) = match &error {
        SosValidationServiceError::InvalidRequest(_) => {
            (StatusCode::BAD_REQUEST, "INVALID_REQUEST")
        }
        SosValidationServiceError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        SosValidationServiceError::Unavailable(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE")
        }
        SosValidationServiceError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
        }
    };

    (
        status,
        Json(SosErrorResponse {
            error: code.to_string(),
            message: error.to_string(),
            details: None,
        }),
    )
}

fn auth_error_response(status: StatusCode) -> (StatusCode, Json<SosErrorResponse>) {
    let (error, message) = match status {
        StatusCode::UNAUTHORIZED => ("UNAUTHORIZED", "Authentication required"),
        StatusCode::FORBIDDEN => ("FORBIDDEN", "Admin access required"),
        _ => ("AUTH_ERROR", "Authorization failed"),
    };

    (
        status,
        Json(SosErrorResponse {
            error: error.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
}

fn require_admin_claims(
    claims: Option<&Extension<Claims>>,
) -> Result<&Claims, (StatusCode, Json<SosErrorResponse>)> {
    let claims = claims
        .map(|extension| &extension.0)
        .ok_or_else(|| auth_error_response(StatusCode::UNAUTHORIZED))?;
    if !claims.role.can_admin() {
        return Err(auth_error_response(StatusCode::FORBIDDEN));
    }
    Ok(claims)
}

fn map_storage_error_response<E: std::fmt::Display>(
    error: E,
) -> (StatusCode, Json<SosErrorResponse>) {
    sos_validation_error_response(SosValidationServiceError::Internal(format!(
        "SoS storage error: {}",
        error
    )))
}

fn data_contract_response(
    service: &SosValidationService,
    contract: super::storage::Contract,
) -> Result<DataContractResponse, (StatusCode, Json<SosErrorResponse>)> {
    service
        .contract_response(contract)
        .map_err(sos_validation_error_response)
}

fn current_contract_approval_context(
    storage_manager: &super::storage::SosStorageManager,
    contract: &super::storage::Contract,
) -> anyhow::Result<(
    Option<super::storage::ContractApprovalRequestRecord>,
    Vec<super::storage::ContractApprovalEvidenceRecord>,
)> {
    let request = storage_manager
        .list_contract_approval_requests(&contract.contract_id, usize::MAX)?
        .into_iter()
        .filter(|request| request.contract_revision == contract.revision)
        .filter(|request| request.status == "approved")
        .max_by(|left, right| left.requested_at.cmp(&right.requested_at));

    let evidence = match &request {
        Some(request) => storage_manager.list_contract_approval_evidence(&request.request_id)?,
        None => Vec::new(),
    };

    Ok((request, evidence))
}

struct ResolvedContractSigningKey {
    signing_key: SigningKey,
    signing_key_ref: Option<String>,
    signing_key_version: Option<String>,
    signing_key_source: String,
    supports_rotation: bool,
    public_key: String,
    key_fingerprint: String,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
    secret_metadata: SecretMetadata,
}

async fn resolve_contract_signing_key(
    state: &Arc<ApiState>,
    initialize_if_missing: bool,
) -> Result<ResolvedContractSigningKey, (StatusCode, Json<SosErrorResponse>)> {
    const SIGNING_KEY_REF_ENV: &str = "GRAPHICA_SOS_CONTRACT_SIGNING_KEY_REF";
    const SIGNING_KEY_HEX_ENV: &str = "GRAPHICA_SOS_CONTRACT_SIGNING_KEY_HEX";
    const DEFAULT_SIGNING_KEY_REF: &str = "sos/contracts/signing-key";

    let signing_key_ref =
        std::env::var(SIGNING_KEY_REF_ENV).unwrap_or_else(|_| DEFAULT_SIGNING_KEY_REF.to_string());

    if let Some(registry) = state.secret_store_registry.as_ref() {
        if let Some(store) = registry.default() {
            match get_secret_by_ref(store.as_ref(), &signing_key_ref, None).await {
                Ok(secret) => {
                    return resolved_contract_signing_key_from_secret(
                        secret,
                        Some(signing_key_ref),
                        "secret_store",
                        true,
                    );
                }
                Err(graphica_core::secrets::SecretError::NotFound(_)) if initialize_if_missing => {
                    let mut seed = [0u8; 32];
                    OsRng.fill_bytes(&mut seed);
                    let metadata = contract_signing_secret_metadata(
                        None,
                        "system",
                        Some("auto-initialized on first SoS contract signing"),
                        None,
                    );
                    let version = put_secret_by_ref(
                        store.as_ref(),
                        &signing_key_ref,
                        SecretValue::Binary(seed.to_vec()),
                        Some(metadata),
                    )
                    .await
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(SosErrorResponse {
                                error: "SIGNATURE_CONFIGURATION_ERROR".to_string(),
                                message: format!(
                                    "Failed to initialize contract signing key in secret store '{}': {}",
                                    signing_key_ref, error
                                ),
                                details: None,
                            }),
                        )
                    })?;
                    let secret = get_secret_by_ref(store.as_ref(), &signing_key_ref, Some(&version))
                        .await
                        .map_err(|error| {
                            contract_signing_key_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "SIGNATURE_CONFIGURATION_ERROR",
                                format!(
                                    "Failed to load initialized contract signing key from secret store '{}': {}",
                                    signing_key_ref, error
                                ),
                            )
                        })?;
                    return resolved_contract_signing_key_from_secret(
                        secret,
                        Some(signing_key_ref),
                        "secret_store",
                        true,
                    );
                }
                Err(graphica_core::secrets::SecretError::NotFound(_)) => {
                    return Err(contract_signing_key_error(
                        StatusCode::NOT_FOUND,
                        "SIGNING_KEY_NOT_FOUND",
                        format!(
                            "Contract signing key '{}' has not been initialized in the default secret store",
                            signing_key_ref
                        ),
                    ));
                }
                Err(error) => {
                    return Err(contract_signing_key_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SIGNATURE_CONFIGURATION_ERROR",
                        format!(
                            "Failed to load contract signing key from secret store '{}': {}",
                            signing_key_ref, error
                        ),
                    ));
                }
            }
        }
    }

    let seed_hex = std::env::var(SIGNING_KEY_HEX_ENV).map_err(|_| {
        let (status, code, message) = if initialize_if_missing {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "SIGNATURE_CONFIGURATION_ERROR",
                format!(
                    "No default secret store is configured and {} is not set",
                    SIGNING_KEY_HEX_ENV
                ),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                "SIGNING_KEY_NOT_FOUND",
                "No SoS contract signing key is configured".to_string(),
            )
        };
        contract_signing_key_error(status, code, message)
    })?;
    decode_ed25519_signing_seed(&seed_hex)
        .map(|seed| {
            let signing_key = SigningKey::from_bytes(&seed);
            let (public_key, key_fingerprint) = contract_signing_key_public_material(&signing_key);
            ResolvedContractSigningKey {
                signing_key,
                signing_key_ref: None,
                signing_key_version: None,
                signing_key_source: "env".to_string(),
                supports_rotation: false,
                public_key,
                key_fingerprint,
                created_at: None,
                updated_at: None,
                secret_metadata: SecretMetadata::default(),
            }
        })
        .map_err(|message| {
            contract_signing_key_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SIGNATURE_CONFIGURATION_ERROR",
                format!("Invalid contract signing key seed: {}", message),
            )
        })
}

fn resolved_contract_signing_key_from_secret(
    secret: graphica_core::secrets::Secret,
    signing_key_ref: Option<String>,
    signing_key_source: &str,
    supports_rotation: bool,
) -> Result<ResolvedContractSigningKey, (StatusCode, Json<SosErrorResponse>)> {
    let signing_key = signing_key_from_secret_value(&secret.value).map_err(|message| {
        contract_signing_key_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SIGNATURE_CONFIGURATION_ERROR",
            message,
        )
    })?;
    let (public_key, key_fingerprint) = contract_signing_key_public_material(&signing_key);
    Ok(ResolvedContractSigningKey {
        signing_key,
        signing_key_ref,
        signing_key_version: Some(secret.version),
        signing_key_source: signing_key_source.to_string(),
        supports_rotation,
        public_key,
        key_fingerprint,
        created_at: Some(secret.created_at),
        updated_at: Some(secret.updated_at),
        secret_metadata: secret.metadata,
    })
}

fn contract_signing_secret_metadata(
    existing: Option<SecretMetadata>,
    actor: &str,
    reason: Option<&str>,
    previous_key_fingerprint: Option<&str>,
) -> SecretMetadata {
    let mut metadata = existing.unwrap_or_default();
    let now = chrono::Utc::now();

    if metadata.description.is_none() {
        metadata.description = Some("Graphica SoS contract signing key (Ed25519 seed)".to_string());
    }
    if metadata.owner.is_none() {
        metadata.owner = Some("sos_validation".to_string());
    }
    for tag in ["sos", "contract-signing"] {
        if !metadata.tags.iter().any(|existing| existing == tag) {
            metadata.tags.push(tag.to_string());
        }
    }

    let mut rotation_policy =
        metadata
            .rotation_policy
            .take()
            .unwrap_or(graphica_core::secrets::types::RotationPolicy {
                interval_days: 90,
                last_rotated: None,
                next_rotation: None,
                auto_rotate: false,
            });
    rotation_policy.last_rotated = Some(now);
    rotation_policy.next_rotation =
        Some(now + chrono::Duration::days(rotation_policy.interval_days as i64));
    metadata.rotation_policy = Some(rotation_policy);

    metadata.custom.insert(
        "algorithm".to_string(),
        Value::String("ed25519".to_string()),
    );
    metadata.custom.insert(
        "usage".to_string(),
        Value::String("sos_contract_signing".to_string()),
    );
    metadata.custom.insert(
        "managed_by".to_string(),
        Value::String("graphica_sos_validation".to_string()),
    );
    metadata
        .custom
        .insert("rotated_by".to_string(), Value::String(actor.to_string()));
    if let Some(reason) = reason {
        metadata.custom.insert(
            "rotation_reason".to_string(),
            Value::String(reason.to_string()),
        );
    }
    if let Some(previous_key_fingerprint) = previous_key_fingerprint {
        metadata.custom.insert(
            "previous_key_fingerprint".to_string(),
            Value::String(previous_key_fingerprint.to_string()),
        );
    }

    metadata
}

fn contract_signing_key_status_response(
    resolved: &ResolvedContractSigningKey,
) -> SosContractSigningKeyStatusResponse {
    let rotation = resolved.secret_metadata.rotation_policy.as_ref();
    SosContractSigningKeyStatusResponse {
        signing_key_ref: resolved.signing_key_ref.clone(),
        signing_key_source: resolved.signing_key_source.clone(),
        signing_key_version: resolved.signing_key_version.clone(),
        public_key: resolved.public_key.clone(),
        key_fingerprint: resolved.key_fingerprint.clone(),
        supports_rotation: resolved.supports_rotation,
        description: resolved.secret_metadata.description.clone(),
        tags: resolved.secret_metadata.tags.clone(),
        owner: resolved.secret_metadata.owner.clone(),
        created_at: resolved.created_at.map(|value| value.to_rfc3339()),
        updated_at: resolved.updated_at.map(|value| value.to_rfc3339()),
        rotation_interval_days: rotation.map(|value| value.interval_days),
        rotation_last_rotated_at: rotation
            .and_then(|value| value.last_rotated.as_ref().map(|time| time.to_rfc3339())),
        rotation_next_due_at: rotation
            .and_then(|value| value.next_rotation.as_ref().map(|time| time.to_rfc3339())),
        rotation_auto_rotate: rotation.map(|value| value.auto_rotate),
        metadata: resolved.secret_metadata.custom.clone(),
    }
}

fn contract_signing_key_error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<SosErrorResponse>) {
    (
        status,
        Json(SosErrorResponse {
            error: code.to_string(),
            message: message.into(),
            details: None,
        }),
    )
}

#[derive(Debug, Clone)]
struct PolicySigningTrustSettings {
    trust_mode: String,
    trust_provider: Option<String>,
    external_key_ref: Option<String>,
    trust_attestation_ref: Option<String>,
}

impl PolicySigningTrustSettings {
    fn as_metadata(&self) -> HashMap<String, Value> {
        let mut metadata = HashMap::from([(
            POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY.to_string(),
            Value::String(self.trust_mode.clone()),
        )]);

        if let Some(trust_provider) = &self.trust_provider {
            metadata.insert(
                POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY.to_string(),
                Value::String(trust_provider.clone()),
            );
        }
        if let Some(external_key_ref) = &self.external_key_ref {
            metadata.insert(
                POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY.to_string(),
                Value::String(external_key_ref.clone()),
            );
        }
        if let Some(trust_attestation_ref) = &self.trust_attestation_ref {
            metadata.insert(
                POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY.to_string(),
                Value::String(trust_attestation_ref.clone()),
            );
        }

        metadata
    }
}

#[derive(Debug, Clone)]
struct ResolvedPolicySigningKey {
    signing_key: SigningKey,
    signing_key_ref: Option<String>,
    signing_key_version: Option<String>,
    signing_key_source: String,
    supports_rotation: bool,
    public_key: String,
    key_fingerprint: String,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
    secret_metadata: SecretMetadata,
    trust_settings: PolicySigningTrustSettings,
}

impl ResolvedPolicySigningKey {
    fn signing_material(&self) -> PolicyAttestationSigningMaterial {
        PolicyAttestationSigningMaterial {
            signing_key: self.signing_key.clone(),
            signing_key_ref: self.signing_key_ref.clone(),
            signing_key_version: self.signing_key_version.clone(),
            signing_key_source: self.signing_key_source.clone(),
            metadata: self.trust_settings.as_metadata(),
        }
    }
}

async fn resolve_policy_signing_key(
    state: &Arc<ApiState>,
    initialize_if_missing: bool,
) -> Result<ResolvedPolicySigningKey, (StatusCode, Json<SosErrorResponse>)> {
    const SIGNING_KEY_REF_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_KEY_REF";
    const SIGNING_KEY_HEX_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_KEY_HEX";
    const DEFAULT_SIGNING_KEY_REF: &str = "sos/policies/signing-key";

    let signing_key_ref =
        std::env::var(SIGNING_KEY_REF_ENV).unwrap_or_else(|_| DEFAULT_SIGNING_KEY_REF.to_string());

    if let Some(registry) = state.secret_store_registry.as_ref() {
        if let Some(store) = registry.default() {
            match get_secret_by_ref(store.as_ref(), &signing_key_ref, None).await {
                Ok(secret) => {
                    return resolved_policy_signing_key_from_secret(
                        secret,
                        Some(signing_key_ref),
                        "secret_store",
                        true,
                    );
                }
                Err(graphica_core::secrets::SecretError::NotFound(_)) if initialize_if_missing => {
                    let mut seed = [0u8; 32];
                    OsRng.fill_bytes(&mut seed);
                    let trust_settings = resolve_policy_signing_trust_settings(None)?;
                    let version = put_secret_by_ref(
                        store.as_ref(),
                        &signing_key_ref,
                        SecretValue::Binary(seed.to_vec()),
                        Some(policy_signing_secret_metadata(
                            None,
                            "system",
                            Some("auto-initialized on first SoS policy approval"),
                            None,
                            &trust_settings,
                        )),
                    )
                    .await
                    .map_err(|error| {
                        policy_signing_key_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "SIGNATURE_CONFIGURATION_ERROR",
                            format!(
                                "Failed to initialize policy signing key in secret store '{}': {}",
                                signing_key_ref, error
                            ),
                        )
                    })?;
                    let secret = get_secret_by_ref(store.as_ref(), &signing_key_ref, Some(&version))
                        .await
                        .map_err(|error| {
                            policy_signing_key_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "SIGNATURE_CONFIGURATION_ERROR",
                                format!(
                                    "Failed to load initialized policy signing key from secret store '{}': {}",
                                    signing_key_ref, error
                                ),
                            )
                        })?;
                    return resolved_policy_signing_key_from_secret(
                        secret,
                        Some(signing_key_ref),
                        "secret_store",
                        true,
                    );
                }
                Err(graphica_core::secrets::SecretError::NotFound(_)) => {
                    return Err(policy_signing_key_error(
                        StatusCode::NOT_FOUND,
                        "SIGNING_KEY_NOT_FOUND",
                        format!(
                            "Policy signing key '{}' has not been initialized in the default secret store",
                            signing_key_ref
                        ),
                    ));
                }
                Err(error) => {
                    return Err(policy_signing_key_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SIGNATURE_CONFIGURATION_ERROR",
                        format!(
                            "Failed to load policy signing key from secret store '{}': {}",
                            signing_key_ref, error
                        ),
                    ));
                }
            }
        }
    }

    let seed_hex = std::env::var(SIGNING_KEY_HEX_ENV).map_err(|_| {
        let (status, code, message) = if state.secret_store_registry.is_some() {
            (
                StatusCode::NOT_FOUND,
                "SIGNING_KEY_NOT_FOUND",
                "No SoS policy signing key is configured".to_string(),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                "SIGNING_KEY_NOT_FOUND",
                format!(
                    "No default secret store is configured and {} is not set",
                    SIGNING_KEY_HEX_ENV
                ),
            )
        };
        policy_signing_key_error(status, code, message)
    })?;

    let trust_settings = resolve_policy_signing_trust_settings(None)?;
    decode_ed25519_signing_seed(&seed_hex)
        .map(|seed| {
            let signing_key = SigningKey::from_bytes(&seed);
            let (public_key, key_fingerprint) = policy_signing_key_public_material(&signing_key);
            ResolvedPolicySigningKey {
                signing_key,
                signing_key_ref: None,
                signing_key_version: None,
                signing_key_source: "env".to_string(),
                supports_rotation: false,
                public_key,
                key_fingerprint,
                created_at: None,
                updated_at: None,
                secret_metadata: SecretMetadata::default(),
                trust_settings,
            }
        })
        .map_err(|message| {
            policy_signing_key_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SIGNATURE_CONFIGURATION_ERROR",
                format!("Invalid policy signing key seed: {}", message),
            )
        })
}

fn resolved_policy_signing_key_from_secret(
    secret: graphica_core::secrets::Secret,
    signing_key_ref: Option<String>,
    signing_key_source: &str,
    supports_rotation: bool,
) -> Result<ResolvedPolicySigningKey, (StatusCode, Json<SosErrorResponse>)> {
    let signing_key = signing_key_from_secret_value(&secret.value).map_err(|message| {
        policy_signing_key_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SIGNATURE_CONFIGURATION_ERROR",
            message,
        )
    })?;
    let (public_key, key_fingerprint) = policy_signing_key_public_material(&signing_key);
    let trust_settings = resolve_policy_signing_trust_settings(Some(&secret.metadata))?;
    Ok(ResolvedPolicySigningKey {
        signing_key,
        signing_key_ref,
        signing_key_version: Some(secret.version),
        signing_key_source: signing_key_source.to_string(),
        supports_rotation,
        public_key,
        key_fingerprint,
        created_at: Some(secret.created_at),
        updated_at: Some(secret.updated_at),
        secret_metadata: secret.metadata,
        trust_settings,
    })
}

fn resolve_policy_signing_trust_settings(
    secret_metadata: Option<&SecretMetadata>,
) -> Result<PolicySigningTrustSettings, (StatusCode, Json<SosErrorResponse>)> {
    const TRUST_MODE_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_TRUST_MODE";
    const TRUST_PROVIDER_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_TRUST_PROVIDER";
    const EXTERNAL_KEY_REF_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_EXTERNAL_KEY_REF";
    const TRUST_ATTESTATION_REF_ENV: &str = "GRAPHICA_SOS_POLICY_SIGNING_TRUST_ATTESTATION_REF";

    let custom = secret_metadata.map(|metadata| &metadata.custom);
    let trust_mode_raw = metadata_or_env_string(
        custom,
        POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY,
        TRUST_MODE_ENV,
    )
    .unwrap_or_else(|| POLICY_ATTESTATION_DEFAULT_TRUST_MODE.to_string());
    let trust_provider = metadata_or_env_string(
        custom,
        POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
        TRUST_PROVIDER_ENV,
    );
    let external_key_ref = metadata_or_env_string(
        custom,
        POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
        EXTERNAL_KEY_REF_ENV,
    );
    let trust_attestation_ref = metadata_or_env_string(
        custom,
        POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
        TRUST_ATTESTATION_REF_ENV,
    );

    let mut trust_settings = PolicySigningTrustSettings {
        trust_mode: parse_policy_signing_trust_mode(&trust_mode_raw).map_err(|message| {
            policy_signing_key_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SIGNATURE_CONFIGURATION_ERROR",
                message,
            )
        })?,
        trust_provider,
        external_key_ref,
        trust_attestation_ref,
    };
    if trust_settings.trust_mode == POLICY_ATTESTATION_DEFAULT_TRUST_MODE
        && (trust_settings.trust_provider.is_some()
            || trust_settings.external_key_ref.is_some()
            || trust_settings.trust_attestation_ref.is_some())
    {
        trust_settings.trust_mode = "external_reference".to_string();
    }
    if trust_settings.trust_mode == POLICY_ATTESTATION_DEFAULT_TRUST_MODE {
        trust_settings.trust_provider = None;
        trust_settings.external_key_ref = None;
        trust_settings.trust_attestation_ref = None;
    }

    Ok(trust_settings)
}

fn apply_policy_trust_overrides(
    trust_settings: &mut PolicySigningTrustSettings,
    request: &RotateSosPolicySigningKeyRequest,
) -> Result<(), (StatusCode, Json<SosErrorResponse>)> {
    if let Some(trust_mode) = request.trust_mode.as_deref() {
        trust_settings.trust_mode =
            parse_policy_signing_trust_mode(trust_mode).map_err(|message| {
                policy_signing_key_error(StatusCode::BAD_REQUEST, "INVALID_REQUEST", message)
            })?;
    }
    if request.trust_provider.is_some() {
        trust_settings.trust_provider = normalize_optional_text(request.trust_provider.as_deref());
    }
    if request.external_key_ref.is_some() {
        trust_settings.external_key_ref =
            normalize_optional_text(request.external_key_ref.as_deref());
    }
    if request.trust_attestation_ref.is_some() {
        trust_settings.trust_attestation_ref =
            normalize_optional_text(request.trust_attestation_ref.as_deref());
    }

    if request.trust_mode.is_none()
        && (request.trust_provider.is_some()
            || request.external_key_ref.is_some()
            || request.trust_attestation_ref.is_some())
    {
        trust_settings.trust_mode = "external_reference".to_string();
    }

    if trust_settings.trust_mode == POLICY_ATTESTATION_DEFAULT_TRUST_MODE {
        trust_settings.trust_provider = None;
        trust_settings.external_key_ref = None;
        trust_settings.trust_attestation_ref = None;
    }

    Ok(())
}

fn parse_policy_signing_trust_mode(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "software" => Ok(POLICY_ATTESTATION_DEFAULT_TRUST_MODE.to_string()),
        "external_reference" | "external" | "kms" | "hsm" => {
            Ok("external_reference".to_string())
        }
        _ => Err(format!(
            "Unsupported policy signing trust_mode '{}'; expected 'software' or 'external_reference'",
            value
        )),
    }
}

fn policy_signing_secret_metadata(
    existing: Option<SecretMetadata>,
    actor: &str,
    reason: Option<&str>,
    previous_key_fingerprint: Option<&str>,
    trust_settings: &PolicySigningTrustSettings,
) -> SecretMetadata {
    let mut metadata = existing.unwrap_or_default();
    let now = chrono::Utc::now();

    if metadata.description.is_none() {
        metadata.description = Some("Graphica SoS policy signing key (Ed25519 seed)".to_string());
    }
    if metadata.owner.is_none() {
        metadata.owner = Some("sos_validation".to_string());
    }
    for tag in ["sos", "policy-signing"] {
        if !metadata.tags.iter().any(|existing| existing == tag) {
            metadata.tags.push(tag.to_string());
        }
    }

    let mut rotation_policy =
        metadata
            .rotation_policy
            .take()
            .unwrap_or(graphica_core::secrets::types::RotationPolicy {
                interval_days: 90,
                last_rotated: None,
                next_rotation: None,
                auto_rotate: false,
            });
    rotation_policy.last_rotated = Some(now);
    rotation_policy.next_rotation =
        Some(now + chrono::Duration::days(rotation_policy.interval_days as i64));
    metadata.rotation_policy = Some(rotation_policy);

    metadata.custom.insert(
        "algorithm".to_string(),
        Value::String("ed25519".to_string()),
    );
    metadata.custom.insert(
        "usage".to_string(),
        Value::String("sos_policy_signing".to_string()),
    );
    metadata.custom.insert(
        "managed_by".to_string(),
        Value::String("graphica_sos_validation".to_string()),
    );
    metadata
        .custom
        .insert("rotated_by".to_string(), Value::String(actor.to_string()));
    if let Some(reason) = reason {
        metadata.custom.insert(
            "rotation_reason".to_string(),
            Value::String(reason.to_string()),
        );
    }
    if let Some(previous_key_fingerprint) = previous_key_fingerprint {
        metadata.custom.insert(
            "previous_key_fingerprint".to_string(),
            Value::String(previous_key_fingerprint.to_string()),
        );
    }

    for (key, value) in trust_settings.as_metadata() {
        metadata.custom.insert(key, value);
    }

    metadata
}

fn policy_signing_key_status_response(
    resolved: &ResolvedPolicySigningKey,
) -> SosPolicySigningKeyStatusResponse {
    let rotation = resolved.secret_metadata.rotation_policy.as_ref();
    SosPolicySigningKeyStatusResponse {
        signing_key_ref: resolved.signing_key_ref.clone(),
        signing_key_source: resolved.signing_key_source.clone(),
        signing_key_version: resolved.signing_key_version.clone(),
        public_key: resolved.public_key.clone(),
        key_fingerprint: resolved.key_fingerprint.clone(),
        supports_rotation: resolved.supports_rotation,
        trust_mode: resolved.trust_settings.trust_mode.clone(),
        trust_provider: resolved.trust_settings.trust_provider.clone(),
        external_key_ref: resolved.trust_settings.external_key_ref.clone(),
        trust_attestation_ref: resolved.trust_settings.trust_attestation_ref.clone(),
        description: resolved.secret_metadata.description.clone(),
        tags: resolved.secret_metadata.tags.clone(),
        owner: resolved.secret_metadata.owner.clone(),
        created_at: resolved.created_at.map(|value| value.to_rfc3339()),
        updated_at: resolved.updated_at.map(|value| value.to_rfc3339()),
        rotation_interval_days: rotation.map(|value| value.interval_days),
        rotation_last_rotated_at: rotation
            .and_then(|value| value.last_rotated.as_ref().map(|time| time.to_rfc3339())),
        rotation_next_due_at: rotation
            .and_then(|value| value.next_rotation.as_ref().map(|time| time.to_rfc3339())),
        rotation_auto_rotate: rotation.map(|value| value.auto_rotate),
        metadata: resolved.secret_metadata.custom.clone(),
    }
}

fn metadata_or_env_string(
    custom: Option<&HashMap<String, Value>>,
    metadata_key: &str,
    env_key: &str,
) -> Option<String> {
    custom
        .and_then(|custom| custom.get(metadata_key))
        .and_then(|value| value.as_str().map(ToString::to_string))
        .or_else(|| env_nonempty(env_key))
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(|value| normalize_optional_text(Some(value.as_str())))
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn policy_signing_key_error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<SosErrorResponse>) {
    (
        status,
        Json(SosErrorResponse {
            error: code.to_string(),
            message: message.into(),
            details: None,
        }),
    )
}

fn signing_key_from_secret_value(value: &SecretValue) -> Result<SigningKey, String> {
    match value {
        SecretValue::Binary(bytes) => {
            let seed: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| "binary signing key seed must be exactly 32 bytes".to_string())?;
            Ok(SigningKey::from_bytes(&seed))
        }
        SecretValue::String(raw) => {
            decode_ed25519_signing_seed(raw).map(|seed| SigningKey::from_bytes(&seed))
        }
        SecretValue::KeyValue(values) => {
            let candidate = values
                .get("seed_hex")
                .or_else(|| values.get("private_key_hex"))
                .ok_or_else(|| {
                    "key-value signing secrets must contain 'seed_hex' or 'private_key_hex'"
                        .to_string()
                })?;
            decode_ed25519_signing_seed(candidate).map(|seed| SigningKey::from_bytes(&seed))
        }
        SecretValue::Json(value) => {
            let candidate = value
                .get("seed_hex")
                .or_else(|| value.get("private_key_hex"))
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    "JSON signing secrets must contain string field 'seed_hex' or 'private_key_hex'"
                        .to_string()
                })?;
            decode_ed25519_signing_seed(candidate).map(|seed| SigningKey::from_bytes(&seed))
        }
    }
}

fn decode_ed25519_signing_seed(raw: &str) -> Result<[u8; 32], String> {
    let normalized = raw.trim();
    if normalized.len() != 64 {
        return Err("expected 64 hex characters".to_string());
    }

    let mut seed = [0u8; 32];
    for (index, chunk) in normalized.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk)
            .map_err(|_| "signing seed must be valid UTF-8 hex".to_string())?;
        seed[index] =
            u8::from_str_radix(hex, 16).map_err(|_| format!("invalid hex byte '{}'", hex))?;
    }

    Ok(seed)
}

fn request_actor(claims: Option<&Extension<Claims>>) -> String {
    claims
        .map(|extension| extension.0.sub.clone())
        .unwrap_or_else(|| "system".to_string())
}
