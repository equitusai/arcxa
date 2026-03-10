//! Data Source Catalog API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::api::ApiState;
use graphica_core::catalog::{
    CatalogErrorResponse, ConnectionTestResult, CreateDataSourceRequest, Credentials, DataSource,
    DataSourceResponse, DataSourceStatus, ExecuteQueryRequest, InferSchemaRequest,
    ListDataSourcesRequest, ListDataSourcesResponse, QueryResult, SchemaDefinition,
    TestConnectionRequest, UpdateDataSourcePatch, UpdateDataSourceRequest,
};
use graphica_core::errors::GraphicaError;
use graphica_core::secrets::SecretValue;

// ============================================================================
// Data Source CRUD Handlers
// ============================================================================

/// Register a new data source
#[utoipa::path(
    post,
    path = "/api/v1/datasources",
    request_body = CreateDataSourceRequest,
    responses(
        (status = 200, description = "Datasource registered successfully", body = DataSourceResponse),
        (status = 400, description = "Invalid request - validation failed", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - failed to register datasource", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn register_datasource(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateDataSourceRequest>,
) -> Result<Json<DataSourceResponse>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    // Parse connection config into DataSource type
    let connection_details: graphica_core::catalog::ConnectionDetails =
        serde_json::from_value(request.connection.clone()).map_err(|e| {
            create_error(
                StatusCode::BAD_REQUEST,
                "INVALID_CONNECTION",
                &format!("Invalid connection config: {}", e),
            )
        })?;

    validate_source_type_matches_connection(&request.source_type, &connection_details)?;

    let source = DataSource::new(
        request.title.clone(),
        request.source_type.clone(),
        connection_details,
    );

    // Apply optional fields
    let mut source = source;
    source.description = request.description;
    source.schema_ref = request.schema_ref;
    source.tags = request.tags;
    source.metadata = request.metadata;

    // Validate configuration
    source.validate().map_err(|errors| {
        create_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            &errors.join(", "),
        )
    })?;

    // Register in catalog
    let response = catalog.register_source(source).await.map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "REGISTRATION_FAILED",
            &format!("Failed to register data source: {}", e),
        )
    })?;

    Ok(Json(redact_datasource_response(response)))
}

/// List all data sources with optional filtering
#[utoipa::path(
    get,
    path = "/api/v1/datasources",
    params(
        ("sourceType" = Option<String>, Query, description = "Filter by source type"),
        ("tags" = Option<Vec<String>>, Query, description = "Filter by tags"),
        ("status" = Option<DataSourceStatus>, Query, description = "Filter by status"),
        ("page" = Option<usize>, Query, description = "Page number (0-indexed)"),
        ("pageSize" = Option<usize>, Query, description = "Page size (default: 50)"),
    ),
    responses(
        (status = 200, description = "List of datasources", body = ListDataSourcesResponse),
        (status = 500, description = "Internal error", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn list_datasources(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ListDataSourcesRequest>,
) -> Result<Json<ListDataSourcesResponse>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let response = catalog.list_sources(&params).await.map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "LIST_FAILED",
            &format!("Failed to list data sources: {}", e),
        )
    })?;

    Ok(Json(redact_list_response(response)))
}

/// Get a specific data source by ID
#[utoipa::path(
    get,
    path = "/api/v1/datasources/{id}",
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 200, description = "Data source retrieved successfully", body = DataSourceResponse),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - failed to retrieve data source", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn get_datasource(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<Json<DataSourceResponse>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let response = catalog.get_source(&source_id).await.map_err(|e| {
        let status = if e.to_string().contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        create_error(
            status,
            "GET_FAILED",
            &format!("Failed to get data source: {}", e),
        )
    })?;

    Ok(Json(redact_datasource_response(response)))
}

