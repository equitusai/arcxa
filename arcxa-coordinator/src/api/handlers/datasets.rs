//! Dataset Import Handlers
//!
//! **ARCHITECTURAL NOTE (2024)**: This module is being phased out in favor of the file library pattern.
//!
//! ## New Architecture
//!
//! **All CSV/file uploads should go through the file library first:**
//! 1. Upload file: `POST /api/v1/file-library/files`
//! 2. Use file_id in ETL operations
//!
//! ## Migration Path
//!
//! - **Old**: `POST /api/v1/datasets/import` (direct upload) → DEPRECATED
//! - **New**: `POST /api/v1/file-library/files` → then reference file_id
//!
//! This handler is kept for backward compatibility but logs deprecation warnings.

use axum::{
    extract::{Extension, Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::api::auth::Claims;
use crate::api::dto::datasets::*;
use crate::api::ApiState;
use crate::common::csv_utils::{
    detect_delimiter_advanced, parse_csv_line_advanced, CsvDetectionConfig,
};

// Security imports for SQL injection prevention
use graphica_core::security::validate_identifier;

// Arrow2 imports for high-performance Parquet conversion
use arrow2::{
    array::*,
    chunk::Chunk,
    datatypes::{DataType, Field, Schema},
    io::parquet::write::{
        CompressionOptions, Encoding, FileWriter, RowGroupIterator, Version, WriteOptions,
    },
};
use graphica_core::catalog::api_types::QueryResult;

// ============================================================================
// Dataset Import Handler
// ============================================================================

/// Import dataset from uploaded file
///
/// **DEPRECATED**: Use file library instead for proper architecture.
///
/// ## Architecture Violation
///
/// This endpoint bypasses the file library, which means:
/// - No centralized file metadata
/// - No access control integration
/// - No lineage with other file operations
/// - No file versioning or tagging
///
/// ## Recommended Migration
///
/// 1. Upload file to library: `POST /api/v1/file-library/files`
/// 2. Use returned file_id in ETL operations
/// 3. Reference file_id for lineage and metadata
///
/// POST /api/v1/datasets/import
#[deprecated(
    since = "2024.0.0",
    note = "Use file library pattern: upload to /api/v1/file-library/files first"
)]
pub async fn import_dataset(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    mut multipart: Multipart,
) -> Result<Json<ImportDatasetResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    warn!("⚠️  DEPRECATED API: POST /api/v1/datasets/import bypasses file library architecture");
    warn!("    Please migrate to: POST /api/v1/file-library/files → use file_id");
    warn!("    See https://docs.graphica.io/file-library for migration guide");

    info!("📦 Starting dataset import (deprecated flow)...");

    // Extract file and metadata from multipart form
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut metadata: Option<ImportMetadata> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        create_error(
            StatusCode::BAD_REQUEST,
            "MULTIPART_ERROR",
            &format!("Failed to read multipart field: {}", e),
        )
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            create_error(
                                StatusCode::BAD_REQUEST,
                                "FILE_READ_ERROR",
                                &format!("Failed to read file data: {}", e),
                            )
                        })?
                        .to_vec(),
                );
            }
            "metadata" => {
                let metadata_bytes = field.bytes().await.map_err(|e| {
                    create_error(
                        StatusCode::BAD_REQUEST,
                        "METADATA_READ_ERROR",
                        &format!("Failed to read metadata: {}", e),
                    )
                })?;

                metadata = Some(serde_json::from_slice(&metadata_bytes).map_err(|e| {
                    create_error(
                        StatusCode::BAD_REQUEST,
                        "INVALID_METADATA",
                        &format!("Invalid metadata JSON: {}", e),
                    )
                })?);
            }
            _ => {
                // Unknown field, skip
            }
        }
    }

    // Validate required fields
    let file_data = file_data
        .ok_or_else(|| create_error(StatusCode::BAD_REQUEST, "MISSING_FILE", "File is required"))?;

    let filename = filename.ok_or_else(|| {
        create_error(
            StatusCode::BAD_REQUEST,
            "MISSING_FILENAME",
            "Filename is required",
        )
    })?;

    // Use metadata or create default
    let metadata = metadata.unwrap_or_else(|| ImportMetadata {
        name: filename.clone(),
        description: None,
        tags: vec![],
        schema: None,
    });

    info!(
        "📄 Processing file: {} ({} bytes)",
        filename,
        file_data.len()
    );

    // Check file size limits
    const MAX_FILE_SIZE: usize = 100 * 1024 * 1024; // 100 MB
    if file_data.len() > MAX_FILE_SIZE {
        return Err(create_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "FILE_TOO_LARGE",
            &format!(
                "File size exceeds {} MB limit",
                MAX_FILE_SIZE / (1024 * 1024)
            ),
        ));
    }

    // Detect file format from extension
    let file_format = detect_file_format(&filename)?;
    info!("📊 Detected format: {:?}", file_format);

    // Parse file and detect schema
    let (record_count, schema) =
        parse_file_and_detect_schema(&file_data, file_format, metadata.schema.as_ref()).await?;

    info!(
        "✅ Parsed {} records with schema: {} columns",
        record_count,
        schema.columns.len()
    );

    // Generate IDs
    let import_id = format!("import_{}", generate_id());
    let dataset_id = format!("ds_import_{}", generate_id());

    // Create storage paths
    let storage_path =
        std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());
    std::fs::create_dir_all(&storage_path).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STORAGE_ERROR",
            &format!("Failed to create storage directory: {}", e),
        )
    })?;

    let parquet_path = format!("{}/{}.parquet", storage_path, dataset_id);

    // Store as Parquet (placeholder - would use Apache Arrow in production)
    // For now, just acknowledge the file
    info!("💾 Storing dataset at: {}", parquet_path);

    // Extract user ID from JWT auth context
    let user_id = claims
        .as_ref()
        .map(|ext| ext.0.sub.clone())
        .unwrap_or_else(|| {
            // Fallback for development/testing when auth is disabled
            debug!("No auth claims found, using default user_id");
            "user_admin".to_string()
        });

    // Generate lineage metadata
    let timestamp = Utc::now();
    let lineage = ImportLineage {
        import_method: "file_upload".to_string(),
        source_file: filename.clone(),
        imported_by: user_id.clone(),
        imported_at: timestamp.to_rfc3339(),
        import_id: import_id.clone(),
    };

    // Generate and store RDF triples for lineage
    if let Err(e) = store_import_lineage(
        &state,
        &dataset_id,
        &import_id,
        &metadata.name,
        record_count,
        &lineage,
        &schema,
    )
    .await
    {
        error!("⚠️  Failed to store lineage: {}", e);
        // Don't fail the import, just log the warning
    }

    // Build response
    let response = ImportDatasetResponse {
        dataset_id: dataset_id.clone(),
        name: metadata.name.clone(),
        status: ImportStatus::Imported,
        record_count,
        file_size_bytes: file_data.len() as u64,
        schema,
        lineage,
        storage: StorageInfo {
            format: "parquet".to_string(),
            path: parquet_path,
            compressed: true,
        },
    };

    info!("🎉 Import complete: dataset_id={}", dataset_id);

    Ok(Json(response))
}

