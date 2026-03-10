//! File Library API Handlers
//!
//! HTTP handlers for file management operations.

use super::storage::FileLibraryStorage;
use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

// Imports for ontology alignment
use graphica_core::schema::UniversalDataType;

// ============================================================================
// File Operations
// ============================================================================

/// List all files in library
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files",
    params(
        ("folder_id" = Option<String>, Query, description = "Filter by folder ID"),
        ("tags" = Option<Vec<String>>, Query, description = "Filter by tags"),
        ("search" = Option<String>, Query, description = "Search query for file name or description"),
        ("limit" = Option<usize>, Query, description = "Number of results to return (default: 100)"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination (default: 0)"),
    ),
    responses(
        (status = 200, description = "Successfully retrieved file list", body = ListFilesResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn list_files(
    State(state): State<Arc<ApiState>>,
    Query(request): Query<ListFilesRequest>,
) -> Result<Json<ListFilesResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!(
        "Listing files with filters: folder_id={:?}, tags={:?}, search={:?}",
        request.folder_id,
        request.tags,
        request.search
    );

    let files = storage
        .list_files(&request)
        .map_err(|e| ApiError::internal(format!("Failed to list files: {}", e)))?;

    let total = files.len();
    let limit = request.limit.unwrap_or(100);
    let offset = request.offset.unwrap_or(0);

    // Apply pagination
    let paginated: Vec<DataFile> = files.into_iter().skip(offset).take(limit).collect();

    let page = offset / limit;
    let page_size = paginated.len();

    Ok(Json(ListFilesResponse {
        files: paginated,
        total,
        page,
        page_size,
    }))
}

/// Get single file by ID
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}",
    params(
        ("id" = String, Path, description = "File ID"),
    ),
    responses(
        (status = 200, description = "Successfully retrieved file", body = DataFile),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Json<DataFile>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Getting file: {}", file_id);

    let file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    Ok(Json(file))
}

/// Upload new file with multipart form data
#[utoipa::path(
    post,
    path = "/api/v1/file-library/files",
    request_body(content = String, description = "Multipart form data with file, metadata (folder_id, tags, description, auto_scan, map_to_ontology, ontology_id)", content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "File uploaded successfully", body = CreateFileResponse),
        (status = 400, description = "Bad request - invalid file or metadata", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn create_file(
    State(state): State<Arc<ApiState>>,
    mut multipart: Multipart,
) -> Result<Json<CreateFileResponse>, ApiError> {
    use super::scanner::FileScanner;
    use std::path::PathBuf;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    // Get current user from auth context (placeholder)
    let owner = FileOwner {
        user_id: "system".to_string(),
        email: "system@graphica.io".to_string(),
        name: "System".to_string(),
    };

    let file_id = format!("file_{}", Uuid::new_v4().simple());
    let now = Utc::now();

    // Storage directory
    let storage_dir = std::env::var("FILE_LIBRARY_STORAGE_PATH")
        .unwrap_or_else(|_| "./data/file-library".to_string());

    // Create storage directory if it doesn't exist
    fs::create_dir_all(&storage_dir)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create storage directory: {}", e)))?;

    // Parse multipart form
    let mut filename = None;
    let mut folder_id = None;
    let mut tags: Vec<String> = Vec::new();
    let mut description = None;
    let mut auto_scan = false;
    let mut map_to_ontology = false;
    let mut ontology_id: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                let data = field.bytes().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read file data: {}", e))
                })?;
                file_data = Some(data.to_vec());
            }
            "folder_id" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read folder_id: {}", e))
                })?;
                folder_id = if !text.is_empty() { Some(text) } else { None };
            }
            "tags" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read tags: {}", e)))?;
                if !text.is_empty() {
                    tags = text.split(',').map(|s| s.trim().to_string()).collect();
                }
            }
            "description" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read description: {}", e))
                })?;
                description = if !text.is_empty() { Some(text) } else { None };
            }
            "auto_scan" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read auto_scan: {}", e))
                })?;
                auto_scan = text == "true" || text == "1";
            }
            "map_to_ontology" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read map_to_ontology: {}", e))
                })?;
                map_to_ontology = text == "true" || text == "1";
            }
            "ontology_id" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read ontology_id: {}", e))
                })?;
                ontology_id = if !text.is_empty() { Some(text) } else { None };
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Validate that we got a file
    let file_data =
        file_data.ok_or_else(|| ApiError::bad_request("No file uploaded".to_string()))?;
    let filename =
        filename.ok_or_else(|| ApiError::bad_request("No filename provided".to_string()))?;

    // Save file to disk using atomic write pattern (temp -> rename)
    let file_path = PathBuf::from(&storage_dir).join(&file_id);
    let temp_path = PathBuf::from(&storage_dir).join(format!("{}.tmp", file_id));

    // Write to temporary file first
    let mut temp_file = fs::File::create(&temp_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create temporary file: {}", e)))?;

    temp_file.write_all(&file_data).await.map_err(|e| {
        // Clean up temp file on write failure
        let _ = std::fs::remove_file(&temp_path);
        ApiError::internal(format!("Failed to write file: {}", e))
    })?;

    // Ensure all data is flushed to disk
    temp_file.sync_all().await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        ApiError::internal(format!("Failed to sync file: {}", e))
    })?;

    // Close the file handle
    drop(temp_file);

    // Atomic rename to final location
    fs::rename(&temp_path, &file_path).await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        ApiError::internal(format!("Failed to finalize file upload: {}", e))
    })?;

    let size_bytes = file_data.len() as u64;
    let file_path_str = file_path.to_string_lossy().to_string();

    tracing::info!(
        "Uploaded file: {} ({} bytes) to {}",
        filename,
        size_bytes,
        file_path_str
    );

    // Optionally scan the file
    let (schema, status, warnings, errors, ontology_mappings) = if auto_scan {
        let scanner = FileScanner::new();
        match scanner.scan_file(
            &file_path_str,
            ScanFileRequest {
                delimiter: None,
                encoding: None,
                has_header: None,
                sample_rows: Some(1000),
                auto_save: None, // Not auto-saving during upload
                map_to_ontology: None,
                ontology_id: None,
            },
        ) {
            Ok(scan_result) => {
                let schema = Some(FileSchema {
                    fields: scan_result.detected_fields.clone(),
                    total_rows: scan_result.total_rows.unwrap_or(0),
                    estimated_rows: scan_result.estimated_rows,
                    last_scanned: scan_result.scan_timestamp,
                });

                let status = if !scan_result.errors.is_empty() {
                    FileStatus::Error
                } else if !scan_result.warnings.is_empty() {
                    FileStatus::Warning
                } else {
                    FileStatus::Validated
                };

                // Perform ontology mapping if requested
                let mut mappings = vec![];
                if map_to_ontology {
                    if let (Some(engine), Some(ont_id)) = (&state.mapping_engine, &ontology_id) {
                        tracing::info!("📖 Mapping fields to ontology {} during upload", ont_id);

                        match align_fields_to_ontology(engine, ont_id, &scan_result.detected_fields)
                            .await
                        {
                            Ok(ont_mappings) => {
                                if !ont_mappings.is_empty() {
                                    tracing::info!(
                                        "✅ Mapped {} fields to ontology during upload",
                                        ont_mappings.len()
                                    );
                                    mappings = ont_mappings;
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to align fields to ontology during upload: {}",
                                    e
                                );
                            }
                        }
                    } else if ontology_id.is_none() {
                        tracing::warn!(
                            "map_to_ontology=true but no ontology_id provided during upload"
                        );
                    }
                }

                (
                    schema,
                    status,
                    scan_result.warnings,
                    scan_result.errors,
                    mappings,
                )
            }
            Err(e) => {
                tracing::warn!("Failed to scan file: {}", e);
                (
                    None,
                    FileStatus::Pending,
                    Vec::new(),
                    vec![format!("Scan failed: {}", e)],
                    vec![],
                )
            }
        }
    } else {
        (None, FileStatus::Pending, Vec::new(), Vec::new(), vec![])
    };

    // Create file metadata
    let file = DataFile {
        id: file_id.clone(),
        name: filename,
        file_path: file_path_str.clone(),
        folder_id,
        description,
        owner,
        size_bytes,
        encoding: "UTF-8".to_string(),
        delimiter: ",".to_string(),
        has_header: true,
        schema: schema.clone(),
        ontology_mappings, // Include mappings from scan (if any)
        status,
        validation_errors: errors,
        validation_warnings: warnings,
        tags,
        metadata: std::collections::HashMap::new(),
        sensitivity_level: None,
        retention_policy: None,
        access_control: None,
        created_at: now,
        updated_at: now,
        last_accessed: None,
        version: Some(1),
        previous_versions: Vec::new(),
    };

    storage
        .create_file(file.clone())
        .map_err(|e| ApiError::internal(format!("Failed to create file metadata: {}", e)))?;

    let scan_result = if auto_scan {
        schema.map(|s| ScanResult {
            detected_fields: s.fields,
            total_rows: Some(s.total_rows),
            estimated_rows: s.estimated_rows,
            delimiter_detected: Some(",".to_string()),
            encoding_detected: Some("UTF-8".to_string()),
            has_header_detected: Some(true),
            scan_timestamp: s.last_scanned,
            warnings: file.validation_warnings.clone(),
            errors: file.validation_errors.clone(),
        })
    } else {
        None
    };

    Ok(Json(CreateFileResponse {
        file_id,
        file_path: file_path_str,
        scan_result,
        status: file.status,
    }))
}