/// Update an existing data source
#[utoipa::path(
    put,
    path = "/api/v1/datasources/{id}",
    request_body = UpdateDataSourceRequest,
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 200, description = "Data source updated successfully", body = DataSourceResponse),
        (status = 400, description = "Invalid request - validation failed", body = CatalogErrorResponse),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - failed to update data source", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn update_datasource(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
    Json(request): Json<UpdateDataSourceRequest>,
) -> Result<Json<DataSourceResponse>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let connection = if let Some(connection) = request.connection {
        let details: graphica_core::catalog::ConnectionDetails = serde_json::from_value(connection)
            .map_err(|e| {
                create_error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_CONNECTION",
                    &format!("Invalid connection config: {}", e),
                )
            })?;

        if let Some(source_type) = request.source_type.as_deref() {
            validate_source_type_matches_connection(source_type, &details)?;
        }

        Some(details)
    } else {
        None
    };

    let updates = UpdateDataSourcePatch {
        title: request.title,
        description: request.description,
        source_type: request.source_type,
        connection,
        schema_ref: request.schema_ref,
        tags: request.tags,
        metadata: request.metadata,
    };

    let response = catalog
        .update_source(&source_id, updates)
        .await
        .map_err(|e| map_catalog_error("UPDATE_FAILED", "Failed to update data source", e))?;

    Ok(Json(redact_datasource_response(response)))
}

/// Delete a data source
#[utoipa::path(
    delete,
    path = "/api/v1/datasources/{id}",
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 204, description = "Data source deleted successfully"),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - failed to delete data source", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn delete_datasource(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    catalog.delete_source(&source_id).await.map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DELETE_FAILED",
            &format!("Failed to delete data source: {}", e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Data Source Operations Handlers
// ============================================================================

/// Test connection to a data source
#[utoipa::path(
    post,
    path = "/api/v1/datasources/test",
    request_body = TestConnectionRequest,
    responses(
        (status = 200, description = "Connection test result", body = ConnectionTestResult),
        (status = 400, description = "Invalid request - missing parameters or invalid connection config", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - connection test failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn test_connection(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<TestConnectionRequest>,
) -> Result<Json<ConnectionTestResult>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    // If source_id provided, test that source
    if let Some(source_id) = request.source_id {
        let result = catalog.test_connection(&source_id).await.map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "TEST_FAILED",
                &format!("Connection test failed: {}", e),
            )
        })?;

        return Ok(Json(result));
    }

    // Otherwise, test inline connection config
    if let (Some(connection_json), Some(source_type)) = (request.connection, request.source_type) {
        // Parse connection details
        use graphica_core::catalog::Credentials;

        let connection_details: graphica_core::catalog::ConnectionDetails =
            serde_json::from_value(connection_json).map_err(|e| {
                create_error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_CONNECTION",
                    &format!("Invalid connection config: {}", e),
                )
            })?;

        validate_source_type_matches_connection(&source_type, &connection_details)?;

        // Create a temporary DataSource for testing
        let temp_source = DataSource::new(
            "Temporary Test Source".to_string(),
            source_type.clone(),
            connection_details.clone(),
        );

        // Validate configuration
        temp_source.validate().map_err(|errors| {
            create_error(
                StatusCode::BAD_REQUEST,
                "VALIDATION_FAILED",
                &errors.join(", "),
            )
        })?;

        // Test the connection inline (using ConnectorRegistry)
        use graphica_core::catalog::ConnectorRegistry;
        use std::time::Instant;

        let registry = ConnectorRegistry::default();
        let connector = registry
            .get_connector(&temp_source.connection.config)
            .ok_or_else(|| {
                create_error(
                    StatusCode::BAD_REQUEST,
                    "UNSUPPORTED_SOURCE_TYPE",
                    &format!("Source type '{}' is not supported", source_type),
                )
            })?;

        let start = Instant::now();

        // Resolve credentials from inline config or secret store
        let credentials =
            resolve_inline_credentials(&connection_details, state.secret_store_registry.as_ref())
                .await?;

        match connector.test_connection(&temp_source, credentials).await {
            Ok(result) => {
                // Return the result from connector (already has correct format)
                Ok(Json(result))
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;

                Ok(Json(ConnectionTestResult {
                    success: false,
                    duration_ms,
                    error: Some(format!("Connection failed: {}", e)),
                    metadata: std::collections::HashMap::new(),
                    tested_at: chrono::Utc::now(),
                }))
            }
        }
    } else {
        Err(create_error(
            StatusCode::BAD_REQUEST,
            "MISSING_PARAMETERS",
            "Either 'sourceId' or both 'connection' and 'sourceType' must be provided",
        ))
    }
}