// ============================================================================
// Datasource Import Handler
// ============================================================================

/// Import dataset from connected datasource
/// POST /api/v1/datasets/import-datasource
pub async fn import_from_datasource(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<DatasourceImportRequest>,
) -> Result<Json<ImportDatasetResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    info!(
        "📦 Starting datasource import from source: {}",
        request.source_id
    );

    // Check if async mode requested or dataset is large (>10k rows)
    let should_async = request.async_mode || request.limit.map(|l| l > 10_000).unwrap_or(false);

    if should_async {
        return handle_async_datasource_import(state, claims, request).await;
    }

    // Continue with synchronous import
    handle_sync_datasource_import(state, claims, request).await
}

/// Handle async datasource import (spawns background job)
async fn handle_async_datasource_import(
    state: Arc<ApiState>,
    claims: Option<Extension<Claims>>,
    request: DatasourceImportRequest,
) -> Result<Json<ImportDatasetResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    use crate::api::import_jobs::ImportJobRequest;

    info!("🚀 Starting ASYNC datasource import");

    // Generate import ID
    let import_id = format!("import_{}", generate_id());

    // Extract user ID
    let user_id = claims
        .as_ref()
        .map(|ext| ext.0.sub.clone())
        .unwrap_or_else(|| "user_admin".to_string());

    // Create job request
    let job_request = ImportJobRequest::from(&request);

    // Register job
    state
        .import_job_manager
        .register_job(import_id.clone(), job_request.clone());

    // Spawn background task
    let state_clone = state.clone();
    let import_id_clone = import_id.clone();
    let user_id_clone = user_id.clone();

    tokio::spawn(async move {
        // Call async import executor
        let result = crate::api::handlers::datasets_async::execute_async_import(
            state_clone,
            import_id_clone,
            job_request,
            user_id_clone,
        )
        .await;
    });

    // Return immediately with pending status
    let dataset_name = request
        .name
        .clone()
        .unwrap_or_else(|| request.table.clone());

    Ok(Json(ImportDatasetResponse {
        dataset_id: format!("pending_{}", import_id),
        name: dataset_name,
        status: ImportStatus::Pending,
        record_count: 0,
        file_size_bytes: 0,
        schema: SchemaDefinition {
            primary_key: None,
            columns: vec![],
        },
        lineage: ImportLineage {
            import_method: "datasource_query_async".to_string(),
            source_file: format!("{}:{}", request.source_id, request.table),
            imported_by: user_id,
            imported_at: Utc::now().to_rfc3339(),
            import_id: import_id.clone(),
        },
        storage: StorageInfo {
            format: "parquet".to_string(),
            path: "pending".to_string(),
            compressed: true,
        },
    }))
}

/// Handle synchronous datasource import (original logic)
async fn handle_sync_datasource_import(
    state: Arc<ApiState>,
    claims: Option<Extension<Claims>>,
    request: DatasourceImportRequest,
) -> Result<Json<ImportDatasetResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    info!("🔄 Synchronous datasource import mode");

    // Get catalog
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Datasource catalog is not configured",
        )
    })?;

    // Get datasource details
    let datasource = catalog.get_source(&request.source_id).await.map_err(|e| {
        create_error(
            StatusCode::NOT_FOUND,
            "DATASOURCE_NOT_FOUND",
            &format!("Datasource not found: {}", e),
        )
    })?;

    info!("📊 Found datasource: {}", datasource.source.title);

    // Validate table name to prevent SQL injection
    let validated_table = validate_identifier(&request.table)
        .map_err(|e| {
            error!("❌ Invalid table name '{}': {}", request.table, e);
            create_error(
                StatusCode::BAD_REQUEST,
                "INVALID_TABLE_NAME",
                &format!("Invalid table name '{}': {}. Table names must be alphanumeric with underscores only.", request.table, e),
            )
        })?;

    // Validate column names to prevent SQL injection
    let validated_columns: Result<Vec<&str>, _> = request
        .columns
        .iter()
        .map(|col| validate_identifier(col))
        .collect();

    let validated_columns = validated_columns.map_err(|e| {
        error!("❌ Invalid column name: {}", e);
        create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_COLUMN_NAME",
            &format!(
                "Invalid column name: {}. Column names must be alphanumeric with underscores only.",
                e
            ),
        )
    })?;

    // WHERE clause is temporarily disabled due to SQL injection risk
    // TODO: Implement parameterized queries or SQL parser before re-enabling
    if request.where_clause.is_some() {
        error!("❌ WHERE clause not yet supported (SQL injection risk)");
        return Err(create_error(
            StatusCode::BAD_REQUEST,
            "WHERE_CLAUSE_NOT_SUPPORTED",
            "WHERE clauses are not yet supported due to SQL injection risk. Please use the limit parameter for now.",
        ));
    }

    // Build SQL query with validated identifiers
    let columns_clause = if validated_columns.is_empty() {
        "*".to_string()
    } else {
        validated_columns.join(", ")
    };

    let mut query = format!("SELECT {} FROM {}", columns_clause, validated_table);

    // Add LIMIT if provided
    if let Some(limit) = request.limit {
        query.push_str(&format!(" LIMIT {}", limit));
    }

    info!("🔍 Executing query: {}", query);

    // Execute query via catalog
    let query_result = catalog
        .execute_query(
            &request.source_id,
            &query,
            std::collections::HashMap::new(),
            request.limit,
        )
        .await
        .map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "QUERY_EXECUTION_FAILED",
                &format!("Failed to execute query: {}", e),
            )
        })?;

    info!(
        "✅ Query returned {} rows in {}ms",
        query_result.row_count, query_result.execution_time_ms
    );

    // Generate IDs
    let import_id = format!("import_{}", generate_id());
    let dataset_id = format!("ds_datasource_{}", generate_id());

    // Create dataset name
    let dataset_name = request
        .name
        .unwrap_or_else(|| format!("{}_{}", datasource.source.title, request.table));

    // Convert QueryResult columns to schema
    let schema = SchemaDefinition {
        primary_key: None,
        columns: query_result
            .columns
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .map(|col| {
                        // Map from graphica_core::catalog::api_types::ColumnDefinition
                        // to crate::api::dto::datasets::ColumnDefinition
                        ColumnDefinition {
                            name: col.name.clone(),
                            data_type: col.data_type.clone(),
                            nullable: col.nullable,
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(Vec::new),
    };

    info!("📋 Detected schema with {} columns", schema.columns.len());

    // Prepare storage directory
    let storage_path =
        std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());
    std::fs::create_dir_all(&storage_path).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STORAGE_ERROR",
            &format!("Failed to create storage directory: {}", e),
        )
    })?;

    let parquet_path = format!("{}/{}.parquet", storage_path, dataset_id);
    info!("💾 Writing to Parquet: {}", parquet_path);

    // Convert QueryResult to Parquet with high-performance Arrow2
    let file_size_bytes = write_query_result_to_parquet(&query_result, &parquet_path)?;

    // Extract user ID from JWT auth context
    let user_id = claims
        .as_ref()
        .map(|ext| ext.0.sub.clone())
        .unwrap_or_else(|| {
            // Fallback for development/testing when auth is disabled
            debug!("No auth claims found, using default user_id");
            "user_admin".to_string()
        });

    // Generate lineage metadata
    let timestamp = Utc::now();
    let lineage = ImportLineage {
        import_method: "datasource_query".to_string(),
        source_file: format!("{}:{}", request.source_id, request.table),
        imported_by: user_id.clone(),
        imported_at: timestamp.to_rfc3339(),
        import_id: import_id.clone(),
    };

    // Generate and store RDF triples for datasource lineage
    if let Err(e) = store_datasource_lineage(
        &state,
        &dataset_id,
        &import_id,
        &dataset_name,
        query_result.row_count as u64,
        &lineage,
        &schema,
        &request.source_id,
        &request.table,
        request.where_clause.as_deref(),
        "parquet",
        &parquet_path,
        file_size_bytes,
    )
    .await
    {
        error!("⚠️  Failed to store lineage: {}", e);
        // Don't fail the import, just log the warning
    }

    // Build response
    let response = ImportDatasetResponse {
        dataset_id: dataset_id.clone(),
        name: dataset_name,
        status: ImportStatus::Imported,
        record_count: query_result.row_count as u64,
        file_size_bytes,
        schema,
        lineage,
        storage: StorageInfo {
            format: "parquet".to_string(),
            path: parquet_path,
            compressed: true,
        },
    };

    info!("🎉 Datasource import complete: dataset_id={}", dataset_id);

    Ok(Json(response))
}

