//! GDPR Data Export API Handlers
//!
//! HTTP handlers for GDPR Article 20: Right to Data Portability

use crate::api::ApiState;
use crate::gdpr::export::{
    ExportErrorInfo, ExportProgressInfo, ExportRequest, ExportRequestResponse, ExportStatus,
    ExportStatusResponse,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs;
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};

/// Request a data export for a user (GDPR Article 20)
///
/// Creates an asynchronous export job that will discover, collect, and package
/// all personal data for the specified user in the requested format.
///
/// # Security
///
/// Ensure proper authentication - users should only be able to export their own data,
/// unless the requester is an admin with appropriate permissions.
#[utoipa::path(
    post,
    path = "/api/v1/gdpr/exports",
    request_body = ExportRequest,
    responses(
        (status = 202, description = "Export job created", body = ExportRequestResponse),
        (status = 404, description = "Export executor not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn request_export(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ExportRequest>,
) -> Result<Json<ExportRequestResponse>, (StatusCode, String)> {
    info!(
        user_id = %request.user_id,
        format = ?request.format,
        "GDPR export request received"
    );

    // Get export executor from state
    let executor = state.export_executor.as_ref().ok_or_else(|| {
        error!("Export executor not available");
        (
            StatusCode::NOT_FOUND,
            "Export executor not configured".to_string(),
        )
    })?;

    // Create export job
    use crate::gdpr::export::ExportJob;
    let job = ExportJob::new(
        request.user_id.clone(),
        "api-user".to_string(), // TODO: Get from auth context
        request,
    );
    let job_id = job.id;

    // Save job
    executor.job_store.save(&job).map_err(|e| {
        error!(error = %e, "Failed to save export job");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create export job: {}", e),
        )
    })?;

    // Start async execution
    let executor_clone = executor.clone();
    tokio::spawn(async move {
        if let Err(e) = executor_clone.execute_job(job_id).await {
            error!(job_id = %job_id, error = %e, "Export job failed");
        }
    });

    let response = ExportRequestResponse {
        job_id,
        status: ExportStatus::Pending,
        message: "Export job created successfully. Use the job_id to check status.".to_string(),
        estimated_completion: None, // TODO: Calculate based on data volume
    };

    info!(job_id = %job_id, "Export job created successfully");
    Ok(Json(response))
}

/// Get status of an export job
///
/// Returns the current status, progress, and download URL (when ready) for an export job.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/exports/{job_id}",
    params(
        ("job_id" = uuid::Uuid, Path, description = "Export job ID")
    ),
    responses(
        (status = 200, description = "Job status retrieved", body = ExportStatusResponse),
        (status = 404, description = "Job not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn get_export_status(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<Json<ExportStatusResponse>, (StatusCode, String)> {
    info!(job_id = %job_id, "Export status request received");

    let executor = state.export_executor.as_ref().ok_or_else(|| {
        error!("Export executor not available");
        (
            StatusCode::NOT_FOUND,
            "Export executor not configured".to_string(),
        )
    })?;

    // Get job
    let job = executor
        .get_job(job_id)
        .map_err(|e| {
            error!(error = %e, "Failed to get export job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve job: {}", e),
            )
        })?
        .ok_or_else(|| {
            warn!(job_id = %job_id, "Export job not found");
            (
                StatusCode::NOT_FOUND,
                format!("Export job not found: {}", job_id),
            )
        })?;

    // Build response
    let progress = Some(ExportProgressInfo {
        phase: job.progress.current_phase,
        percent_complete: job.progress.percent_complete,
        message: job.progress.status_message.clone(),
    });

    let download_url = job.result.as_ref().map(|r| r.download_url.clone());
    let expires_at = job.expires_at;
    let error = job.error.as_ref().map(|e| ExportErrorInfo {
        code: e.code,
        message: e.message.clone(),
    });

    let response = ExportStatusResponse {
        job_id,
        status: job.status,
        progress,
        download_url,
        expires_at,
        error,
    };

    Ok(Json(response))
}

