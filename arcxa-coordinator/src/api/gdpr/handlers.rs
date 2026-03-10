////! GDPR API Handlers
//!
//! HTTP handlers for GDPR compliance operations.

use super::types::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs;
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};

/// Erase all data for a tenant (GDPR Article 17: Right to Erasure)
///
/// This endpoint permanently deletes all tenant data from all storage backends.
/// It supports dry-run mode for previewing what will be deleted.
///
/// # Security
///
/// This endpoint should be heavily protected and audited. Ensure proper
/// authentication and authorization before allowing access.
#[utoipa::path(
    post,
    path = "/api/v1/gdpr/tenants/{tenant_id}/erase",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID to erase data for")
    ),
    request_body = EraseTenantDataRequest,
    responses(
        (status = 200, description = "Erasure completed", body = EraseTenantDataResponse),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn erase_tenant_data(
    State(state): State<Arc<ApiState>>,
    Path(tenant_id): Path<String>,
    Json(request): Json<EraseTenantDataRequest>,
) -> Result<Json<EraseTenantDataResponse>, (StatusCode, String)> {
    info!(
        tenant_id = %tenant_id,
        dry_run = %request.dry_run,
        "GDPR erasure request received"
    );

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    // Execute erasure
    let result = gdpr_coordinator
        .erase_tenant_data(&tenant_id, request.dry_run)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to erase tenant data");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erasure failed: {}", e),
            )
        })?;

    // Convert to API response
    let backend_results: Vec<BackendErasureDetail> = result
        .backend_results
        .into_iter()
        .map(|(backend_name, backend_result)| BackendErasureDetail {
            backend_name,
            success: backend_result.success,
            records_erased: backend_result.records_erased,
            error_message: backend_result.error_message,
        })
        .collect();

    let errors: Vec<String> = backend_results
        .iter()
        .filter_map(|r| r.error_message.clone())
        .collect();

    let response = EraseTenantDataResponse {
        success: result.success,
        total_records_erased: result.total_records_erased,
        backends_succeeded: result.backends_succeeded,
        backends_failed: result.backends_failed,
        dry_run: result.request.dry_run,
        backend_results,
        errors,
    };

    if response.success {
        info!(
            tenant_id = %tenant_id,
            records_erased = %response.total_records_erased,
            "GDPR erasure completed successfully"
        );
    } else {
        warn!(
            tenant_id = %tenant_id,
            records_erased = %response.total_records_erased,
            failed_backends = %response.backends_failed,
            "GDPR erasure completed with failures"
        );
    }

    Ok(Json(response))
}

/// Count all records for a tenant across all backends
///
/// This endpoint provides transparency about how much data exists for a tenant
/// before performing an erasure operation.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/tenants/{tenant_id}/count",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID to count data for")
    ),
    responses(
        (status = 200, description = "Count completed", body = TenantDataCountResponse),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn count_tenant_data(
    State(state): State<Arc<ApiState>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantDataCountResponse>, (StatusCode, String)> {
    info!(tenant_id = %tenant_id, "GDPR count request received");

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    // Get counts
    let total_records = gdpr_coordinator
        .count_tenant_data(&tenant_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to count tenant data");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Count failed: {}", e),
            )
        })?;

    let breakdown = gdpr_coordinator
        .get_tenant_data_breakdown(&tenant_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to get tenant data breakdown");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Breakdown failed: {}", e),
            )
        })?;

    let response = TenantDataCountResponse {
        tenant_id: tenant_id.clone(),
        total_records,
        breakdown,
    };

    info!(
        tenant_id = %tenant_id,
        total_records = %response.total_records,
        "GDPR count completed"
    );

    Ok(Json(response))
}