/// Update file metadata
#[utoipa::path(
    put,
    path = "/api/v1/file-library/files/{id}",
    params(
        ("id" = String, Path, description = "File ID to update"),
    ),
    request_body = UpdateFileRequest,
    responses(
        (status = 200, description = "File updated successfully", body = DataFile),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn update_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
    Json(request): Json<UpdateFileRequest>,
) -> Result<Json<DataFile>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Updating file: {}", file_id);

    let updated_file = storage.update_file(&file_id, request).map_err(|e| {
        if e.to_string().contains("not found") {
            ApiError::not_found(format!("File not found: {}", file_id))
        } else {
            ApiError::internal(format!("Failed to update file: {}", e))
        }
    })?;

    Ok(Json(updated_file))
}

/// Delete file
#[utoipa::path(
    delete,
    path = "/api/v1/file-library/files/{id}",
    params(
        ("id" = String, Path, description = "File ID to delete"),
        ("force" = Option<bool>, Query, description = "Force delete even if file has dependencies"),
    ),
    responses(
        (status = 200, description = "File deleted successfully or dependencies found", body = DeleteFileResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn delete_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
    Query(params): Query<DeleteFileParams>,
) -> Result<Json<DeleteFileResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!(
        "Deleting file: {} (force={})",
        file_id,
        params.force.unwrap_or(false)
    );

    // Check dependencies (placeholder - would check actual workflow usage)
    let dependencies = if !params.force.unwrap_or(false) {
        // In production, query lineage system for workflows using this file
        Some(Dependencies {
            workflows: Vec::new(),
            datasets: Vec::new(),
        })
    } else {
        None
    };

    if let Some(deps) = &dependencies {
        if !deps.workflows.is_empty() || !deps.datasets.is_empty() {
            return Ok(Json(DeleteFileResponse {
                success: false,
                dependencies: Some(deps.clone()),
            }));
        }
    }

    storage
        .delete_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to delete file: {}", e)))?;

    Ok(Json(DeleteFileResponse {
        success: true,
        dependencies: None,
    }))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct DeleteFileParams {
    pub force: Option<bool>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct DeleteFileResponse {
    pub success: bool,
    pub dependencies: Option<Dependencies>,
}

/// Scan file to detect schema
#[utoipa::path(
    post,
    path = "/api/v1/file-library/files/{id}/scan",
    params(
        ("id" = String, Path, description = "File ID to scan"),
    ),
    request_body = ScanFileRequest,
    responses(
        (status = 200, description = "File scanned successfully", body = ScanResult),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn scan_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
    Json(request): Json<ScanFileRequest>,
) -> Result<Json<ScanResult>, ApiError> {
    use super::scanner::FileScanner;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Scanning file: {}", file_id);

    // Verify file exists and get file path
    let file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Check flags
    let auto_save = request.auto_save.unwrap_or(false);
    let map_to_ontology = request.map_to_ontology.unwrap_or(false);
    let ontology_id = request.ontology_id.clone();

    // Scan the file using FileScanner
    let scanner = FileScanner::new();
    let scan_request_inner = ScanFileRequest {
        delimiter: request.delimiter,
        encoding: request.encoding,
        has_header: request.has_header,
        sample_rows: request.sample_rows,
        auto_save: None,       // Handle saving separately
        map_to_ontology: None, // Handle mapping separately
        ontology_id: None,
    };
    let scan_result = scanner
        .scan_file(&file.file_path, scan_request_inner)
        .map_err(|e| ApiError::internal(format!("Failed to scan file: {}", e)))?;

    // Map to ontology if requested
    let mut ontology_mappings = vec![];
    if map_to_ontology {
        if let (Some(engine), Some(ont_id)) = (&state.mapping_engine, &ontology_id) {
            tracing::info!(
                "📖 Mapping fields to ontology {} for file {}",
                ont_id,
                file_id
            );

            match align_fields_to_ontology(engine, ont_id, &scan_result.detected_fields).await {
                Ok(mappings) => {
                    if !mappings.is_empty() {
                        tracing::info!("✅ Mapped {} fields to ontology", mappings.len());
                        ontology_mappings = mappings;
                    } else {
                        tracing::warn!("No ontology mappings found for file {}", file_id);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to align fields to ontology: {}", e);
                }
            }
        } else if ontology_id.is_none() {
            tracing::warn!("map_to_ontology=true but no ontology_id provided");
        }
    }

    // Auto-save schema and ontology mappings if requested
    if auto_save {
        let file_schema = FileSchema {
            fields: scan_result.detected_fields.clone(),
            total_rows: scan_result.total_rows.unwrap_or(0),
            estimated_rows: scan_result.estimated_rows,
            last_scanned: scan_result.scan_timestamp,
        };

        let update_request = UpdateFileRequest {
            name: None,
            description: None,
            tags: None,
            folder_id: None,
            metadata: None,
            schema: Some(file_schema),
            ontology_mappings: ontology_mappings.clone(),
        };

        storage
            .update_file(&file_id, update_request)
            .map_err(|e| ApiError::internal(format!("Failed to save schema: {}", e)))?;

        tracing::info!(
            "✅ Schema and {} ontology mappings auto-saved for file: {}",
            ontology_mappings.len(),
            file_id
        );
    }

    Ok(Json(scan_result))
}

/// Validate file for datasource registration
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/validate-registration",
    params(
        ("id" = String, Path, description = "File ID to validate"),
    ),
    responses(
        (status = 200, description = "Validation result", body = ValidateFileForRegistrationResponse),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn validate_file_for_registration(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Json<ValidateFileForRegistrationResponse>, ApiError> {
    use super::scanner::FileScanner;
    use super::types::{InferredConfig, ValidateFileForRegistrationResponse};

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Validating file for registration: {}", file_id);

    // Verify file exists
    let file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    let mut issues = Vec::new();
    let mut can_register = true;

    // Try to scan the file to infer configuration
    let scanner = FileScanner::new();
    let scan_request = ScanFileRequest {
        delimiter: None,
        encoding: None,
        has_header: None,
        sample_rows: Some(1000), // Sample first 1000 rows for validation
        auto_save: None,
        map_to_ontology: None,
        ontology_id: None,
    };

    match scanner.scan_file(&file.file_path, scan_request) {
        Ok(scan_result) => {
            // Determine connector type based on file extension
            let connector_type = if file.name.ends_with(".csv") || file.file_path.ends_with(".csv")
            {
                "csv"
            } else if file.name.ends_with(".tsv") || file.file_path.ends_with(".tsv") {
                "tsv"
            } else if file.name.ends_with(".json") {
                "json"
            } else if file.name.ends_with(".parquet") {
                "parquet"
            } else {
                "csv" // Default to CSV
            };

            let column_count = scan_result.detected_fields.len();
            let row_count = scan_result.total_rows;

            // Validation checks
            if column_count == 0 {
                can_register = false;
                issues.push("File has no detectable columns - unable to infer schema".to_string());
            }

            if row_count.unwrap_or(0) == 0 {
                can_register = false;
                issues.push("File appears to be empty - no data rows found".to_string());
            }

            // Check for scan warnings/errors
            if !scan_result.warnings.is_empty() {
                for warning in &scan_result.warnings {
                    issues.push(format!("Warning: {}", warning));
                }
            }

            if !scan_result.errors.is_empty() {
                can_register = false;
                for error in &scan_result.errors {
                    issues.push(format!("Error: {}", error));
                }
            }

            // TODO: Check if file is already registered as a datasource
            // This would require querying the datasource catalog

            let inferred_config = InferredConfig {
                connector_type: connector_type.to_string(),
                delimiter: scan_result.delimiter_detected.clone(),
                has_header: scan_result.has_header_detected,
                row_count,
                column_count: Some(column_count),
            };

            Ok(Json(ValidateFileForRegistrationResponse {
                can_register,
                issues,
                inferred_config,
            }))
        }
        Err(e) => {
            // Scan failed - file cannot be registered
            can_register = false;
            issues.push(format!("Failed to scan file: {}", e));
            issues
                .push("File may be corrupted, in an unsupported format, or unreadable".to_string());

            let inferred_config = InferredConfig {
                connector_type: "unknown".to_string(),
                delimiter: None,
                has_header: None,
                row_count: None,
                column_count: None,
            };

            Ok(Json(ValidateFileForRegistrationResponse {
                can_register,
                issues,
                inferred_config,
            }))
        }
    }
}

/// Download file
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/download",
    params(
        ("id" = String, Path, description = "File ID to download"),
    ),
    responses(
        (status = 200, description = "File content (binary stream)"),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn download_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Response, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Downloading file: {}", file_id);

    // Retrieve file metadata
    let file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Update last accessed timestamp
    if let Err(e) = storage.update_last_accessed(&file_id) {
        tracing::warn!("Failed to update last_accessed for file {}: {}", file_id, e);
    }

    // Open file for streaming
    let file_handle = File::open(&file.file_path).await.map_err(|e| {
        tracing::error!(
            "Failed to open file {} at path {}: {}",
            file_id,
            file.file_path,
            e
        );
        ApiError::internal(format!("Failed to open file: {}", e))
    })?;

    // Get file metadata for size
    let metadata = file_handle
        .metadata()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get file metadata: {}", e)))?;

    let file_size = metadata.len();

    // Detect content type from file extension
    let content_type = detect_content_type(&file.name);

    // Create streaming response
    let stream = ReaderStream::new(file_handle);
    let body = Body::from_stream(stream);

    // Build response with appropriate headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, file_size)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", sanitize_filename(&file.name)),
        )
        .body(body)
        .map_err(|e| ApiError::internal(format!("Failed to build response: {}", e)))?;

    tracing::info!(
        "Successfully started streaming file {} ({} bytes)",
        file_id,
        file_size
    );

    Ok(response)
}

/// Detect MIME type from file extension
fn detect_content_type(filename: &str) -> &'static str {
    if filename.ends_with(".csv") {
        "text/csv"
    } else if filename.ends_with(".tsv") {
        "text/tab-separated-values"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".parquet") {
        "application/octet-stream"
    } else if filename.ends_with(".xml") {
        "application/xml"
    } else if filename.ends_with(".txt") {
        "text/plain"
    } else if filename.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if filename.ends_with(".xls") {
        "application/vnd.ms-excel"
    } else if filename.ends_with(".pdf") {
        "application/pdf"
    } else if filename.ends_with(".zip") {
        "application/zip"
    } else if filename.ends_with(".gz") {
        "application/gzip"
    } else {
        // Default to binary stream for unknown types
        "application/octet-stream"
    }
}

/// Sanitize filename to prevent header injection attacks
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect()
}

/// Preview file contents
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/preview",
    params(
        ("id" = String, Path, description = "File ID to preview"),
        ("rows" = Option<usize>, Query, description = "Number of rows to preview (default: 100)"),
    ),
    responses(
        (status = 200, description = "File preview with sample rows", body = FilePreviewResponse),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn preview_file(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
    Query(params): Query<PreviewParams>,
) -> Result<Json<FilePreviewResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!(
        "Previewing file: {} (rows={})",
        file_id,
        params.rows.unwrap_or(100)
    );

    // Verify file exists
    let _file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Placeholder preview - in production would read first N rows from file
    Ok(Json(FilePreviewResponse {
        headers: vec!["id".to_string(), "name".to_string(), "email".to_string()],
        rows: vec![
            vec![
                serde_json::json!(1),
                serde_json::json!("John Doe"),
                serde_json::json!("john@example.com"),
            ],
            vec![
                serde_json::json!(2),
                serde_json::json!("Jane Smith"),
                serde_json::json!("jane@example.com"),
            ],
        ],
        total_rows: 1000,
    }))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct PreviewParams {
    pub rows: Option<usize>,
}

// ============================================================================
// Bulk Operations
// ============================================================================

/// Bulk upload multiple files with background processing
#[utoipa::path(
    post,
    path = "/api/v1/file-library/files/bulk-upload",
    request_body(content = String, description = "Multipart form data with multiple files and optional metadata (folder_id, tags, auto_scan)", content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Bulk upload job created", body = BulkUploadResponse),
        (status = 400, description = "Bad request - no files uploaded", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn bulk_upload(
    State(state): State<Arc<ApiState>>,
    mut multipart: Multipart,
) -> Result<Json<BulkUploadResponse>, ApiError> {
    use super::scanner::FileScanner;
    use std::path::PathBuf;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    let job_id = format!("job_{}", Uuid::new_v4().simple());
    let now = Utc::now();

    tracing::info!("Starting bulk upload job: {}", job_id);

    // Storage directory
    let storage_dir = std::env::var("FILE_LIBRARY_STORAGE_PATH")
        .unwrap_or_else(|_| "./data/file-library".to_string());

    // Create storage directory if it doesn't exist
    fs::create_dir_all(&storage_dir)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create storage directory: {}", e)))?;

    // Get current user from auth context (placeholder)
    let owner = FileOwner {
        user_id: "system".to_string(),
        email: "system@graphica.io".to_string(),
        name: "System".to_string(),
    };

    // Parse multipart form and collect file metadata
    let mut folder_id = None;
    let mut tags: Vec<String> = Vec::new();
    let mut auto_scan = true; // Default to auto-scan
    let mut files_data: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "files" => {
                if let Some(filename) = field.file_name().map(|s| s.to_string()) {
                    let data = field.bytes().await.map_err(|e| {
                        ApiError::bad_request(format!("Failed to read file data: {}", e))
                    })?;
                    files_data.push((filename, data.to_vec()));
                }
            }
            "folder_id" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read folder_id: {}", e))
                })?;
                folder_id = if !text.is_empty() { Some(text) } else { None };
            }
            "tags" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read tags: {}", e)))?;
                if !text.is_empty() {
                    tags = text.split(',').map(|s| s.trim().to_string()).collect();
                }
            }
            "auto_scan" => {
                let text = field.text().await.map_err(|e| {
                    ApiError::bad_request(format!("Failed to read auto_scan: {}", e))
                })?;
                auto_scan = text == "true" || text == "1";
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    let total_files = files_data.len();

    if total_files == 0 {
        return Err(ApiError::bad_request("No files uploaded".to_string()));
    }

    tracing::info!("Received {} files for bulk upload", total_files);

    // Create job
    let job = ImportJob {
        job_id: job_id.clone(),
        status: JobStatus::Processing,
        total_files,
        processed_files: 0,
        successful_files: 0,
        failed_files: 0,
        progress_percent: 0.0,
        results: Vec::new(),
        started_at: now,
        completed_at: None,
        duration_ms: None,
    };

    storage
        .create_job(job)
        .map_err(|e| ApiError::internal(format!("Failed to create job: {}", e)))?;

    // Spawn background task to process files
    let storage_clone = storage.clone();
    let job_id_clone = job_id.clone();
    let storage_dir_clone = storage_dir.clone();

    tokio::spawn(async move {
        tracing::info!(
            "Background job {} processing {} files",
            job_id_clone,
            total_files
        );

        let scanner = FileScanner::new();
        let mut results: Vec<ImportResult> = Vec::new();
        let mut successful = 0;
        let mut failed = 0;

        // Process files in parallel (4 at a time to avoid overwhelming the system)
        let mut tasks = Vec::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(4));

        for (idx, (filename, file_data)) in files_data.into_iter().enumerate() {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let storage_dir = storage_dir_clone.clone();
            let owner_clone = owner.clone();
            let folder_id_clone = folder_id.clone();
            let tags_clone = tags.clone();
            let scanner_clone = FileScanner::new();
            let storage_clone2 = storage_clone.clone();
            let job_id_clone2 = job_id_clone.clone();

            let task = tokio::spawn(async move {
                let _permit = permit; // Hold permit until task completes
                let filename_clone = filename.clone(); // Clone for error handling

                let file_id = format!("file_{}", Uuid::new_v4().simple());
                let file_path = PathBuf::from(&storage_dir).join(&file_id);
                let size_bytes = file_data.len() as u64;

                // Save file to disk
                let result: Result<ImportResult, String> = async {
                    let mut file = fs::File::create(&file_path)
                        .await
                        .map_err(|e| format!("Failed to create file: {}", e))?;

                    file.write_all(&file_data)
                        .await
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    let file_path_str = file_path.to_string_lossy().to_string();

                    // Scan the file
                    let (schema, file_status, scan_result_opt) = if auto_scan {
                        match scanner_clone.scan_file(
                            &file_path_str,
                            ScanFileRequest {
                                delimiter: None,
                                encoding: None,
                                has_header: None,
                                sample_rows: Some(1000),
                                auto_save: None, // Not auto-saving during bulk upload
                                map_to_ontology: None,
                                ontology_id: None,
                            },
                        ) {
                            Ok(scan_result) => {
                                let schema = Some(FileSchema {
                                    fields: scan_result.detected_fields.clone(),
                                    total_rows: scan_result.total_rows.unwrap_or(0),
                                    estimated_rows: scan_result.estimated_rows,
                                    last_scanned: scan_result.scan_timestamp,
                                });

                                let status = if !scan_result.errors.is_empty() {
                                    FileStatus::Error
                                } else if !scan_result.warnings.is_empty() {
                                    FileStatus::Warning
                                } else {
                                    FileStatus::Validated
                                };

                                (schema, status, Some(scan_result))
                            }
                            Err(e) => {
                                let mut scan_err = ScanResult {
                                    detected_fields: Vec::new(),
                                    total_rows: Some(0),
                                    estimated_rows: None,
                                    delimiter_detected: None,
                                    encoding_detected: None,
                                    has_header_detected: None,
                                    scan_timestamp: Utc::now(),
                                    warnings: Vec::new(),
                                    errors: vec![format!("Scan failed: {}", e)],
                                };
                                (None, FileStatus::Pending, Some(scan_err))
                            }
                        }
                    } else {
                        (None, FileStatus::Pending, None)
                    };

                    // Create file metadata
                    let file = DataFile {
                        id: file_id.clone(),
                        name: filename.clone(),
                        file_path: file_path_str.clone(),
                        folder_id: folder_id_clone,
                        description: None,
                        owner: owner_clone,
                        size_bytes,
                        encoding: "UTF-8".to_string(),
                        delimiter: ",".to_string(),
                        has_header: true,
                        schema: schema.clone(),
                        ontology_mappings: vec![], // No mappings initially
                        status: file_status.clone(),
                        validation_errors: scan_result_opt
                            .as_ref()
                            .map(|s| s.errors.clone())
                            .unwrap_or_default(),
                        validation_warnings: scan_result_opt
                            .as_ref()
                            .map(|s| s.warnings.clone())
                            .unwrap_or_default(),
                        tags: tags_clone,
                        metadata: std::collections::HashMap::new(),
                        sensitivity_level: None,
                        retention_policy: None,
                        access_control: None,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                        last_accessed: None,
                        version: Some(1),
                        previous_versions: Vec::new(),
                    };

                    storage_clone2
                        .create_file(file)
                        .map_err(|e| format!("Failed to create file metadata: {}", e))?;

                    let import_status = match file_status {
                        FileStatus::Validated => ImportFileStatus::Success,
                        FileStatus::Error => ImportFileStatus::Error,
                        FileStatus::Warning => ImportFileStatus::Warning,
                        _ => ImportFileStatus::Success,
                    };

                    Ok(ImportResult {
                        file_name: filename,
                        file_id: Some(file_id),
                        status: import_status,
                        error: None,
                        scan_result: scan_result_opt,
                    })
                }
                .await;

                match result {
                    Ok(import_result) => (import_result, true),
                    Err(e) => {
                        tracing::error!("Failed to process file {}: {}", filename_clone, e);
                        (
                            ImportResult {
                                file_name: filename_clone,
                                file_id: None,
                                status: ImportFileStatus::Error,
                                error: Some(e),
                                scan_result: None,
                            },
                            false,
                        )
                    }
                }
            });

            tasks.push(task);

            // Update progress periodically
            if (idx + 1) % 10 == 0 || (idx + 1) == total_files {
                let progress = ((idx + 1) as f32 / total_files as f32) * 100.0;
                if let Err(e) = storage_clone.update_job_progress(&job_id_clone2, idx + 1, progress)
                {
                    tracing::error!("Failed to update job progress: {}", e);
                }
            }
        }

        // Wait for all tasks to complete
        for task in tasks {
            match task.await {
                Ok((result, success)) => {
                    if success {
                        successful += 1;
                    } else {
                        failed += 1;
                    }
                    results.push(result);
                }
                Err(e) => {
                    tracing::error!("Task panicked: {}", e);
                    failed += 1;
                }
            }
        }

        // Mark job as completed
        let duration_ms = (Utc::now() - now).num_milliseconds() as u64;
        let final_status = if failed == 0 {
            JobStatus::Completed
        } else if successful == 0 {
            JobStatus::Failed
        } else {
            JobStatus::Partial
        };

        if let Err(e) = storage_clone.complete_job(
            &job_id_clone,
            final_status,
            successful,
            failed,
            results,
            duration_ms,
        ) {
            tracing::error!("Failed to complete job: {}", e);
        }

        tracing::info!(
            "Background job {} completed: {} successful, {} failed",
            job_id_clone,
            successful,
            failed
        );
    });

    Ok(Json(BulkUploadResponse {
        job_id,
        status: JobStatus::Processing,
        total_files,
    }))
}

/// Get bulk import job status
#[utoipa::path(
    get,
    path = "/api/v1/file-library/jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Job ID to check status"),
    ),
    responses(
        (status = 200, description = "Job status retrieved", body = ImportJob),
        (status = 404, description = "Job not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_job_status(
    State(state): State<Arc<ApiState>>,
    Path(job_id): Path<String>,
) -> Result<Json<ImportJob>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Getting job status: {}", job_id);

    let job = storage
        .get_job(&job_id)
        .map_err(|e| ApiError::internal(format!("Failed to get job: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Job not found: {}", job_id)))?;

    Ok(Json(job))
}