// ============================================================================
// Batch Datasource Import Handler
// ============================================================================

/// Batch import multiple tables from a datasource
/// POST /api/v1/datasets/import-batch
pub async fn batch_import_datasources(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<BatchDatasourceImportRequest>,
) -> Result<Json<BatchImportResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    use crate::api::import_jobs::ImportJobRequest;

    info!(
        "📦 Starting batch import of {} tables from source: {}",
        request.tables.len(),
        request.source_id
    );

    let batch_id = format!("batch_{}", generate_id());
    let mut import_ids = Vec::new();

    // Extract user ID
    let user_id = claims
        .as_ref()
        .map(|ext| ext.0.sub.clone())
        .unwrap_or_else(|| "user_admin".to_string());

    for table_import in request.tables {
        let import_id = format!("import_{}", generate_id());

        let job_request = ImportJobRequest {
            name: table_import.name.clone(),
            source_id: request.source_id.clone(),
            table: table_import.table.clone(),
            where_clause: table_import.where_clause,
            columns: table_import.columns,
            limit: table_import.limit,
            profile: request.profile,
            tags: request.tags.clone(),
            description: None,
        };

        // Register job
        state
            .import_job_manager
            .register_job(import_id.clone(), job_request.clone());

        // Spawn background task
        let state_clone = state.clone();
        let user_id_clone = user_id.clone();
        let import_id_clone = import_id.clone();

        tokio::spawn(async move {
            crate::api::handlers::datasets_async::execute_async_import(
                state_clone,
                import_id_clone,
                job_request,
                user_id_clone,
            )
            .await;
        });

        import_ids.push(import_id);
        info!("✅ Queued import for table: {}", table_import.table);
    }

    info!("🎉 Batch import queued: {} tables", import_ids.len());

    Ok(Json(BatchImportResponse {
        batch_id,
        import_ids,
        status: "processing".to_string(),
        started_at: Utc::now().to_rfc3339(),
    }))
}

// ============================================================================
// Get Import Status Handler
// ============================================================================

/// Get import status
/// GET /api/v1/datasets/imports/:import_id
pub async fn get_import_status(
    State(state): State<Arc<ApiState>>,
    Path(import_id): Path<String>,
) -> Result<Json<ImportStatusResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    info!("📊 Getting import status for: {}", import_id);

    match state.import_job_manager.get_status(&import_id) {
        Some(job_status) => {
            info!("✅ Found job status: {:?}", job_status.status);
            Ok(Json((&job_status).into()))
        }
        None => {
            warn!("⚠️  Import job not found: {}", import_id);
            Err(create_error(
                StatusCode::NOT_FOUND,
                "IMPORT_NOT_FOUND",
                &format!("Import job not found: {}", import_id),
            ))
        }
    }
}

// ============================================================================
// List Imports Handler
// ============================================================================