/// Infer schema from a data source
#[utoipa::path(
    post,
    path = "/api/v1/datasources/{id}/schema/infer",
    request_body = InferSchemaRequest,
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 200, description = "Schema inferred successfully", body = SchemaDefinition),
        (status = 400, description = "Invalid request - invalid parameters", body = CatalogErrorResponse),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - schema inference failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn infer_schema(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
    Json(request): Json<InferSchemaRequest>,
) -> Result<Json<SchemaDefinition>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let schema = catalog
        .infer_schema(
            &source_id,
            request.table_name.as_deref(),
            request.sample_size,
        )
        .await
        .map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SCHEMA_INFERENCE_FAILED",
                &format!("Schema inference failed: {}", e),
            )
        })?;

    Ok(Json(schema))
}

/// Enhanced schema inference with semantic types and RDF storage
#[utoipa::path(
    post,
    path = "/api/v1/datasources/{id}/schema/infer-enhanced",
    request_body = InferSchemaRequest,
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 200, description = "Enhanced schema inferred successfully with semantic types and RDF triples", body = serde_json::Value),
        (status = 400, description = "Invalid request - invalid parameters", body = CatalogErrorResponse),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - enhanced schema inference failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn infer_schema_enhanced(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
    Json(request): Json<InferSchemaRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<CatalogErrorResponse>)> {
    use graphica_core::catalog::connectors::enhanced_inference::ColumnInferenceEngine;
    use graphica_core::catalog::schema_to_rdf::SchemaRdfConverter;

    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    // Step 1: Basic schema inference
    let mut schema = catalog
        .infer_schema(
            &source_id,
            request.table_name.as_deref(),
            request.sample_size,
        )
        .await
        .map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SCHEMA_INFERENCE_FAILED",
                &format!("Schema inference failed: {}", e),
            )
        })?;

    // Step 2: Enhance with semantic types and statistics
    // TODO: This is a simplified version - in production, would:
    // - Query pg_stats for PostgreSQL sources
    // - Run semantic detection on sample data
    // - Enrich each column

    // For now, run basic semantic detection on column names
    for table in &mut schema.tables {
        let engine = ColumnInferenceEngine::new(&schema.name, &table.name);

        for column in &mut table.columns {
            // Run semantic detection on column name only (no samples for now)
            let enriched = engine
                .enrich_column(
                    column.clone(),
                    None,   // No pg_stats row
                    vec![], // No sample values
                    table.estimated_rows,
                )
                .await
                .map_err(|e| {
                    create_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ENRICHMENT_FAILED",
                        &format!("Column enrichment failed: {}", e),
                    )
                })?;

            *column = enriched;
        }
    }

    // Step 3: Convert to RDF triples
    let converter = SchemaRdfConverter::new(&source_id);
    let triples = converter.convert_schema(&schema).map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "RDF_CONVERSION_FAILED",
            &format!("RDF conversion failed: {}", e),
        )
    })?;

    // Step 4: Generate Turtle representation
    let turtle = SchemaRdfConverter::triples_to_turtle(&triples);

    // Step 5: Store in governance brain (if available)
    let mut stored_count = 0u64;
    let mut storage_error = None;

    if let Some(rdf_storage) = &state.rdf_storage {
        tracing::info!(
            "Storing {} RDF triples for schema inference of source {}",
            triples.len(),
            source_id
        );

        let mut storage_client = rdf_storage.lock().await;

        match storage_client
            .store_schema_triples(&source_id, triples.clone())
            .await
        {
            Ok(count) => {
                stored_count = count;
                tracing::info!(
                    "Successfully stored {} triples for source {} in governance brain",
                    count,
                    source_id
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to store triples in governance brain for source {}: {}",
                    source_id,
                    e
                );
                storage_error = Some(format!("Storage warning: {}", e));
            }
        }
    } else {
        tracing::info!(
            "RDF storage not configured, generated {} triples but not storing",
            triples.len()
        );
        storage_error = Some("RDF storage not configured".to_string());
    }

    // Return enhanced schema with RDF representation
    Ok(Json(serde_json::json!({
        "schema": schema,
        "rdf_triples_count": triples.len(),
        "rdf_triples_stored": stored_count,
        "rdf_turtle": turtle,
        "semantic_types_detected": schema.tables.iter()
            .flat_map(|t| &t.columns)
            .filter(|c| c.semantic_type.is_some())
            .count(),
        "storage_status": if storage_error.is_none() { "success" } else { "warning" },
        "storage_message": storage_error.unwrap_or_else(|| format!("Stored {} triples successfully", stored_count))
    })))
}