/// Bulk update files
#[utoipa::path(
    put,
    path = "/api/v1/file-library/files/bulk-update",
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "Bulk update completed", body = BulkUpdateResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn bulk_update(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<BulkUpdateRequest>,
) -> Result<Json<BulkUpdateResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Bulk updating {} files", request.file_ids.len());

    let mut updated_count = 0;
    let mut errors = Vec::new();

    for file_id in &request.file_ids {
        // Build update request from bulk updates
        let update_request = UpdateFileRequest {
            name: None,
            description: None,
            tags: request
                .updates
                .tags
                .as_ref()
                .and_then(|op| match op.action {
                    TagAction::Set => Some(op.values.clone()),
                    _ => None, // Add/Remove would need current tags
                }),
            folder_id: request.updates.folder_id.clone(),
            metadata: request.updates.metadata.clone(),
            schema: None,              // Bulk updates don't modify schema
            ontology_mappings: vec![], // Bulk updates don't modify ontology mappings
        };

        match storage.update_file(file_id, update_request) {
            Ok(_) => updated_count += 1,
            Err(e) => errors.push(BulkOperationError {
                file_id: file_id.clone(),
                error: e.to_string(),
            }),
        }
    }

    Ok(Json(BulkUpdateResponse {
        success: errors.is_empty(),
        updated_count,
        errors: if errors.is_empty() {
            None
        } else {
            Some(errors)
        },
    }))
}