/// List all imports with filtering
/// GET /api/v1/datasets/imports
pub async fn list_imports(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListImportsQuery>,
) -> Result<Json<ListImportsResponse>, (StatusCode, Json<ImportErrorResponse>)> {
    info!(
        "📋 Listing imports: status={:?}, page={}, page_size={}",
        query.status, query.page, query.page_size
    );

    let imports = state
        .import_job_manager
        .list_jobs(query.status.clone(), query.page_size);
    let total = imports.len();

    info!("✅ Found {} imports", total);

    Ok(Json(ListImportsResponse {
        imports,
        total,
        page: query.page,
        page_size: query.page_size,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// File format enum
#[derive(Debug)]
enum FileFormat {
    Csv,
    Parquet,
    JsonLines,
    JsonArray,
}

/// Detect file format from filename
fn detect_file_format(
    filename: &str,
) -> Result<FileFormat, (StatusCode, Json<ImportErrorResponse>)> {
    let lowercase = filename.to_lowercase();

    if lowercase.ends_with(".csv") {
        Ok(FileFormat::Csv)
    } else if lowercase.ends_with(".parquet") {
        Ok(FileFormat::Parquet)
    } else if lowercase.ends_with(".jsonl") {
        Ok(FileFormat::JsonLines)
    } else if lowercase.ends_with(".json") {
        Ok(FileFormat::JsonArray)
    } else {
        Err(create_error(
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_FORMAT",
            &format!("Unsupported file format. Supported: .csv, .parquet, .jsonl, .json"),
        ))
    }
}

/// Parse file and detect schema
async fn parse_file_and_detect_schema(
    data: &[u8],
    format: FileFormat,
    provided_schema: Option<&SchemaDefinition>,
) -> Result<(u64, SchemaDefinition), (StatusCode, Json<ImportErrorResponse>)> {
    match format {
        FileFormat::Csv => parse_csv(data, provided_schema).await,
        FileFormat::Parquet => parse_parquet(data, provided_schema).await,
        FileFormat::JsonLines => parse_jsonl(data, provided_schema).await,
        FileFormat::JsonArray => parse_json_array(data, provided_schema).await,
    }
}

/// Parse CSV file using common CSV utilities
async fn parse_csv(
    data: &[u8],
    provided_schema: Option<&SchemaDefinition>,
) -> Result<(u64, SchemaDefinition), (StatusCode, Json<ImportErrorResponse>)> {
    // Convert bytes to string
    let content = std::str::from_utf8(data).map_err(|e| {
        create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_ENCODING",
            &format!("CSV file is not valid UTF-8: {}", e),
        )
    })?;

    // Split into lines
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if lines.is_empty() {
        return Ok((
            0,
            SchemaDefinition {
                primary_key: None,
                columns: vec![],
            },
        ));
    }

    // Detect delimiter if not provided
    let delimiter = if lines.len() > 1 {
        // Use common utility to detect delimiter
        let config = CsvDetectionConfig::default();
        // For in-memory data, we can't use Path-based detection, so use simple heuristic
        detect_delimiter_from_lines(&lines)
    } else {
        ",".to_string()
    };

    // Parse header row (first line)
    let headers = parse_csv_line_advanced(&lines[0], &delimiter).map_err(|e| {
        create_error(
            StatusCode::BAD_REQUEST,
            "CSV_PARSE_ERROR",
            &format!("Failed to parse CSV header: {}", e),
        )
    })?;

    // Count data rows
    let record_count = lines.len().saturating_sub(1) as u64;

    // If schema provided, use it; otherwise infer from headers
    let schema = if let Some(provided) = provided_schema {
        provided.clone()
    } else {
        // Create basic schema from headers
        // In a real implementation, we'd sample data rows to infer types
        let columns = headers
            .iter()
            .map(|header| {
                ColumnDefinition {
                    name: header.clone(),
                    data_type: "VARCHAR".to_string(), // Default to VARCHAR
                    nullable: true,
                }
            })
            .collect();

        SchemaDefinition {
            primary_key: headers.first().cloned(),
            columns,
        }
    };

    Ok((record_count, schema))
}

/// Simple delimiter detection from lines (in-memory version)
fn detect_delimiter_from_lines(lines: &[String]) -> String {
    let candidates = vec![",", "\t", "|", ";"];
    let sample = &lines[0..lines.len().min(10)];

    let mut best_delim = ",";
    let mut best_consistency = 0.0;

    for delim in &candidates {
        let mut counts = Vec::new();
        for line in sample {
            if let Ok(fields) = parse_csv_line_advanced(line, delim) {
                counts.push(fields.len());
            }
        }

        if counts.is_empty() {
            continue;
        }

        // Calculate consistency (lower stddev = more consistent)
        let avg = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
        if avg > 1.0 {
            let variance: f64 = counts
                .iter()
                .map(|&c| (c as f64 - avg).powi(2))
                .sum::<f64>()
                / counts.len() as f64;
            let stddev = variance.sqrt();
            let consistency = 1.0 - (stddev / avg).min(1.0);

            if consistency > best_consistency {
                best_consistency = consistency;
                best_delim = delim;
            }
        }
    }

    best_delim.to_string()
}

/// Parse Parquet file
async fn parse_parquet(
    _data: &[u8],
    provided_schema: Option<&SchemaDefinition>,
) -> Result<(u64, SchemaDefinition), (StatusCode, Json<ImportErrorResponse>)> {
    // TODO: Implement Parquet parsing with arrow crate

    let schema = provided_schema
        .cloned()
        .unwrap_or_else(|| SchemaDefinition {
            primary_key: None,
            columns: vec![],
        });

    Ok((0, schema))
}

/// Parse JSON Lines file
async fn parse_jsonl(
    data: &[u8],
    provided_schema: Option<&SchemaDefinition>,
) -> Result<(u64, SchemaDefinition), (StatusCode, Json<ImportErrorResponse>)> {
    // TODO: Implement JSONL parsing

    let line_count = data.iter().filter(|&&b| b == b'\n').count() as u64;

    let schema = provided_schema
        .cloned()
        .unwrap_or_else(|| SchemaDefinition {
            primary_key: None,
            columns: vec![],
        });

    Ok((line_count, schema))
}

/// Parse JSON array file
async fn parse_json_array(
    data: &[u8],
    provided_schema: Option<&SchemaDefinition>,
) -> Result<(u64, SchemaDefinition), (StatusCode, Json<ImportErrorResponse>)> {
    // TODO: Implement JSON array parsing

    let json_value: serde_json::Value = serde_json::from_slice(data).map_err(|e| {
        create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_JSON",
            &format!("Invalid JSON: {}", e),
        )
    })?;

    let record_count = match json_value {
        serde_json::Value::Array(arr) => arr.len() as u64,
        _ => {
            return Err(create_error(
                StatusCode::BAD_REQUEST,
                "INVALID_JSON_STRUCTURE",
                "JSON file must contain an array at the root",
            ))
        }
    };

    let schema = provided_schema
        .cloned()
        .unwrap_or_else(|| SchemaDefinition {
            primary_key: None,
            columns: vec![],
        });

    Ok((record_count, schema))
}

fn escape_turtle_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn sanitize_iri_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn dataset_uri(dataset_id: &str) -> String {
    format!("http://graphica.io/ontology/dataset/{}", dataset_id)
}

fn import_uri(import_id: &str) -> String {
    format!("http://graphica.io/ontology/import/{}", import_id)
}

fn user_uri(user_id: &str) -> String {
    format!(
        "http://graphica.io/ontology/user/{}",
        sanitize_iri_token(user_id)
    )
}

fn datasource_uri(source_id: &str) -> String {
    format!(
        "http://graphica.io/ontology/datasource/{}",
        sanitize_iri_token(source_id)
    )
}

fn workflow_definition_uri(workflow_id: &str) -> String {
    format!(
        "{}workflow/{}",
        crate::governance::ontology::WORKFLOW_NS,
        sanitize_iri_token(workflow_id)
    )
}

fn workflow_execution_uri(execution_id: &str) -> String {
    format!(
        "{}execution/{}",
        crate::governance::ontology::WORKFLOW_NS,
        sanitize_iri_token(execution_id)
    )
}

fn build_schema_triples(dataset_id: &str, schema: &SchemaDefinition) -> String {
    let dataset = dataset_uri(dataset_id);

    schema
        .columns
        .iter()
        .map(|column| {
            let column_uri = format!("{}/column/{}", dataset, sanitize_iri_token(&column.name));

            format!(
                r#"
<{dataset}> gph:hasColumn <{column_uri}> .
<{column_uri}> a gph:DatasetColumn ;
    gph:columnName "{column_name}" ;
    gph:columnType "{column_type}" ;
    gph:nullable {nullable} .
"#,
                dataset = dataset,
                column_uri = column_uri,
                column_name = escape_turtle_literal(&column.name),
                column_type = escape_turtle_literal(&column.data_type),
                nullable = column.nullable
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn build_import_lineage_turtle(
    dataset_id: &str,
    import_id: &str,
    dataset_name: &str,
    record_count: u64,
    lineage: &ImportLineage,
    schema: &SchemaDefinition,
) -> String {
    let dataset = dataset_uri(dataset_id);
    let import = import_uri(import_id);
    let user = user_uri(&lineage.imported_by);
    let schema_triples = build_schema_triples(dataset_id, schema);

    format!(
        r#"
@prefix gph: <http://graphica.io/ontology#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Dataset
<{dataset}> a gph:Dataset ;
    gph:datasetName "{dataset_name}" ;
    gph:datasetType "imported" ;
    gph:recordCount {record_count} ;
    gph:createdAt "{imported_at}"^^xsd:dateTime ;
    gph:importedFrom <{import}> .

# Import Activity (W3C PROV)
<{import}> a prov:Activity ;
    prov:startedAtTime "{imported_at}"^^xsd:dateTime ;
    prov:endedAtTime "{imported_at}"^^xsd:dateTime ;
    prov:wasAssociatedWith <{user}> ;
    gph:sourceFile "{source_file}" ;
    gph:importMethod "{import_method}" .

# Entity Generated (dataset was generated by import)
<{dataset}> prov:wasGeneratedBy <{import}> .

# User Attribution
<{user}> a prov:Agent .
{schema_triples}"#,
        dataset = dataset,
        dataset_name = escape_turtle_literal(dataset_name),
        record_count = record_count,
        imported_at = lineage.imported_at,
        import = import,
        user = user,
        source_file = escape_turtle_literal(&lineage.source_file),
        import_method = escape_turtle_literal(&lineage.import_method),
        schema_triples = schema_triples
    )
}

fn build_datasource_lineage_turtle(
    dataset_id: &str,
    import_id: &str,
    dataset_name: &str,
    record_count: u64,
    lineage: &ImportLineage,
    schema: &SchemaDefinition,
    source_id: &str,
    table_name: &str,
    where_clause: Option<&str>,
    storage_format: &str,
    storage_path: &str,
    file_size_bytes: u64,
) -> String {
    let dataset = dataset_uri(dataset_id);
    let import = import_uri(import_id);
    let user = user_uri(&lineage.imported_by);
    let datasource = datasource_uri(source_id);
    let schema_triples = build_schema_triples(dataset_id, schema);
    let query_filter = where_clause.unwrap_or_default();

    format!(
        r#"
@prefix gph: <http://graphica.io/ontology#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Imported Dataset
<{dataset}> a gph:Dataset ;
    gph:datasetName "{dataset_name}" ;
    gph:datasetType "imported" ;
    gph:recordCount {record_count} ;
    gph:createdAt "{imported_at}"^^xsd:dateTime ;
    gph:storageFormat "{storage_format}" ;
    gph:storagePath "{storage_path}" ;
    gph:fileSizeBytes {file_size_bytes} ;
    gph:sourceDataSource "{source_id}" ;
    gph:importedFrom <{import}> ;
    prov:wasDerivedFrom <{datasource}> .

# Import Activity (W3C PROV)
<{import}> a prov:Activity ;
    prov:startedAtTime "{imported_at}"^^xsd:dateTime ;
    prov:endedAtTime "{imported_at}"^^xsd:dateTime ;
    prov:wasAssociatedWith <{user}> ;
    gph:sourceTable "{table_name}" ;
    gph:queryFilter "{query_filter}" ;
    gph:importMethod "{import_method}" ;
    prov:used <{datasource}> .

# Dataset was generated by import activity
<{dataset}> prov:wasGeneratedBy <{import}> .

# User attribution
<{user}> a prov:Agent .

# Source datasource reference
<{datasource}> a gph:DataSource ;
    gph:datasourceId "{source_id}" .
{schema_triples}"#,
        dataset = dataset,
        dataset_name = escape_turtle_literal(dataset_name),
        record_count = record_count,
        imported_at = lineage.imported_at,
        storage_format = escape_turtle_literal(storage_format),
        storage_path = escape_turtle_literal(storage_path),
        file_size_bytes = file_size_bytes,
        source_id = escape_turtle_literal(source_id),
        import = import,
        datasource = datasource,
        user = user,
        table_name = escape_turtle_literal(table_name),
        query_filter = escape_turtle_literal(query_filter),
        import_method = escape_turtle_literal(&lineage.import_method),
        schema_triples = schema_triples
    )
}

fn build_workflow_output_lineage_turtle(
    dataset_id: &str,
    dataset_name: &str,
    record_count: u64,
    schema: &SchemaDefinition,
    workflow_id: &str,
    execution_id: &str,
    workflow_name: &str,
    completed_at: &str,
    storage_format: &str,
    storage_path: &str,
    file_size_bytes: u64,
) -> String {
    let dataset = dataset_uri(dataset_id);
    let workflow = workflow_definition_uri(workflow_id);
    let execution = workflow_execution_uri(execution_id);
    let schema_triples = build_schema_triples(dataset_id, schema);

    format!(
        r#"
@prefix gph: <http://graphica.io/ontology#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix workflow: <{workflow_ns}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Workflow Output Dataset
<{dataset}> a gph:Dataset ;
    gph:datasetName "{dataset_name}" ;
    gph:datasetType "workflow_output" ;
    gph:recordCount {record_count} ;
    gph:createdAt "{completed_at}"^^xsd:dateTime ;
    gph:updatedAt "{completed_at}"^^xsd:dateTime ;
    gph:storageFormat "{storage_format}" ;
    gph:storagePath "{storage_path}" ;
    gph:fileSizeBytes {file_size_bytes} ;
    gph:producedByWorkflow <{execution}> ;
    prov:wasGeneratedBy <{execution}> .

# Workflow Definition
<{workflow}> workflow:workflowName "{workflow_name}" .

{schema_triples}"#,
        workflow_ns = crate::governance::ontology::WORKFLOW_NS,
        dataset = dataset,
        dataset_name = escape_turtle_literal(dataset_name),
        record_count = record_count,
        completed_at = completed_at,
        storage_format = escape_turtle_literal(storage_format),
        storage_path = escape_turtle_literal(storage_path),
        file_size_bytes = file_size_bytes,
        execution = execution,
        workflow = workflow,
        workflow_name = escape_turtle_literal(workflow_name),
        schema_triples = schema_triples
    )
}

/// Store import lineage as RDF triples
async fn store_import_lineage(
    state: &ApiState,
    dataset_id: &str,
    import_id: &str,
    dataset_name: &str,
    record_count: u64,
    lineage: &ImportLineage,
    schema: &SchemaDefinition,
) -> Result<(), String> {
    use crate::governance::rdf_store::{NamedGraph, RdfStore};

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| "RDF store not available".to_string())?;

    let turtle = build_import_lineage_turtle(
        dataset_id,
        import_id,
        dataset_name,
        record_count,
        lineage,
        schema,
    );

    info!(
        "📝 Generated {} bytes of RDF triples for lineage",
        turtle.len()
    );
    info!("🔍 RDF Triples:\n{}", turtle);

    // Insert triples into RDF store using load_turtle
    let graph_uri_str = format!("http://graphica.io/catalog/imports/{}", import_id);
    let graph = NamedGraph::new(graph_uri_str.clone());

    match rdf_store.load_turtle(&turtle, Some(&graph)) {
        Ok(_) => {
            info!("✅ Stored lineage triples in graph: {}", graph_uri_str);
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to store lineage: {}", e);
            Err(e.to_string())
        }
    }
}

/// Store datasource import lineage as RDF triples with prov:wasDerivedFrom
pub async fn store_datasource_lineage(
    state: &ApiState,
    dataset_id: &str,
    import_id: &str,
    dataset_name: &str,
    record_count: u64,
    lineage: &ImportLineage,
    schema: &SchemaDefinition,
    source_id: &str,
    table_name: &str,
    where_clause: Option<&str>,
    storage_format: &str,
    storage_path: &str,
    file_size_bytes: u64,
) -> Result<(), String> {
    use crate::governance::rdf_store::{NamedGraph, RdfStore};

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| "RDF store not available".to_string())?;

    let turtle = build_datasource_lineage_turtle(
        dataset_id,
        import_id,
        dataset_name,
        record_count,
        lineage,
        schema,
        source_id,
        table_name,
        where_clause,
        storage_format,
        storage_path,
        file_size_bytes,
    );

    info!(
        "📝 Generated {} bytes of RDF triples for datasource lineage",
        turtle.len()
    );
    info!("🔍 RDF Triples:\\n{}", turtle);

    // Insert triples into RDF store using load_turtle
    let graph_uri_str = format!("http://graphica.io/catalog/imports/{}", import_id);
    let graph = NamedGraph::new(graph_uri_str.clone());

    match rdf_store.load_turtle(&turtle, Some(&graph)) {
        Ok(_) => {
            info!(
                "✅ Stored datasource lineage triples in graph: {}",
                graph_uri_str
            );
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to store datasource lineage: {}", e);
            Err(e.to_string())
        }
    }
}

/// Store workflow output lineage as RDF triples with producedByWorkflow linkage.
pub async fn store_workflow_output_lineage(
    state: &ApiState,
    dataset_id: &str,
    dataset_name: &str,
    record_count: u64,
    schema: &SchemaDefinition,
    workflow_id: &str,
    execution_id: &str,
    workflow_name: &str,
    completed_at: &str,
    storage_format: &str,
    storage_path: &str,
    file_size_bytes: u64,
) -> Result<(), String> {
    use crate::governance::rdf_store::{NamedGraph, RdfStore};

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| "RDF store not available".to_string())?;

    let turtle = build_workflow_output_lineage_turtle(
        dataset_id,
        dataset_name,
        record_count,
        schema,
        workflow_id,
        execution_id,
        workflow_name,
        completed_at,
        storage_format,
        storage_path,
        file_size_bytes,
    );

    let graph_uri_str = format!("http://graphica.io/catalog/workflows/{}", execution_id);
    let graph = NamedGraph::new(graph_uri_str.clone());

    match rdf_store.load_turtle(&turtle, Some(&graph)) {
        Ok(_) => {
            info!(
                "✅ Stored workflow output lineage triples in graph: {}",
                graph_uri_str
            );
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to store workflow output lineage: {}", e);
            Err(e.to_string())
        }
    }
}

/// Convert QueryResult to Parquet file with high-performance Arrow2
/// Returns the file size in bytes
pub fn write_query_result_to_parquet(
    query_result: &QueryResult,
    parquet_path: &str,
) -> Result<u64, (StatusCode, Json<ImportErrorResponse>)> {
    debug!(
        "Starting Parquet conversion for {} rows",
        query_result.row_count
    );

    // Extract column definitions
    let columns = query_result.columns.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "MISSING_SCHEMA",
            "Query result missing column definitions",
        )
    })?;

    if columns.is_empty() {
        return Err(create_error(
            StatusCode::BAD_REQUEST,
            "EMPTY_SCHEMA",
            "Cannot create Parquet file with no columns",
        ));
    }

    // Build Arrow schema from column definitions
    let fields: Vec<Field> = columns
        .iter()
        .map(|col| {
            // Map SQL types to Arrow types (simplified mapping for MVP)
            let data_type = match col.data_type.to_uppercase().as_str() {
                dt if dt.contains("INT") || dt.contains("SERIAL") => DataType::Int64,
                dt if dt.contains("FLOAT") || dt.contains("DOUBLE") || dt.contains("REAL") => {
                    DataType::Float64
                }
                dt if dt.contains("BOOL") => DataType::Boolean,
                dt if dt.contains("DATE") && !dt.contains("TIME") => DataType::Date32,
                dt if dt.contains("TIMESTAMP") || dt.contains("DATETIME") => {
                    DataType::Timestamp(arrow2::datatypes::TimeUnit::Millisecond, None)
                }
                dt if dt.contains("DECIMAL") || dt.contains("NUMERIC") => DataType::Float64, // Simplified
                _ => DataType::Utf8, // Default to string for VARCHAR, TEXT, etc.
            };

            Field::new(&col.name, data_type, col.nullable)
        })
        .collect();

    let schema = Schema::from(fields);
    debug!("Created Arrow schema with {} fields", schema.fields.len());

    // Convert JSON rows to Arrow arrays
    let arrays: Result<Vec<Box<dyn Array>>, _> = schema
        .fields
        .iter()
        .enumerate()
        .map(|(col_idx, field)| {
            convert_column_to_arrow(
                &query_result.rows,
                &field.name,
                col_idx,
                &field.data_type,
                field.is_nullable,
            )
        })
        .collect();

    let arrays = arrays?;

    // Create chunk (RecordBatch equivalent in arrow2)
    let chunk = Chunk::new(arrays);
    debug!("Created Arrow chunk with {} rows", chunk.len());

    // Write to Parquet file with optimal settings
    let file = File::create(parquet_path).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FILE_CREATE_ERROR",
            &format!("Failed to create Parquet file: {}", e),
        )
    })?;

    let writer = BufWriter::new(file);

    // Configure Parquet write options for performance
    let options = WriteOptions {
        write_statistics: true,
        compression: CompressionOptions::Snappy, // Fast compression
        version: Version::V2,
        data_pagesize_limit: None,
    };

    // Set up encodings (use plain encoding for all columns for compatibility)
    let encodings: Vec<Vec<Encoding>> = schema
        .fields
        .iter()
        .map(|_field| vec![Encoding::Plain])
        .collect();

    let row_groups =
        RowGroupIterator::try_new(vec![Ok(chunk)].into_iter(), &schema, options, encodings)
            .map_err(|e| {
                create_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "PARQUET_ENCODING_ERROR",
                    &format!("Failed to create row group: {}", e),
                )
            })?;

    let mut writer = FileWriter::try_new(writer, schema, options).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PARQUET_WRITER_ERROR",
            &format!("Failed to create Parquet writer: {}", e),
        )
    })?;

    for group in row_groups {
        writer
            .write(group.map_err(|e| {
                create_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "PARQUET_WRITE_ERROR",
                    &format!("Failed to write row group: {}", e),
                )
            })?)
            .map_err(|e| {
                create_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "PARQUET_WRITE_ERROR",
                    &format!("Failed to write row group: {}", e),
                )
            })?;
    }

    let _size = writer.end(None).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PARQUET_FINALIZE_ERROR",
            &format!("Failed to finalize Parquet file: {}", e),
        )
    })?;

    // Get actual file size from metadata
    let metadata = std::fs::metadata(parquet_path).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "FILE_STAT_ERROR",
            &format!("Failed to get file size: {}", e),
        )
    })?;

    let file_size = metadata.len();
    info!(
        "✅ Wrote Parquet file: {} bytes, {} rows",
        file_size, query_result.row_count
    );

    Ok(file_size)
}

