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
    Json,
};
use std::sync::Arc;

use super::types::*;
use crate::api::ApiState;

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

    // 4. TODO: Store RDF representation in governance store
    // This will enable SPARQL-based policy validation and governance integration

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

    // TODO: Update RDF store representation
    // This will enable SPARQL-based policy validation and governance integration

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
            // TODO: Delete from RDF store when RDF integration is complete

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
    let interface = Interface {
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

    // 6. TODO: Store RDF representation in governance store
    // This will enable SPARQL-based policy validation

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

    // TODO: Update RDF store representation

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
            // TODO: Delete from RDF store when RDF integration is complete

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
    // TODO: Implement schema validation
    // Use JSON Schema validator to validate data against interface schema

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "Schema validation not yet implemented".to_string(),
            details: None,
        }),
    ))
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
    Json(request): Json<CreateDataContractRequest>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Contract;
    use super::validators::validate_sla_metrics;
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

    // ========================================================================
    // STEP 7: Create Contract entity and store in RocksDB
    // ========================================================================
    let now = Utc::now();
    let contract = Contract {
        contract_id: request.contract_id.clone(),
        contract_name: request.contract_name.clone(),
        provider_interface_id: request.provider_interface_id.clone(),
        consumer_interface_id: request.consumer_interface_id.clone(),
        sla_metrics: sla_metrics,
        transformation_rules: request.transformation_rules.clone(),
        description: request.description.clone(),
        tags: request.tags.clone(),
        approved: false, // New contracts start as unapproved
        signed: false,   // New contracts start as unsigned
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

    // ========================================================================
    // STEP 8: TODO: Store RDF representation in governance store
    // This will enable SPARQL-based policy validation
    // ========================================================================

    // ========================================================================
    // STEP 9: Return DataContractResponse
    // ========================================================================
    Ok(Json(DataContractResponse {
        contract_id: contract.contract_id,
        contract_name: contract.contract_name,
        provider_interface_id: contract.provider_interface_id,
        consumer_interface_id: contract.consumer_interface_id,
        sla_metrics: request.sla_metrics,
        transformation_rules: contract.transformation_rules,
        description: contract.description,
        tags: contract.tags,
        approved: contract.approved,
        signed: contract.signed,
        created_at: contract.created_at.to_rfc3339(),
        updated_at: contract.updated_at.to_rfc3339(),
    }))
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
            let response_contracts: Vec<DataContractResponse> = contracts
                .into_iter()
                .map(|contract| {
                    // Convert storage SLA metrics to response format
                    let sla_metrics = contract
                        .sla_metrics
                        .iter()
                        .map(|m| SlaMetric {
                            name: m.name.clone(),
                            value: m.value,
                            operator: m.operator.clone(),
                            unit: m.unit.clone(),
                        })
                        .collect();

                    DataContractResponse {
                        contract_id: contract.contract_id,
                        contract_name: contract.contract_name,
                        provider_interface_id: contract.provider_interface_id,
                        consumer_interface_id: contract.consumer_interface_id,
                        sla_metrics,
                        transformation_rules: contract.transformation_rules,
                        description: contract.description,
                        tags: contract.tags,
                        approved: contract.approved,
                        signed: contract.signed,
                        created_at: contract.created_at.to_rfc3339(),
                        updated_at: contract.updated_at.to_rfc3339(),
                    }
                })
                .collect();

            Ok(Json(response_contracts))
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
            // Convert storage SLA metrics to response format
            let sla_metrics = contract
                .sla_metrics
                .iter()
                .map(|m| SlaMetric {
                    name: m.name.clone(),
                    value: m.value,
                    operator: m.operator.clone(),
                    unit: m.unit.clone(),
                })
                .collect();

            Ok(Json(DataContractResponse {
                contract_id: contract.contract_id,
                contract_name: contract.contract_name,
                provider_interface_id: contract.provider_interface_id,
                consumer_interface_id: contract.consumer_interface_id,
                sla_metrics,
                transformation_rules: contract.transformation_rules,
                description: contract.description,
                tags: contract.tags,
                approved: contract.approved,
                signed: contract.signed,
                created_at: contract.created_at.to_rfc3339(),
                updated_at: contract.updated_at.to_rfc3339(),
            }))
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
    Path(id): Path<String>,
    Json(request): Json<UpdateDataContractRequest>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Contract;
    use super::validators::validate_sla_metrics;
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

    // Check if contract is signed - signed contracts cannot be modified
    if contract.signed {
        return Err((
            StatusCode::CONFLICT,
            Json(SosErrorResponse {
                error: "CONTRACT_SIGNED".to_string(),
                message: "Signed contracts cannot be modified".to_string(),
                details: None,
            }),
        ));
    }

    // Update fields if provided
    if let Some(name) = request.contract_name {
        if !name.is_empty() {
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

        contract.sla_metrics = storage_metrics;
    }

    if let Some(transformation_rules) = request.transformation_rules {
        contract.transformation_rules = transformation_rules;
    }

    if let Some(description) = request.description {
        contract.description = Some(description);
    }

    if let Some(tags) = request.tags {
        contract.tags = tags;
    }

    // Update timestamp
    contract.updated_at = Utc::now();

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

    // Convert SLA metrics to response format
    let response_sla_metrics = contract
        .sla_metrics
        .iter()
        .map(|m| SlaMetric {
            name: m.name.clone(),
            value: m.value,
            operator: m.operator.clone(),
            unit: m.unit.clone(),
        })
        .collect();

    Ok(Json(DataContractResponse {
        contract_id: contract.contract_id,
        contract_name: contract.contract_name,
        provider_interface_id: contract.provider_interface_id,
        consumer_interface_id: contract.consumer_interface_id,
        sla_metrics: response_sla_metrics,
        transformation_rules: contract.transformation_rules,
        description: contract.description,
        tags: contract.tags,
        approved: contract.approved,
        signed: contract.signed,
        created_at: contract.created_at.to_rfc3339(),
        updated_at: contract.updated_at.to_rfc3339(),
    }))
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
    if contract.signed {
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

    Ok(StatusCode::NO_CONTENT)
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
    Path(id): Path<String>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Contract;
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

    // Set approved = true (idempotent operation)
    contract.approved = true;
    contract.updated_at = Utc::now();

    // Store updated contract
    if let Err(e) = storage_manager.put_contract(&contract) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SosErrorResponse {
                error: "DATABASE_ERROR".to_string(),
                message: format!("Failed to approve contract: {}", e),
                details: None,
            }),
        ));
    }

    // Convert SLA metrics to response format
    let response_sla_metrics = contract
        .sla_metrics
        .iter()
        .map(|m| SlaMetric {
            name: m.name.clone(),
            value: m.value,
            operator: m.operator.clone(),
            unit: m.unit.clone(),
        })
        .collect();

    Ok(Json(DataContractResponse {
        contract_id: contract.contract_id,
        contract_name: contract.contract_name,
        provider_interface_id: contract.provider_interface_id,
        consumer_interface_id: contract.consumer_interface_id,
        sla_metrics: response_sla_metrics,
        transformation_rules: contract.transformation_rules,
        description: contract.description,
        tags: contract.tags,
        approved: contract.approved,
        signed: contract.signed,
        created_at: contract.created_at.to_rfc3339(),
        updated_at: contract.updated_at.to_rfc3339(),
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
    Path(id): Path<String>,
) -> Result<Json<DataContractResponse>, (StatusCode, Json<SosErrorResponse>)> {
    use super::storage::Contract;
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

    // Check if contract is approved - cannot sign unapproved contracts
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

    // Set signed = true (idempotent operation)
    contract.signed = true;
    contract.updated_at = Utc::now();

    // Store updated contract
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

    // Convert SLA metrics to response format
    let response_sla_metrics = contract
        .sla_metrics
        .iter()
        .map(|m| SlaMetric {
            name: m.name.clone(),
            value: m.value,
            operator: m.operator.clone(),
            unit: m.unit.clone(),
        })
        .collect();

    Ok(Json(DataContractResponse {
        contract_id: contract.contract_id,
        contract_name: contract.contract_name,
        provider_interface_id: contract.provider_interface_id,
        consumer_interface_id: contract.consumer_interface_id,
        sla_metrics: response_sla_metrics,
        transformation_rules: contract.transformation_rules,
        description: contract.description,
        tags: contract.tags,
        approved: contract.approved,
        signed: contract.signed,
        created_at: contract.created_at.to_rfc3339(),
        updated_at: contract.updated_at.to_rfc3339(),
    }))
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
    // TODO: Implement validation
    // Match on request type and delegate to appropriate validator

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "Validation not yet implemented".to_string(),
            details: None,
        }),
    ))
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
    // TODO: Implement dry-run validation

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "Dry-run validation not yet implemented".to_string(),
            details: None,
        }),
    ))
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
    responses(
        (status = 200, description = "Compatibility matrix generated", body = CompatibilityMatrixResponse),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn get_compatibility_matrix(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<CompatibilityMatrixResponse>, (StatusCode, Json<SosErrorResponse>)> {
    // TODO: Implement compatibility matrix
    // Compute compatibility scores for all interface pairs

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "Compatibility matrix not yet implemented".to_string(),
            details: None,
        }),
    ))
}

/// Get system dependency graph
///
/// GET /api/v1/sos/dependency-graph
#[utoipa::path(
    get,
    path = "/api/v1/sos/dependency-graph",
    responses(
        (status = 200, description = "Dependency graph generated", body = serde_json::Value),
        (status = 500, description = "Internal server error", body = SosErrorResponse),
    ),
    tag = "SoS - Analytics"
)]
pub async fn get_dependency_graph(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<SosErrorResponse>)> {
    // TODO: Implement dependency graph
    // Build graph of systems and their contracts/interfaces

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "Dependency graph not yet implemented".to_string(),
            details: None,
        }),
    ))
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
    // TODO: Implement what-if analysis
    // Apply hypothetical changes and analyze impact

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(SosErrorResponse {
            error: "NOT_IMPLEMENTED".to_string(),
            message: "What-if analysis not yet implemented".to_string(),
            details: None,
        }),
    ))
}