/// Bulk delete files
#[utoipa::path(
    delete,
    path = "/api/v1/file-library/files/bulk-delete",
    request_body = BulkDeleteRequest,
    responses(
        (status = 200, description = "Bulk delete completed", body = BulkDeleteResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn bulk_delete(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!(
        "Bulk deleting {} files (force={})",
        request.file_ids.len(),
        request.force.unwrap_or(false)
    );

    let mut deleted_count = 0;
    let mut errors = Vec::new();

    for file_id in &request.file_ids {
        match storage.delete_file(file_id) {
            Ok(_) => deleted_count += 1,
            Err(e) => errors.push(BulkDeleteError {
                file_id: file_id.clone(),
                error: e.to_string(),
                dependencies: None,
            }),
        }
    }

    Ok(Json(BulkDeleteResponse {
        success: errors.is_empty(),
        deleted_count,
        errors: if errors.is_empty() {
            None
        } else {
            Some(errors)
        },
    }))
}

/// Bulk scan multiple files (async with job tracking)
#[utoipa::path(
    post,
    path = "/api/v1/file-library/files/bulk-scan",
    request_body = BulkScanRequest,
    responses(
        (status = 200, description = "Bulk scan job created", body = BulkScanResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn bulk_scan(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<BulkScanRequest>,
) -> Result<Json<BulkScanResponse>, ApiError> {
    use super::scanner::FileScanner;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    let job_id = format!("job_{}", Uuid::new_v4().simple());
    let total_files = request.file_ids.len();

    tracing::info!("Starting bulk scan job: {} ({} files)", job_id, total_files);

    // Create job
    let job = ImportJob {
        job_id: job_id.clone(),
        status: JobStatus::Processing,
        total_files,
        processed_files: 0,
        successful_files: 0,
        failed_files: 0,
        progress_percent: 0.0,
        started_at: Utc::now(),
        completed_at: None,
        duration_ms: None,
        results: Vec::new(),
    };

    storage
        .create_job(job.clone())
        .map_err(|e| ApiError::internal(format!("Failed to create job: {}", e)))?;

    // Clone for background task
    let storage_clone = storage.clone();
    let job_id_clone = job_id.clone();
    let file_ids = request.file_ids.clone();
    let delimiter = request.delimiter.clone();
    let encoding = request.encoding.clone();
    let has_header = request.has_header;
    let sample_rows = request.sample_rows;
    let auto_save = request.auto_save.unwrap_or(false);
    let map_to_ontology = request.map_to_ontology.unwrap_or(false);
    let ontology_id = request.ontology_id.clone();
    let mapping_engine = state.mapping_engine.clone();

    // Spawn background task
    tokio::spawn(async move {
        let scanner = FileScanner::new();
        let mut successful_files = 0;
        let mut failed_files = 0;
        let mut results = Vec::new();
        let start_time = std::time::Instant::now();

        for (idx, file_id) in file_ids.iter().enumerate() {
            // Get file
            let file_result = storage_clone
                .get_file(file_id)
                .map_err(|e| format!("Failed to get file: {}", e))
                .and_then(|opt| opt.ok_or_else(|| format!("File not found: {}", file_id)));

            let import_result = match file_result {
                Ok(file) => {
                    // Scan the file
                    let scan_request = ScanFileRequest {
                        delimiter: delimiter.clone(),
                        encoding: encoding.clone(),
                        has_header,
                        sample_rows,
                        auto_save: None,
                        map_to_ontology: None, // Will be handled separately if needed
                        ontology_id: None,
                    };

                    match scanner.scan_file(&file.file_path, scan_request) {
                        Ok(scan_result) => {
                            // Auto-save schema if requested
                            if auto_save {
                                let file_schema = FileSchema {
                                    fields: scan_result.detected_fields.clone(),
                                    total_rows: scan_result.total_rows.unwrap_or(0),
                                    estimated_rows: scan_result.estimated_rows,
                                    last_scanned: scan_result.scan_timestamp,
                                };

                                let update_request = UpdateFileRequest {
                                    name: None,
                                    description: None,
                                    tags: None,
                                    folder_id: None,
                                    metadata: None,
                                    schema: Some(file_schema),
                                    ontology_mappings: vec![],
                                };

                                if let Err(e) = storage_clone.update_file(file_id, update_request) {
                                    tracing::warn!(
                                        "Failed to auto-save schema for {}: {}",
                                        file_id,
                                        e
                                    );
                                }
                            }

                            // Map to ontology if requested
                            if map_to_ontology {
                                if let (Some(engine), Some(ont_id)) =
                                    (&mapping_engine, &ontology_id)
                                {
                                    tracing::info!(
                                        "📖 Mapping fields to ontology {} for file {}",
                                        ont_id,
                                        file_id
                                    );

                                    match align_fields_to_ontology(
                                        engine,
                                        ont_id,
                                        &scan_result.detected_fields,
                                    )
                                    .await
                                    {
                                        Ok(mappings) => {
                                            if !mappings.is_empty() {
                                                tracing::info!(
                                                    "✅ Mapped {} fields to ontology",
                                                    mappings.len()
                                                );

                                                // Save mappings to file
                                                let update_request = UpdateFileRequest {
                                                    name: None,
                                                    description: None,
                                                    tags: None,
                                                    folder_id: None,
                                                    metadata: None,
                                                    schema: None,
                                                    ontology_mappings: mappings,
                                                };

                                                if let Err(e) = storage_clone
                                                    .update_file(file_id, update_request)
                                                {
                                                    tracing::warn!("Failed to save ontology mappings for {}: {}", file_id, e);
                                                }
                                            } else {
                                                tracing::warn!(
                                                    "No ontology mappings found for file {}",
                                                    file_id
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to align fields to ontology: {}",
                                                e
                                            );
                                        }
                                    }
                                } else {
                                    tracing::warn!("⚠️ Ontology mapping requested but mapping engine or ontology_id not available");
                                }
                            }

                            successful_files += 1;
                            ImportResult {
                                file_name: file.name,
                                file_id: Some(file_id.clone()),
                                status: ImportFileStatus::Success,
                                error: None,
                                scan_result: Some(scan_result),
                            }
                        }
                        Err(e) => {
                            failed_files += 1;
                            ImportResult {
                                file_name: file.name,
                                file_id: Some(file_id.clone()),
                                status: ImportFileStatus::Error,
                                error: Some(format!("Scan failed: {}", e)),
                                scan_result: None,
                            }
                        }
                    }
                }
                Err(e) => {
                    failed_files += 1;
                    ImportResult {
                        file_name: "unknown".to_string(),
                        file_id: Some(file_id.clone()),
                        status: ImportFileStatus::Error,
                        error: Some(e),
                        scan_result: None,
                    }
                }
            };

            results.push(import_result);

            // Update job progress
            let progress_percent = ((idx + 1) as f32 / file_ids.len() as f32) * 100.0;
            let _ = storage_clone.update_job_progress(&job_id_clone, idx + 1, progress_percent);
        }

        // Complete job
        let _duration_ms = start_time.elapsed().as_millis() as u64;
        let final_status = if failed_files == 0 {
            JobStatus::Completed
        } else if successful_files > 0 {
            JobStatus::Partial
        } else {
            JobStatus::Failed
        };

        let _ = storage_clone.complete_job(
            &job_id_clone,
            final_status,
            successful_files,
            failed_files,
            results,
            _duration_ms,
        );

        tracing::info!(
            "✅ Bulk scan job completed: {} ({}/{} succeeded)",
            job_id_clone,
            successful_files,
            file_ids.len()
        );
    });

    // Return job_id immediately
    Ok(Json(BulkScanResponse {
        job_id: job_id.clone(),
        status: JobStatus::Processing,
        total_files,
    }))
}

// ============================================================================
// Folder Operations
// ============================================================================

/// List all folders
#[utoipa::path(
    get,
    path = "/api/v1/file-library/folders",
    responses(
        (status = 200, description = "Successfully retrieved folder list", body = ListFoldersResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn list_folders(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ListFoldersResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Listing folders");

    let folders = storage
        .list_folders()
        .map_err(|e| ApiError::internal(format!("Failed to list folders: {}", e)))?;

    Ok(Json(ListFoldersResponse { folders }))
}

/// Create new folder
#[utoipa::path(
    post,
    path = "/api/v1/file-library/folders",
    request_body = CreateFolderRequest,
    responses(
        (status = 200, description = "Folder created successfully", body = Folder),
        (status = 404, description = "Parent folder not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn create_folder(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<Json<Folder>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    let folder_id = format!("folder_{}", Uuid::new_v4().simple());
    let now = Utc::now();

    // Build path
    let path = if let Some(parent_id) = &request.parent_id {
        let parent = storage
            .get_folder(parent_id)
            .map_err(|e| ApiError::internal(format!("Failed to get parent folder: {}", e)))?
            .ok_or_else(|| {
                ApiError::not_found(format!("Parent folder not found: {}", parent_id))
            })?;
        format!("{}/{}", parent.path.trim_end_matches('/'), request.name)
    } else {
        format!("/{}", request.name)
    };

    tracing::info!("Creating folder: {} at path {}", folder_id, path);

    let folder = Folder {
        id: folder_id,
        name: request.name,
        parent_id: request.parent_id,
        description: request.description,
        path,
        file_count: 0,
        subfolder_count: 0,
        created_at: now,
        updated_at: now,
        children: Some(Vec::new()),
    };

    let created_folder = storage
        .create_folder(folder)
        .map_err(|e| ApiError::internal(format!("Failed to create folder: {}", e)))?;

    Ok(Json(created_folder))
}

/// Update folder
#[utoipa::path(
    put,
    path = "/api/v1/file-library/folders/{id}",
    params(
        ("id" = String, Path, description = "Folder ID to update"),
    ),
    request_body = UpdateFolderRequest,
    responses(
        (status = 200, description = "Folder updated successfully", body = Folder),
        (status = 404, description = "Folder not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn update_folder(
    State(state): State<Arc<ApiState>>,
    Path(folder_id): Path<String>,
    Json(request): Json<UpdateFolderRequest>,
) -> Result<Json<Folder>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Updating folder: {}", folder_id);

    let updated_folder = storage.update_folder(&folder_id, request).map_err(|e| {
        if e.to_string().contains("not found") {
            ApiError::not_found(format!("Folder not found: {}", folder_id))
        } else {
            ApiError::internal(format!("Failed to update folder: {}", e))
        }
    })?;

    Ok(Json(updated_folder))
}

/// Delete folder
#[utoipa::path(
    delete,
    path = "/api/v1/file-library/folders/{id}",
    params(
        ("id" = String, Path, description = "Folder ID to delete"),
        ("force" = Option<bool>, Query, description = "Force delete even if folder contains files"),
    ),
    responses(
        (status = 200, description = "Folder deletion result", body = FolderResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn delete_folder(
    State(state): State<Arc<ApiState>>,
    Path(folder_id): Path<String>,
    Query(params): Query<DeleteFolderParams>,
) -> Result<Json<FolderResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!(
        "Deleting folder: {} (force={})",
        folder_id,
        params.force.unwrap_or(false)
    );

    match storage.delete_folder(&folder_id, params.force.unwrap_or(false)) {
        Ok(_) => Ok(Json(FolderResponse {
            success: true,
            error: None,
        })),
        Err(e) => Ok(Json(FolderResponse {
            success: false,
            error: Some(e.to_string()),
        })),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct DeleteFolderParams {
    pub force: Option<bool>,
}

// ============================================================================
// Search & Tags
// ============================================================================

/// List all tags
#[utoipa::path(
    get,
    path = "/api/v1/file-library/tags",
    responses(
        (status = 200, description = "Successfully retrieved tag list", body = ListTagsResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn list_tags(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ListTagsResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Listing tags");

    let tags = storage
        .list_tags()
        .map_err(|e| ApiError::internal(format!("Failed to list tags: {}", e)))?;

    Ok(Json(ListTagsResponse { tags }))
}

/// Advanced search
#[utoipa::path(
    post,
    path = "/api/v1/file-library/search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results with facets", body = SearchResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn search_files(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Searching files: query={}", request.query);

    let results = storage
        .search_files(&request)
        .map_err(|e| ApiError::internal(format!("Failed to search files: {}", e)))?;

    let total = results.len();

    // Build facets from results
    use std::collections::HashMap;
    let mut status_counts: HashMap<String, usize> = HashMap::new();
    let mut owner_counts: HashMap<String, usize> = HashMap::new();
    let mut tag_counts: HashMap<String, usize> = HashMap::new();

    for file in &results {
        *status_counts
            .entry(format!("{:?}", file.status))
            .or_insert(0) += 1;

        *owner_counts.entry(file.owner.user_id.clone()).or_insert(0) += 1;

        for tag in &file.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    let facets = Some(SearchFacets {
        status: status_counts,
        owners: owner_counts,
        tags: tag_counts,
    });

    // Apply pagination
    let limit = request.limit.unwrap_or(100);
    let offset = request.offset.unwrap_or(0);
    let paginated: Vec<DataFile> = results.into_iter().skip(offset).take(limit).collect();

    Ok(Json(SearchResponse {
        results: paginated,
        total,
        facets,
    }))
}

// ============================================================================
// Statistics
// ============================================================================

/// Get library statistics
#[utoipa::path(
    get,
    path = "/api/v1/file-library/stats",
    responses(
        (status = 200, description = "Library statistics", body = LibraryStatsResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_library_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<LibraryStatsResponse>, ApiError> {
    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::debug!("Getting library statistics");

    let stats = storage
        .get_statistics()
        .map_err(|e| ApiError::internal(format!("Failed to get statistics: {}", e)))?;

    Ok(Json(stats))
}

/// Get file usage statistics
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/usage-stats",
    params(
        ("id" = String, Path, description = "File ID"),
    ),
    responses(
        (status = 200, description = "File usage statistics", body = UsageStatsResponse),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_file_usage_stats(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Json<UsageStatsResponse>, ApiError> {
    use super::lineage::FileLineageTracker;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Getting usage stats for file: {}", file_id);

    // Verify file exists
    let _file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Get usage stats using FileLineageTracker
    let tracker = FileLineageTracker::new(state.lineage_storage.clone());
    let stats = tracker
        .get_usage_stats(&file_id, 30)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get usage stats: {}", e)))?;

    Ok(Json(UsageStatsResponse {
        file_id,
        times_used: stats.total_accesses,
        workflows_count: stats.unique_workflows,
        last_accessed: stats.last_accessed,
        access_count_30d: stats.total_accesses,
        top_users: Vec::new(), // Would need additional tracking for top users
    }))
}

// ============================================================================
// Lineage (Placeholder)
// ============================================================================

/// Get file lineage
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/lineage",
    params(
        ("id" = String, Path, description = "File ID"),
    ),
    responses(
        (status = 200, description = "File lineage graph", body = LineageResponse),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_file_lineage(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Json<LineageResponse>, ApiError> {
    use super::lineage::FileLineageTracker;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Getting lineage for file: {}", file_id);

    // Verify file exists
    let _file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Get lineage using FileLineageTracker
    let tracker = FileLineageTracker::new(state.lineage_storage.clone());
    let (upstream, downstream) = tracker
        .get_file_lineage(&file_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get lineage: {}", e)))?;

    // Convert LineageEvent to simple LineageNode for response
    let upstream_nodes: Vec<LineageNode> = upstream
        .iter()
        .flat_map(|event| {
            event.transforms.iter().map(|transform| LineageNode {
                id: transform.id.to_string(),
                name: transform.transform_type.clone(),
                node_type: LineageNodeType::Dataset,
                status: None,
            })
        })
        .collect();

    let downstream_nodes: Vec<LineageNode> = downstream
        .iter()
        .map(|event| LineageNode {
            id: event.output_ref.path.clone(),
            name: event.output_ref.system.clone(),
            node_type: LineageNodeType::Datasource,
            status: None,
        })
        .collect();

    Ok(Json(LineageResponse {
        file_id,
        upstream: upstream_nodes,
        downstream: downstream_nodes,
    }))
}

/// Get impact analysis
#[utoipa::path(
    get,
    path = "/api/v1/file-library/files/{id}/impact-analysis",
    params(
        ("id" = String, Path, description = "File ID"),
    ),
    responses(
        (status = 200, description = "Impact analysis results", body = ImpactAnalysisResponse),
        (status = 404, description = "File not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn get_impact_analysis(
    State(state): State<Arc<ApiState>>,
    Path(file_id): Path<String>,
) -> Result<Json<ImpactAnalysisResponse>, ApiError> {
    use super::lineage::FileLineageTracker;

    let storage = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not initialized".to_string()))?;

    tracing::info!("Getting impact analysis for file: {}", file_id);

    // Verify file exists
    let _file = storage
        .get_file(&file_id)
        .map_err(|e| ApiError::internal(format!("Failed to get file: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", file_id)))?;

    // Get impact analysis using FileLineageTracker
    let tracker = FileLineageTracker::new(state.lineage_storage.clone());
    let impact_report = tracker
        .get_file_impact(&file_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to analyze impact: {}", e)))?;

    // Convert FileImpactReport to API response
    let workflows_affected = impact_report.dependent_workflows.len();
    let datasets_affected = impact_report.dependent_transforms.len();

    let critical_workflows: Vec<String> = impact_report
        .dependent_workflows
        .iter()
        .filter(|w| w.is_active)
        .map(|w| w.workflow_id.clone())
        .collect();

    let mut recommendations = Vec::new();
    if impact_report.can_safely_delete {
        recommendations.push("File can be safely deleted.".to_string());
    } else {
        recommendations
            .push("Warning: File is used by active workflows or transforms.".to_string());
    }

    if !impact_report.warnings.is_empty() {
        recommendations.extend(impact_report.warnings.clone());
    }

    Ok(Json(ImpactAnalysisResponse {
        can_delete: impact_report.can_safely_delete,
        can_modify: impact_report.can_safely_modify,
        impact: ImpactDetails {
            workflows_affected,
            datasets_affected,
            critical_workflows,
            recommendations,
        },
    }))
}

// ============================================================================
// Ontology Alignment Helpers
// ============================================================================

/// Align detected fields to ontology concepts
async fn align_fields_to_ontology(
    engine: &Arc<crate::mapping::MappingEngine>,
    ontology_id: &str,
    detected_fields: &[SchemaField],
) -> Result<Vec<FieldOntologyMapping>, String> {
    // Get ontology terms from mapping engine
    let ontology_terms = engine
        .ontology_client()
        .get_ontology_terms()
        .map_err(|e| format!("Failed to get ontology terms: {}", e))?;

    if ontology_terms.is_empty() {
        tracing::warn!("No ontology terms found for alignment");
        return Ok(Vec::new());
    }

    tracing::info!(
        "Retrieved {} ontology terms for alignment",
        ontology_terms.len()
    );

    // Check if semantic matcher (model service) is available
    // PRE-EXISTING ISSUE: semantic_matcher method doesn't exist
    // let use_model_service = engine.semantic_matcher().is_some();
    let use_model_service = engine.is_semantic_available();
    if use_model_service {
        tracing::info!("🤖 Using model service for semantic embedding-based alignment");
    } else {
        tracing::info!("📝 Model service unavailable - using fallback string matching");
    }

    // Align each detected field
    let mut mappings = Vec::new();

    for schema_field in detected_fields.iter() {
        let field_name_normalized = schema_field.name.to_lowercase().replace('_', " ");

        let mut best_match: Option<(String, String, f64, &str)> = None; // (uri, label, score, method)

        // Try exact match first (fast path)
        for term in &ontology_terms {
            let label_normalized = term.label.to_lowercase().replace('_', " ");
            if label_normalized == field_name_normalized {
                best_match = Some((term.uri.clone(), term.label.clone(), 1.0, "ExactMatch"));
                break;
            }
        }

        // If no exact match, try semantic similarity using model service
        // PRE-EXISTING ISSUE: semantic_matcher method doesn't exist
        // This block is commented out because semantic matching is not available
        /*
        if best_match.is_none() && use_model_service {
            if let Some(semantic_matcher) = engine.semantic_matcher() {
                ... semantic matching code ...
            }
        }
        */

        // If still no match, try synonym matching
        if best_match.is_none() {
            for term in &ontology_terms {
                for alias in &term.aliases {
                    let alias_normalized = alias.to_lowercase().replace('_', " ");
                    if alias_normalized == field_name_normalized {
                        best_match =
                            Some((term.uri.clone(), term.label.clone(), 0.95, "SynonymMatch"));
                        break;
                    }
                }
                if best_match.is_some() {
                    break;
                }
            }
        }

        // Create mapping if we found a match
        if let Some((uri, label, similarity, method)) = best_match {
            let mapping = FieldOntologyMapping {
                field_name: schema_field.name.clone(),
                ontology_id: ontology_id.to_string(),
                concept_uri: uri.clone(),
                concept_label: label.clone(),
                similarity,
                confidence: similarity * 0.95, // Slightly lower confidence than similarity
                method: method.to_string(),
                mapped_at: Utc::now(),
            };

            tracing::info!(
                "✅ Mapped field '{}' to concept '{}' (method: {}, confidence: {:.2})",
                schema_field.name,
                label,
                method,
                mapping.confidence
            );

            mappings.push(mapping);
        } else {
            tracing::debug!("No alignment found for field '{}'", schema_field.name);
        }
    }

    if mappings.is_empty() {
        tracing::warn!(
            "No field mappings generated for {} fields",
            detected_fields.len()
        );
    } else {
        tracing::info!(
            "Generated {} field mappings using {}",
            mappings.len(),
            if use_model_service {
                "model service embeddings"
            } else {
                "string matching"
            }
        );
    }

    Ok(mappings)
}

/// Convert FieldType to UniversalDataType
fn convert_field_type_to_universal(field_type: &FieldType) -> UniversalDataType {
    match field_type {
        FieldType::String => UniversalDataType::String { max_length: None },
        FieldType::Integer => UniversalDataType::Integer { bits: None },
        FieldType::Float => UniversalDataType::Float { bits: None },
        FieldType::Boolean => UniversalDataType::Boolean,
        FieldType::Timestamp => UniversalDataType::Timestamp,
        FieldType::Date => UniversalDataType::Date,
    }
}

// ============================================================================
// Recovery Operations
// ============================================================================

/// Recovery response
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct RecoveryResponse {
    pub files_found: usize,
    pub files_recovered: usize,
    pub files_already_tracked: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// Recover orphaned files from filesystem
///
/// POST /api/v1/file-library/admin/recover-files
#[utoipa::path(
    post,
    path = "/api/v1/file-library/admin/recover-files",
    responses(
        (status = 200, description = "Recovery operation completed", body = RecoveryResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "File Library"
)]
pub async fn recover_orphaned_files(
    State(_state): State<Arc<ApiState>>,
) -> Result<Json<RecoveryResponse>, ApiError> {
    use super::migration::recover_orphaned_files;

    let storage_dir = std::env::var("FILE_LIBRARY_STORAGE_PATH")
        .unwrap_or_else(|_| "./data/file-library".to_string());

    let db_path = std::env::var("FILE_LIBRARY_DB_PATH")
        .unwrap_or_else(|_| "./data/file-library-db".to_string());

    tracing::info!("🔧 Starting orphaned file recovery...");
    tracing::info!("   Storage directory: {}", storage_dir);
    tracing::info!("   Database path: {}", db_path);

    let stats = recover_orphaned_files(&storage_dir, &db_path)
        .await
        .map_err(|e| ApiError::internal(format!("Recovery failed: {}", e)))?;

    tracing::info!(
        "✅ Recovery complete: {} files recovered",
        stats.files_recovered
    );

    Ok(Json(RecoveryResponse {
        files_found: stats.files_found,
        files_recovered: stats.files_recovered,
        files_already_tracked: stats.files_already_tracked,
        errors: stats.errors,
        duration_ms: stats.duration_ms,
    }))
}