/// Convert a single column from JSON values to Arrow array
fn convert_column_to_arrow(
    rows: &[serde_json::Value],
    column_name: &str,
    col_idx: usize,
    data_type: &DataType,
    nullable: bool,
) -> Result<Box<dyn Array>, (StatusCode, Json<ImportErrorResponse>)> {
    use serde_json::Value;

    match data_type {
        DataType::Int64 => {
            let values: Vec<Option<i64>> = rows
                .iter()
                .map(|row| match &row[column_name] {
                    Value::Null => None,
                    Value::Number(n) => n.as_i64(),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                })
                .collect();
            Ok(Box::new(PrimitiveArray::<i64>::from(values)) as Box<dyn Array>)
        }
        DataType::Float64 => {
            let values: Vec<Option<f64>> = rows
                .iter()
                .map(|row| match &row[column_name] {
                    Value::Null => None,
                    Value::Number(n) => n.as_f64(),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                })
                .collect();
            Ok(Box::new(PrimitiveArray::<f64>::from(values)) as Box<dyn Array>)
        }
        DataType::Boolean => {
            let values: Vec<Option<bool>> = rows
                .iter()
                .map(|row| match &row[column_name] {
                    Value::Null => None,
                    Value::Bool(b) => Some(*b),
                    Value::Number(n) => Some(n.as_i64().unwrap_or(0) != 0),
                    Value::String(s) => match s.to_lowercase().as_str() {
                        "true" | "t" | "yes" | "y" | "1" => Some(true),
                        "false" | "f" | "no" | "n" | "0" => Some(false),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            Ok(Box::new(BooleanArray::from(values)) as Box<dyn Array>)
        }
        DataType::Utf8 => {
            let values: Vec<Option<String>> = rows
                .iter()
                .map(|row| match &row[column_name] {
                    Value::Null => None,
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    Value::Array(_) | Value::Object(_) => Some(row[column_name].to_string()),
                })
                .collect();
            Ok(Box::new(Utf8Array::<i32>::from(values)) as Box<dyn Array>)
        }
        DataType::Date32 => {
            let values: Vec<Option<i32>> = rows
                .iter()
                .map(|row| {
                    match &row[column_name] {
                        Value::Null => None,
                        Value::String(s) => {
                            // Parse date string to days since epoch
                            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .ok()
                                .map(|d| {
                                    d.signed_duration_since(
                                        chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
                                    )
                                    .num_days() as i32
                                })
                        }
                        _ => None,
                    }
                })
                .collect();
            Ok(Box::new(PrimitiveArray::<i32>::from(values)) as Box<dyn Array>)
        }
        DataType::Timestamp(unit, _) => {
            let values: Vec<Option<i64>> = rows
                .iter()
                .map(|row| {
                    match &row[column_name] {
                        Value::Null => None,
                        Value::String(s) => {
                            // Parse ISO 8601 timestamp
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|dt| dt.timestamp_millis())
                        }
                        Value::Number(n) => n.as_i64(), // Assume already in milliseconds
                        _ => None,
                    }
                })
                .collect();
            Ok(Box::new(PrimitiveArray::<i64>::from(values)) as Box<dyn Array>)
        }
        _ => {
            // Fallback to string representation for unsupported types
            let values: Vec<Option<String>> = rows
                .iter()
                .map(|row| match &row[column_name] {
                    Value::Null => None,
                    v => Some(v.to_string()),
                })
                .collect();
            Ok(Box::new(Utf8Array::<i32>::from(values)) as Box<dyn Array>)
        }
    }
}

/// Generate a unique ID (simple timestamp-based for now)
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("{:x}", timestamp)
}

/// Helper to create error responses
pub fn create_error(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<ImportErrorResponse>) {
    (
        status,
        Json(ImportErrorResponse {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::api_types::ColumnDefinition as CatalogColumnDef;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_detect_file_format() {
        assert!(matches!(
            detect_file_format("test.csv"),
            Ok(FileFormat::Csv)
        ));
        assert!(matches!(
            detect_file_format("test.parquet"),
            Ok(FileFormat::Parquet)
        ));
        assert!(matches!(
            detect_file_format("test.jsonl"),
            Ok(FileFormat::JsonLines)
        ));
        assert!(matches!(
            detect_file_format("test.json"),
            Ok(FileFormat::JsonArray)
        ));
        assert!(detect_file_format("test.txt").is_err());
    }

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        // Sleep for 2ms to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = generate_id();
        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
    }

    #[test]
    fn test_write_query_result_to_parquet_integer_column() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_int.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "value": 100}),
                json!({"id": 2, "value": 200}),
                json!({"id": 3, "value": 300}),
            ],
            row_count: 3,
            execution_time_ms: 10,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "value".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(result.is_ok(), "Parquet write failed: {:?}", result.err());

        let file_size = result.unwrap();
        assert!(file_size > 0, "File size should be greater than 0");
        assert!(parquet_path.exists(), "Parquet file should exist");
    }

    #[test]
    fn test_write_query_result_to_parquet_mixed_types() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_mixed.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "name": "Alice", "score": 95.5, "active": true}),
                json!({"id": 2, "name": "Bob", "score": 87.3, "active": false}),
                json!({"id": 3, "name": "Charlie", "score": 92.1, "active": true}),
            ],
            row_count: 3,
            execution_time_ms: 15,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "score".to_string(),
                    data_type: "DOUBLE".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "active".to_string(),
                    data_type: "BOOLEAN".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(result.is_ok(), "Parquet write failed: {:?}", result.err());

        let file_size = result.unwrap();
        assert!(file_size > 0, "File size should be greater than 0");
        assert!(parquet_path.exists(), "Parquet file should exist");
    }

    #[test]
    fn test_write_query_result_to_parquet_null_values() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_nulls.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "value": 100}),
                json!({"id": 2, "value": null}),
                json!({"id": 3, "value": 300}),
            ],
            row_count: 3,
            execution_time_ms: 10,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "value".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(
            result.is_ok(),
            "Parquet write with nulls failed: {:?}",
            result.err()
        );

        let file_size = result.unwrap();
        assert!(file_size > 0, "File size should be greater than 0");
    }

    #[test]
    fn test_write_query_result_to_parquet_large_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_large.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        // Create 10,000 rows
        let rows: Vec<serde_json::Value> = (0..10000)
            .map(|i| json!({"id": i, "name": format!("User {}", i), "score": (i % 100) as f64}))
            .collect();

        let query_result = QueryResult {
            rows,
            row_count: 10000,
            execution_time_ms: 500,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "score".to_string(),
                    data_type: "DOUBLE".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let start = std::time::Instant::now();
        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        let duration = start.elapsed();

        assert!(
            result.is_ok(),
            "Large dataset write failed: {:?}",
            result.err()
        );
        println!("Wrote 10,000 rows in {:?}", duration);

        let file_size = result.unwrap();
        assert!(file_size > 0, "File size should be greater than 0");

        // Snappy compression should make the file reasonably small
        assert!(
            file_size < 500_000,
            "File size should be compressed (< 500KB)"
        );
    }

    #[test]
    fn test_write_query_result_to_parquet_date_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_dates.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![
                json!({
                    "id": 1,
                    "birth_date": "1990-01-15",
                    "created_at": "2024-01-15T10:30:00Z"
                }),
                json!({
                    "id": 2,
                    "birth_date": "1985-05-20",
                    "created_at": "2024-02-20T15:45:00Z"
                }),
            ],
            row_count: 2,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "birth_date".to_string(),
                    data_type: "DATE".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "created_at".to_string(),
                    data_type: "TIMESTAMP".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(
            result.is_ok(),
            "Date/timestamp write failed: {:?}",
            result.err()
        );

        let file_size = result.unwrap();
        assert!(file_size > 0, "File size should be greater than 0");
    }

    #[test]
    fn test_write_query_result_to_parquet_missing_columns() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_missing_cols.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![json!({"id": 1})],
            row_count: 1,
            execution_time_ms: 5,
            truncated: false,
            columns: None, // Missing column definitions
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(result.is_err(), "Should fail with missing columns");

        let err = result.unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_write_query_result_to_parquet_empty_columns() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("test_empty_cols.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![],
            row_count: 0,
            execution_time_ms: 1,
            truncated: false,
            columns: Some(vec![]), // Empty column list
        };

        let result = write_query_result_to_parquet(&query_result, parquet_path_str);
        assert!(result.is_err(), "Should fail with empty columns");

        let err = result.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_convert_column_to_arrow_integer() {
        let rows = vec![
            json!({"value": 100}),
            json!({"value": 200}),
            json!({"value": null}),
        ];

        let result = convert_column_to_arrow(&rows, "value", 0, &DataType::Int64, true);
        assert!(result.is_ok());

        let array = result.unwrap();
        assert_eq!(array.len(), 3);
        assert_eq!(array.null_count(), 1);
    }

    #[test]
    fn test_convert_column_to_arrow_string() {
        let rows = vec![
            json!({"name": "Alice"}),
            json!({"name": "Bob"}),
            json!({"name": null}),
        ];

        let result = convert_column_to_arrow(&rows, "name", 0, &DataType::Utf8, true);
        assert!(result.is_ok());

        let array = result.unwrap();
        assert_eq!(array.len(), 3);
        assert_eq!(array.null_count(), 1);
    }

    #[test]
    fn test_convert_column_to_arrow_boolean() {
        let rows = vec![
            json!({"active": true}),
            json!({"active": false}),
            json!({"active": "yes"}),
            json!({"active": "no"}),
            json!({"active": null}),
        ];

        let result = convert_column_to_arrow(&rows, "active", 0, &DataType::Boolean, true);
        assert!(result.is_ok());

        let array = result.unwrap();
        assert_eq!(array.len(), 5);
        assert_eq!(array.null_count(), 1);
    }

    #[test]
    fn test_build_datasource_lineage_turtle_includes_storage_and_schema() {
        let schema = SchemaDefinition {
            primary_key: None,
            columns: vec![ColumnDefinition {
                name: "customer_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
            }],
        };
        let lineage = ImportLineage {
            import_method: "datasource_query".to_string(),
            source_file: "source_1:customers".to_string(),
            imported_by: "user@example.com".to_string(),
            imported_at: "2026-03-09T12:00:00Z".to_string(),
            import_id: "import_123".to_string(),
        };

        let turtle = build_datasource_lineage_turtle(
            "ds_datasource_123",
            "import_123",
            "Customers",
            42,
            &lineage,
            &schema,
            "source_1",
            "customers",
            Some("customer_id > 10"),
            "parquet",
            "/tmp/ds_datasource_123.parquet",
            1024,
        );

        assert!(turtle.contains("gph:storageFormat \"parquet\""));
        assert!(turtle.contains("gph:storagePath \"/tmp/ds_datasource_123.parquet\""));
        assert!(turtle.contains("gph:fileSizeBytes 1024"));
        assert!(turtle.contains("gph:sourceDataSource \"source_1\""));
        assert!(turtle.contains("gph:hasColumn"));
        assert!(turtle.contains("gph:columnName \"customer_id\""));
    }
}