/// Verify that all tenant data has been erased
///
/// This endpoint confirms that the erasure operation was successful by checking
/// that no records remain for the tenant.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/tenants/{tenant_id}/verify",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID to verify erasure for")
    ),
    responses(
        (status = 200, description = "Verification completed", body = VerifyErasureResponse),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn verify_erasure(
    State(state): State<Arc<ApiState>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<VerifyErasureResponse>, (StatusCode, String)> {
    info!(tenant_id = %tenant_id, "GDPR verification request received");

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    // Verify erasure
    let verified = gdpr_coordinator
        .verify_erasure(&tenant_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to verify erasure");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Verification failed: {}", e),
            )
        })?;

    let remaining_records = if verified {
        0
    } else {
        gdpr_coordinator
            .count_tenant_data(&tenant_id)
            .await
            .unwrap_or(0)
    };

    let response = VerifyErasureResponse {
        tenant_id: tenant_id.clone(),
        verified,
        remaining_records,
    };

    if response.verified {
        info!(tenant_id = %tenant_id, "GDPR erasure verified");
    } else {
        warn!(
            tenant_id = %tenant_id,
            remaining_records = %response.remaining_records,
            "GDPR erasure verification failed - data still exists"
        );
    }

    Ok(Json(response))
}

// ============================================================================
// User-Level Erasure Endpoints (Enhanced GDPR Article 17 Support)
// ============================================================================

/// Erase all data for a user (GDPR Article 17: Right to Erasure)
///
/// This endpoint permanently deletes or anonymizes all user data from all storage backends.
/// It includes:
/// - Legal hold checking
/// - Retention policy enforcement
/// - Multiple erasure strategies (hard_delete, anonymize, tombstone)
/// - Dry-run mode for previewing changes
///
/// # Security
///
/// This endpoint should be protected and audited. Ensure proper authentication
/// and authorization before allowing access.
#[utoipa::path(
    post,
    path = "/api/v1/gdpr/users/{user_id}/erase",
    params(
        ("user_id" = String, Path, description = "User ID to erase data for")
    ),
    request_body = EraseUserDataRequest,
    responses(
        (status = 200, description = "Erasure completed", body = EraseUserDataResponse),
        (status = 400, description = "Invalid erasure strategy or user under legal hold"),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn erase_user_data(
    State(state): State<Arc<ApiState>>,
    Path(user_id): Path<String>,
    Json(request): Json<EraseUserDataRequest>,
) -> Result<Json<EraseUserDataResponse>, (StatusCode, String)> {
    info!(
        user_id = %user_id,
        dry_run = %request.dry_run,
        strategy = %request.strategy,
        "GDPR user erasure request received"
    );

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    // Parse erasure strategy
    use graphica_core::gdpr::ErasureStrategy;
    let strategy = match request.strategy.to_lowercase().as_str() {
        "hard_delete" => ErasureStrategy::HardDelete,
        "anonymize" => ErasureStrategy::Anonymize,
        "tombstone" => ErasureStrategy::Tombstone,
        "archive_then_delete" => ErasureStrategy::ArchiveThenDelete,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid erasure strategy: '{}'. Valid options: hard_delete, anonymize, tombstone, archive_then_delete", request.strategy),
            ));
        }
    };

    // Execute erasure
    let result = gdpr_coordinator
        .erase_user_data(&user_id, request.dry_run, strategy)
        .await
        .map_err(|e| {
            let error_msg = e.to_string();

            // Check if this is a legal hold error
            if error_msg.contains("legal hold") {
                error!(error = %e, "User is under legal hold");
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Cannot erase user data: {}", error_msg),
                );
            }

            error!(error = %e, "Failed to erase user data");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erasure failed: {}", e),
            )
        })?;

    // Convert to API response
    let backend_results: Vec<BackendErasureDetail> = result
        .backend_results
        .into_iter()
        .map(|(backend_name, backend_result)| BackendErasureDetail {
            backend_name,
            success: backend_result.success,
            records_erased: backend_result.records_erased,
            error_message: backend_result.error_message,
        })
        .collect();

    let errors: Vec<String> = backend_results
        .iter()
        .filter_map(|r| r.error_message.clone())
        .collect();

    let response = EraseUserDataResponse {
        success: result.success,
        total_records_erased: result.total_records_erased,
        backends_succeeded: result.backends_succeeded,
        backends_failed: result.backends_failed,
        dry_run: result.request.dry_run,
        strategy: request.strategy,
        backend_results,
        errors,
        warnings: Vec::new(), // TODO: Add retention policy warnings
    };

    if response.success {
        info!(
            user_id = %user_id,
            records_erased = %response.total_records_erased,
            strategy = %response.strategy,
            "GDPR user erasure completed successfully"
        );
    } else {
        warn!(
            user_id = %user_id,
            records_erased = %response.total_records_erased,
            failed_backends = %response.backends_failed,
            "GDPR user erasure completed with failures"
        );
    }

    Ok(Json(response))
}