/// Query parameters for listing export jobs
#[derive(Debug, Deserialize)]
pub struct ListExportsQuery {
    /// User ID to list exports for
    user_id: String,
    /// Maximum number of results (default: 10, max: 100)
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

/// List export jobs for a user
///
/// Returns all export jobs created by or for a specific user.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/exports",
    params(
        ("user_id" = String, Query, description = "User ID to list exports for"),
        ("limit" = Option<usize>, Query, description = "Maximum number of results")
    ),
    responses(
        (status = 200, description = "Export jobs retrieved", body = Vec<ExportStatusResponse>),
        (status = 404, description = "Export executor not available"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn list_user_exports(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListExportsQuery>,
) -> Result<Json<Vec<ExportStatusResponse>>, (StatusCode, String)> {
    info!(user_id = %query.user_id, limit = %query.limit, "List exports request received");

    let executor = state.export_executor.as_ref().ok_or_else(|| {
        error!("Export executor not available");
        (
            StatusCode::NOT_FOUND,
            "Export executor not configured".to_string(),
        )
    })?;

    // Limit to reasonable max
    let limit = query.limit.min(100);

    // Get jobs
    let jobs = executor
        .list_user_jobs(&query.user_id, Some(limit))
        .map_err(|e| {
            error!(error = %e, "Failed to list export jobs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list jobs: {}", e),
            )
        })?;

    // Convert to response format
    let responses: Vec<ExportStatusResponse> = jobs
        .into_iter()
        .map(|job| {
            let progress = Some(ExportProgressInfo {
                phase: job.progress.current_phase,
                percent_complete: job.progress.percent_complete,
                message: job.progress.status_message.clone(),
            });

            let download_url = job.result.as_ref().map(|r| r.download_url.clone());
            let expires_at = job.expires_at;
            let error = job.error.as_ref().map(|e| ExportErrorInfo {
                code: e.code,
                message: e.message.clone(),
            });

            ExportStatusResponse {
                job_id: job.id,
                status: job.status,
                progress,
                download_url,
                expires_at,
                error,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Download an export file
///
/// Streams the export file to the client. The URL includes the job ID which
/// is used to locate the file.
///
/// # Security
///
/// Production implementations should use signed URLs with expiry for enhanced security.
/// Currently uses path-based access which should be protected by authentication middleware.
#[utoipa::path(
    get,
    path = "/api/v1/gdpr/exports/{job_id}/download",
    params(
        ("job_id" = uuid::Uuid, Path, description = "Export job ID")
    ),
    responses(
        (status = 200, description = "Export file download", content_type = "application/octet-stream"),
        (status = 404, description = "Export not found or expired"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn download_export(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<Response, (StatusCode, String)> {
    info!(job_id = %job_id, "Export download request received");

    let executor = state.export_executor.as_ref().ok_or_else(|| {
        error!("Export executor not available");
        (
            StatusCode::NOT_FOUND,
            "Export executor not configured".to_string(),
        )
    })?;

    // Get job
    let job = executor
        .get_job(job_id)
        .map_err(|e| {
            error!(error = %e, "Failed to get export job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve job: {}", e),
            )
        })?
        .ok_or_else(|| {
            warn!(job_id = %job_id, "Export job not found");
            (
                StatusCode::NOT_FOUND,
                format!("Export job not found: {}", job_id),
            )
        })?;

    // Check if job is ready
    if job.status != ExportStatus::Ready {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Export not ready. Current status: {:?}", job.status),
        ));
    }

    // Check expiry
    if job.is_expired() {
        return Err((
            StatusCode::NOT_FOUND,
            "Export has expired. Please request a new export.".to_string(),
        ));
    }

    // Get file path from result
    let result = job.result.ok_or_else(|| {
        error!(job_id = %job_id, "Job marked as ready but has no result");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Export file not available".to_string(),
        )
    })?;

    let file_path = std::path::PathBuf::from(&result.file_path);

    // Open file
    let file = fs::File::open(&file_path).await.map_err(|e| {
        error!(error = %e, path = %file_path.display(), "Failed to open export file");
        (StatusCode::NOT_FOUND, "Export file not found".to_string())
    })?;

    // Stream file
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Determine content type from format
    let content_type = job.request.format.mime_type();

    // Generate filename
    let filename = format!("{}.{}", job_id, job.request.format.extension());

    // Build response with headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("Content-Length", result.file_size_bytes.to_string())
        .header("X-Checksum-SHA256", &result.checksum)
        .body(body)
        .map_err(|e| {
            error!(error = %e, "Failed to build response");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build response".to_string(),
            )
        })?;

    info!(job_id = %job_id, filename = %filename, "Export download started");
    Ok(response)
}

/// Cancel an export job
///
/// Cancels a pending or in-progress export job. Completed or failed jobs cannot be cancelled.
#[utoipa::path(
    delete,
    path = "/api/v1/gdpr/exports/{job_id}",
    params(
        ("job_id" = uuid::Uuid, Path, description = "Export job ID to cancel")
    ),
    responses(
        (status = 200, description = "Job cancelled successfully"),
        (status = 404, description = "Job not found"),
        (status = 409, description = "Job cannot be cancelled (already completed/failed)"),
        (status = 500, description = "Internal server error")
    ),
    tag = "GDPR"
)]
pub async fn cancel_export(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    info!(job_id = %job_id, "Export cancellation request received");

    let executor = state.export_executor.as_ref().ok_or_else(|| {
        error!("Export executor not available");
        (
            StatusCode::NOT_FOUND,
            "Export executor not configured".to_string(),
        )
    })?;

    // Cancel job
    executor.cancel_job(job_id).map_err(|e| {
        let error_msg = e.to_string();
        if error_msg.contains("Cannot cancel job in status") {
            (StatusCode::CONFLICT, error_msg)
        } else if error_msg.contains("not found") {
            (StatusCode::NOT_FOUND, error_msg)
        } else {
            error!(error = %e, "Failed to cancel export job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to cancel job: {}", e),
            )
        }
    })?;

    info!(job_id = %job_id, "Export job cancelled successfully");
    Ok(Json(serde_json::json!({
        "message": "Export job cancelled successfully",
        "job_id": job_id
    })))
}