/// Execute a query against a data source
#[utoipa::path(
    post,
    path = "/api/v1/datasources/{id}/query",
    request_body = ExecuteQueryRequest,
    params(
        ("id" = String, Path, description = "Data source unique identifier"),
    ),
    responses(
        (status = 200, description = "Query executed successfully", body = QueryResult),
        (status = 400, description = "Invalid request - invalid query or parameters", body = CatalogErrorResponse),
        (status = 404, description = "Data source not found", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - query execution failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn execute_query(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
    Json(request): Json<ExecuteQueryRequest>,
) -> Result<Json<QueryResult>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let result = catalog
        .execute_query(
            &source_id,
            &request.query,
            request.parameters,
            request.limit,
        )
        .await
        .map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "QUERY_FAILED",
                &format!("Query execution failed: {}", e),
            )
        })?;

    Ok(Json(result))
}

/// Search data sources by text
#[utoipa::path(
    get,
    path = "/api/v1/datasources/search",
    params(
        ("q" = String, Query, description = "Search query string"),
        ("limit" = Option<usize>, Query, description = "Maximum number of results (default: 50)"),
    ),
    responses(
        (status = 200, description = "Search results", body = Vec<DataSourceResponse>),
        (status = 400, description = "Invalid request - missing or invalid search query", body = CatalogErrorResponse),
        (status = 500, description = "Internal error - search failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn search_datasources(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<SearchRequest>,
) -> Result<Json<Vec<DataSourceResponse>>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available",
        )
    })?;

    let results = catalog
        .search_sources(&params.q, params.limit.unwrap_or(50))
        .await
        .map_err(|e| {
            create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SEARCH_FAILED",
                &format!("Search failed: {}", e),
            )
        })?;

    Ok(Json(
        results
            .into_iter()
            .map(redact_datasource_response)
            .collect(),
    ))
}

// ============================================================================
// Helper Types and Functions
// ============================================================================

/// Search query parameters
#[derive(serde::Deserialize)]
pub struct SearchRequest {
    /// Search query
    pub q: String,
    /// Maximum results
    pub limit: Option<usize>,
}

const REDACT_KEYS: &[&str] = &[
    "username",
    "password",
    "user",
    "pass",
    "access_key_id",
    "secret_access_key",
    "token",
    "access_token",
    "session_token",
    "api_key",
    "apikey",
    "client_id",
    "client_secret",
];