/// Count all records for a user across all backends
///
/// This endpoint provides transparency about how much data exists for a user
/// before performing an erasure operation.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/users/{user_id}/count",
    params(
        ("user_id" = String, Path, description = "User ID to count data for")
    ),
    responses(
        (status = 200, description = "Count completed", body = UserDataCountResponse),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn count_user_data(
    State(state): State<Arc<ApiState>>,
    Path(user_id): Path<String>,
) -> Result<Json<UserDataCountResponse>, (StatusCode, String)> {
    info!(user_id = %user_id, "GDPR user count request received");

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    let total_records = gdpr_coordinator
        .count_user_data(&user_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to count user data");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Count failed: {}", e),
            )
        })?;

    let breakdown = gdpr_coordinator
        .get_user_data_breakdown(&user_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to get user data breakdown");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Breakdown failed: {}", e),
            )
        })?;

    let response = UserDataCountResponse {
        user_id: user_id.clone(),
        total_records,
        breakdown,
    };

    info!(
        user_id = %user_id,
        total_records = %response.total_records,
        "GDPR user count completed"
    );

    Ok(Json(response))
}

/// Check if a user is under legal hold
///
/// This endpoint checks whether a user has active legal holds that would
/// prevent data erasure.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/users/{user_id}/legal-holds",
    params(
        ("user_id" = String, Path, description = "User ID to check legal holds for")
    ),
    responses(
        (status = 200, description = "Legal hold check completed", body = CheckLegalHoldResponse),
        (status = 404, description = "GDPR coordinator not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn check_legal_holds(
    State(state): State<Arc<ApiState>>,
    Path(user_id): Path<String>,
) -> Result<Json<CheckLegalHoldResponse>, (StatusCode, String)> {
    info!(user_id = %user_id, "GDPR legal hold check request received");

    // Get GDPR coordinator
    let gdpr_coordinator = state.gdpr_coordinator.as_ref().ok_or_else(|| {
        error!("GDPR coordinator not available");
        (
            StatusCode::NOT_FOUND,
            "GDPR coordinator not configured".to_string(),
        )
    })?;

    let retention_manager = gdpr_coordinator.retention_manager();
    let under_hold = retention_manager.is_subject_under_hold(&user_id);
    let active_holds = retention_manager.get_active_holds_for_subject(&user_id);

    let active_holds_info: Vec<LegalHoldInfo> = active_holds
        .into_iter()
        .map(|hold| LegalHoldInfo {
            id: hold.id.clone(),
            name: hold.name.clone(),
            reason: hold.reason.clone(),
            placed_at: hold.placed_at.to_rfc3339(),
            placed_by: hold.placed_by.clone(),
            expires_at: hold.expires_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    let response = CheckLegalHoldResponse {
        user_id: user_id.clone(),
        under_hold,
        active_holds: active_holds_info,
    };

    if response.under_hold {
        info!(
            user_id = %user_id,
            holds_count = %response.active_holds.len(),
            "User is under legal hold"
        );
    } else {
        info!(user_id = %user_id, "User is not under legal hold");
    }

    Ok(Json(response))
}