fn redact_list_response(mut response: ListDataSourcesResponse) -> ListDataSourcesResponse {
    response.sources = response
        .sources
        .into_iter()
        .map(redact_datasource_response)
        .collect();
    response
}

fn redact_datasource_response(mut response: DataSourceResponse) -> DataSourceResponse {
    response.source.connection.credentials.clear();
    redact_metadata(&mut response.source.metadata);
    response
}

fn validate_source_type_matches_connection(
    source_type: &str,
    connection: &graphica_core::catalog::ConnectionDetails,
) -> Result<(), (StatusCode, Json<CatalogErrorResponse>)> {
    if !connection.config.matches_source_type_name(source_type) {
        return Err(create_error(
            StatusCode::BAD_REQUEST,
            "SOURCE_TYPE_MISMATCH",
            &format!(
                "sourceType '{}' does not match connection type '{}'",
                source_type,
                connection.config.source_type()
            ),
        ));
    }

    Ok(())
}

fn map_catalog_error(
    code: &str,
    message: &str,
    error: GraphicaError,
) -> (StatusCode, Json<CatalogErrorResponse>) {
    let status = match error {
        GraphicaError::Configuration(_) => StatusCode::BAD_REQUEST,
        GraphicaError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    create_error(status, code, &format!("{}: {}", message, error))
}

fn redact_metadata(metadata: &mut std::collections::HashMap<String, String>) {
    let keys: Vec<String> = metadata
        .keys()
        .filter(|key| {
            let lower = key.to_lowercase();
            REDACT_KEYS.contains(&lower.as_str())
        })
        .cloned()
        .collect();

    for key in keys {
        metadata.remove(&key);
    }
}

async fn resolve_inline_credentials(
    connection: &graphica_core::catalog::ConnectionDetails,
    registry: Option<&Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
) -> Result<Credentials, (StatusCode, Json<CatalogErrorResponse>)> {
    if !connection.credentials.is_empty() {
        return credentials_from_map(&connection.credentials, "connection.credentials");
    }

    if !connection.secret_ref.trim().is_empty() {
        let registry = registry.ok_or_else(|| {
            create_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "SECRET_STORE_UNAVAILABLE",
                "Secret store registry not configured",
            )
        })?;
        let store = registry
            .default()
            .or_else(|| registry.get("default"))
            .ok_or_else(|| {
                create_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "SECRET_STORE_UNAVAILABLE",
                    "No default secret store configured",
                )
            })?;

        let secret = store
            .get_secret(&connection.secret_ref, None)
            .await
            .map_err(|e| {
                create_error(
                    StatusCode::BAD_REQUEST,
                    "SECRET_RESOLUTION_FAILED",
                    &format!(
                        "Failed to resolve secretRef '{}': {}",
                        connection.secret_ref, e
                    ),
                )
            })?;

        return credentials_from_secret_value(&secret.value);
    }

    Err(create_error(
        StatusCode::BAD_REQUEST,
        "MISSING_CREDENTIALS",
        "Connection credentials or secretRef required for test_connection",
    ))
}

fn credentials_from_secret_value(
    value: &SecretValue,
) -> Result<Credentials, (StatusCode, Json<CatalogErrorResponse>)> {
    match value {
        SecretValue::KeyValue(map) => credentials_from_map(map, "secret value"),
        SecretValue::String(raw) => credentials_from_json_str(raw),
        SecretValue::Json(json) => credentials_from_json_value(json),
        SecretValue::Binary(_) => Err(create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SECRET_VALUE",
            "Binary secret values are not supported for datasource credentials",
        )),
    }
}

fn credentials_from_json_str(
    raw: &str,
) -> Result<Credentials, (StatusCode, Json<CatalogErrorResponse>)> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SECRET_VALUE",
            &format!("Failed to parse secret JSON credentials: {}", e),
        )
    })?;
    credentials_from_json_value(&value)
}

fn credentials_from_json_value(
    value: &serde_json::Value,
) -> Result<Credentials, (StatusCode, Json<CatalogErrorResponse>)> {
    let obj = value.as_object().ok_or_else(|| {
        create_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SECRET_VALUE",
            "Credentials JSON must be an object",
        )
    })?;

    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_string());
        } else {
            map.insert(k.clone(), v.to_string());
        }
    }

    credentials_from_map(&map, "credentials JSON")
}

fn credentials_from_map(
    map: &std::collections::HashMap<String, String>,
    context: &str,
) -> Result<Credentials, (StatusCode, Json<CatalogErrorResponse>)> {
    let (username, password) =
        if let (Some(user), Some(pass)) = (map.get("username"), map.get("password")) {
            (user.to_string(), pass.to_string())
        } else if let (Some(user), Some(pass)) = (map.get("user"), map.get("pass")) {
            (user.to_string(), pass.to_string())
        } else {
            return Err(create_error(
                StatusCode::BAD_REQUEST,
                "MISSING_CREDENTIALS",
                &format!(
                    "Missing credentials in {} (expected username/password or user/pass)",
                    context
                ),
            ));
        };

    let mut credentials = Credentials::new(username, password);
    for (k, v) in map {
        if matches!(k.as_str(), "username" | "password" | "user" | "pass") {
            continue;
        }
        credentials.additional.insert(k.clone(), v.clone());
    }

    Ok(credentials)
}

/// Sync all datasources to RDF (Admin only - for backfill)
#[utoipa::path(
    post,
    path = "/api/v1/admin/datasources/sync-to-rdf",
    responses(
        (status = 200, description = "Datasources synced to RDF governance layer successfully", body = serde_json::Value),
        (status = 500, description = "Internal error - sync to RDF failed", body = CatalogErrorResponse),
    ),
    tag = "datasources"
)]
pub async fn sync_datasources_to_rdf(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<CatalogErrorResponse>)> {
    let catalog = state.datasource_catalog_impl.as_ref().ok_or_else(|| {
        create_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "CATALOG_UNAVAILABLE",
            "Data source catalog not available or does not support RDF sync",
        )
    })?;

    let synced_count = catalog.sync_all_to_rdf().await.map_err(|e| {
        create_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SYNC_FAILED",
            &format!("Failed to sync datasources to RDF: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "synced_count": synced_count,
        "message": format!("Successfully synced {} datasources to RDF governance layer", synced_count)
    })))
}

/// Helper to create error responses
fn create_error(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<CatalogErrorResponse>) {
    (
        status,
        Json(CatalogErrorResponse {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
            request_id: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::types::PostgreSQLConfig;
    use graphica_core::catalog::{ConnectionDetails, SourceConfig};

    #[test]
    fn test_create_error() {
        let (status, Json(error)) =
            create_error(StatusCode::BAD_REQUEST, "TEST_ERROR", "Test message");

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "TEST_ERROR");
        assert_eq!(error.message, "Test message");
    }

    #[test]
    fn test_validate_source_type_matches_connection_accepts_alias() {
        let connection = ConnectionDetails {
            secret_ref: "vault://postgres".to_string(),
            config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "app".to_string(),
                schema: None,
                ssl_mode: Some("require".to_string()),
            }),
            encryption_enabled: true,
            credentials: Default::default(),
        };

        let result = validate_source_type_matches_connection("postgres", &connection);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_source_type_matches_connection_rejects_mismatch() {
        let connection = ConnectionDetails {
            secret_ref: "vault://postgres".to_string(),
            config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "app".to_string(),
                schema: None,
                ssl_mode: Some("require".to_string()),
            }),
            encryption_enabled: true,
            credentials: Default::default(),
        };

        let error = validate_source_type_matches_connection("Oracle", &connection).unwrap_err();
        let (status, Json(body)) = error;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.code, "SOURCE_TYPE_MISMATCH");
        assert!(body.message.contains("does not match connection type"));
    }
}
