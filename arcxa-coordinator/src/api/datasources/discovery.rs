//! # Schema Discovery REST API
//!
//! Production-grade REST endpoints for asynchronous schema discovery with real-time progress tracking.
//!
//! ## Endpoints
//!
//! - `POST /api/v1/datasources/:id/discover` - Start async discovery
//! - `GET /api/v1/datasources/:id/discovery/progress` - Get current progress
//! - `GET /api/v1/datasources/:id/discovery/result` - Get final result
//! - `DELETE /api/v1/datasources/:id/discovery` - Cancel discovery
//!
//! ## Usage Example
//!
//! ```bash
//! # Start discovery
//! curl -X POST http://localhost:8080/api/v1/datasources/ds-123/discover \
//!   -H "Content-Type: application/json" \
//!   -d '{"schema_filter": "public", "sample_size": 1000}'
//!
//! # Response: {"discovery_id": "uuid-here"}
//!
//! # Check progress
//! curl http://localhost:8080/api/v1/datasources/ds-123/discovery/progress?discovery_id=uuid-here
//!
//! # Get result
//! curl http://localhost:8080/api/v1/datasources/ds-123/discovery/result?discovery_id=uuid-here
//! ```

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::api::ApiState;
use crate::common::datasource_readiness::{evaluate_datasource_readiness, DatasourceOperation};
use crate::mapping::discovery::{
    DiscoveredTable, DiscoveryConfig, DiscoveryOrchestrator, DiscoveryProgress,
    DiscoveryStateManager,
};
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;

/// Request to start schema discovery
#[derive(Debug, Clone, Deserialize)]
pub struct StartDiscoveryRequest {
    /// Schema filter (e.g., "public", "dbo")
    pub schema_filter: Option<String>,

    /// Table name filter (pattern)
    pub table_filter: Option<String>,

    /// Number of sample rows to extract per table
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_sample_size() -> usize {
    1000
}

fn default_cache_ttl() -> u64 {
    3600 // 1 hour
}

/// Response when starting discovery
#[derive(Debug, Clone, Serialize)]
pub struct StartDiscoveryResponse {
    /// Unique discovery ID for tracking progress
    pub discovery_id: String,

    /// Associated datasource ID
    pub datasource_id: String,

    /// Initial status
    pub status: String,

    /// Message
    pub message: String,
}

/// Query parameters for progress/result endpoints
#[derive(Debug, Deserialize)]
pub struct DiscoveryQueryParams {
    /// Discovery ID to query
    pub discovery_id: String,
}

/// Query parameters for paginated result endpoint
#[derive(Debug, Deserialize)]
pub struct DiscoveryResultQueryParams {
    /// Discovery ID to query
    pub discovery_id: String,

    /// Number of tables to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Number of tables to skip (default: 0)
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

/// Paginated discovery result response
#[derive(Debug, Serialize)]
pub struct PaginatedDiscoveryResultResponse {
    /// Discovery ID
    pub discovery_id: String,

    /// Paginated tables
    pub tables: Vec<DiscoveredTable>,

    /// Total number of tables discovered
    pub total: usize,

    /// Current page number (calculated from offset/limit)
    pub page: usize,

    /// Number of tables in this response
    pub page_size: usize,

    /// Timestamp when discovery was cached
    pub cached_at: String,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub error: String,
    pub details: Option<String>,
}

impl ErrorResponse {
    fn new(error: impl Into<String>) -> Self {
        Self {
            code: None,
            error: error.into(),
            details: None,
        }
    }

    fn with_details(error: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            code: None,
            error: error.into(),
            details: Some(details.into()),
        }
    }

    fn with_code(
        code: impl Into<String>,
        error: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            code: Some(code.into()),
            error: error.into(),
            details: Some(details.into()),
        }
    }
}

/// POST /api/v1/datasources/:id/discover
///
/// Start asynchronous schema discovery for a data source.
/// Returns immediately with a discovery_id for tracking progress.
pub async fn start_discovery(
    State(state): State<Arc<ApiState>>,
    Path(datasource_id): Path<String>,
    Json(request): Json<StartDiscoveryRequest>,
) -> Result<Json<StartDiscoveryResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        datasource_id = %datasource_id,
        schema_filter = ?request.schema_filter,
        sample_size = request.sample_size,
        "Starting schema discovery"
    );

    // Get discovery state manager
    let state_manager = state.discovery_state.as_ref().ok_or_else(|| {
        error!("Discovery state manager not initialized");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Discovery service not available")),
        )
    })?;

    // Get discovery orchestrator
    let orchestrator = state
        .discovery_orchestrator
        .as_ref()
        .ok_or_else(|| {
            error!("Discovery orchestrator not initialized");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Discovery service not available")),
            )
        })?
        .clone();

    // Get datasource catalog
    let catalog = state.datasource_catalog.as_ref().ok_or_else(|| {
        error!("Datasource catalog not initialized");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Datasource catalog not available")),
        )
    })?;

    // Fetch datasource configuration
    let datasource_response = catalog.get_source(&datasource_id).await.map_err(|e| {
        error!(datasource_id = %datasource_id, error = %e, "Failed to fetch datasource");
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::with_details(
                "Datasource not found",
                e.to_string(),
            )),
        )
    })?;

    evaluate_datasource_readiness(&datasource_response, DatasourceOperation::Discovery).map_err(
        |failure| {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse::with_code(
                    failure.code,
                    "Datasource not ready",
                    failure.message,
                )),
            )
        },
    )?;

    let datasource = datasource_response.source;

    // Get credentials from secret store
    let credentials = get_credentials(&state, &datasource).await.map_err(|e| {
        error!(datasource_id = %datasource_id, error = %e, "Failed to get credentials");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_details(
                "Failed to get credentials",
                e.to_string(),
            )),
        )
    })?;

    // Create discovery in state manager
    let discovery_id = state_manager.create_discovery(datasource_id.clone());

    // Build discovery config
    let config = DiscoveryConfig {
        schema_filter: request.schema_filter,
        table_filter: request.table_filter,
        sample_size: request.sample_size,
        cache_ttl_secs: request.cache_ttl_secs,
    };

    // Spawn async discovery task
    let state_manager_clone = state_manager.clone();
    let discovery_id_clone = discovery_id.clone();
    let discovery_id_for_error = discovery_id.clone(); // Clone for error logging
    let datasource_id_clone = datasource_id.clone();
    let rdf_store = state.rdf_store.clone();

    tokio::spawn(async move {
        if let Err(e) = run_discovery(
            orchestrator,
            state_manager_clone,
            discovery_id_clone,
            datasource,
            credentials,
            config,
            rdf_store,
        )
        .await
        {
            error!(
                datasource_id = %datasource_id_clone,
                discovery_id = %discovery_id_for_error,
                error = %e,
                "Discovery task failed"
            );
        }
    });

    // Mark discovery as running
    state_manager.start_discovery(&discovery_id).map_err(|e| {
        error!(discovery_id = %discovery_id, error = %e, "Failed to start discovery");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_details(
                "Failed to start discovery",
                e.to_string(),
            )),
        )
    })?;

    info!(
        datasource_id = %datasource_id,
        discovery_id = %discovery_id,
        "Discovery started successfully"
    );

    Ok(Json(StartDiscoveryResponse {
        discovery_id,
        datasource_id,
        status: "running".to_string(),
        message: "Discovery started successfully".to_string(),
    }))
}

/// GET /api/v1/datasources/:id/discovery/progress
///
/// Get current progress of a discovery operation.
pub async fn get_discovery_progress(
    State(state): State<Arc<ApiState>>,
    Path(_datasource_id): Path<String>,
    Query(params): Query<DiscoveryQueryParams>,
) -> Result<Json<DiscoveryProgress>, (StatusCode, Json<ErrorResponse>)> {
    let state_manager = state.discovery_state.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Discovery service not available")),
        )
    })?;

    let progress = state_manager
        .get_progress(&params.discovery_id)
        .ok_or_else(|| {
            warn!(discovery_id = %params.discovery_id, "Discovery not found");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Discovery not found")),
            )
        })?;

    Ok(Json(progress))
}

/// GET /api/v1/datasources/:id/discovery/result
///
/// Get final result of a completed discovery operation with pagination support.
///
/// **Pagination**: Returns 50 tables by default. Use `limit` and `offset` query params for custom pagination.
///
/// **Example**: `/api/v1/datasources/:id/discovery/result?discovery_id=abc&limit=100&offset=0`
pub async fn get_discovery_result(
    State(state): State<Arc<ApiState>>,
    Path(_datasource_id): Path<String>,
    Query(params): Query<DiscoveryResultQueryParams>,
) -> Result<Json<PaginatedDiscoveryResultResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state_manager = state.discovery_state.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Discovery service not available")),
        )
    })?;

    // Check if discovery is complete
    let progress = state_manager
        .get_progress(&params.discovery_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Discovery not found")),
            )
        })?;

    use crate::mapping::discovery::DiscoveryStatus;
    match progress.status {
        DiscoveryStatus::Completed => {
            // Get cached result
            let result = state_manager
                .get_result(&params.discovery_id)
                .ok_or_else(|| {
                    warn!(discovery_id = %params.discovery_id, "Discovery result not found (expired?)");
                    (
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::new("Discovery result not found or expired")),
                    )
                })?;

            // Apply pagination to tables
            let total = result.schema.tables.len();
            let tables: Vec<DiscoveredTable> = result
                .schema
                .tables
                .into_iter()
                .skip(params.offset)
                .take(params.limit)
                .collect();

            let page = if params.limit > 0 {
                params.offset / params.limit
            } else {
                0
            };
            let page_size = tables.len();

            Ok(Json(PaginatedDiscoveryResultResponse {
                discovery_id: params.discovery_id.clone(),
                tables,
                total,
                page,
                page_size,
                cached_at: chrono::DateTime::from_timestamp(result.cached_at.timestamp(), 0)
                    .unwrap_or_default()
                    .to_rfc3339(),
            }))
        }
        DiscoveryStatus::Failed => {
            let error_msg = progress.errors.join("; ");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::with_details("Discovery failed", error_msg)),
            ))
        }
        DiscoveryStatus::Cancelled => Err((
            StatusCode::GONE,
            Json(ErrorResponse::new("Discovery was cancelled")),
        )),
        _ => Err((
            StatusCode::ACCEPTED,
            Json(ErrorResponse::new(format!(
                "Discovery still in progress ({}%)",
                progress.percent_complete
            ))),
        )),
    }
}

/// DELETE /api/v1/datasources/:id/discovery
///
/// Cancel a running discovery operation.
pub async fn cancel_discovery(
    State(state): State<Arc<ApiState>>,
    Path(_datasource_id): Path<String>,
    Query(params): Query<DiscoveryQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let state_manager = state.discovery_state.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Discovery service not available")),
        )
    })?;

    state_manager
        .cancel_discovery(&params.discovery_id)
        .map_err(|e| {
            error!(discovery_id = %params.discovery_id, error = %e, "Failed to cancel discovery");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::with_details(
                    "Discovery not found",
                    e.to_string(),
                )),
            )
        })?;

    info!(discovery_id = %params.discovery_id, "Discovery cancelled");

    Ok(Json(serde_json::json!({
        "discovery_id": params.discovery_id,
        "status": "cancelled",
        "message": "Discovery cancelled successfully"
    })))
}

/// Helper: Get credentials for a datasource
async fn get_credentials(state: &ApiState, datasource: &DataSource) -> Result<Credentials> {
    // Get secret reference from connection details
    let secret_ref = &datasource.connection.secret_ref;

    // Fetch from secret store
    if let Some(secret_registry) = &state.secret_store_registry {
        // Get default store or first registered store
        let store = secret_registry
            .default()
            .or_else(|| secret_registry.get("default"))
            .ok_or_else(|| anyhow!("No secret store configured in registry"))?;

        // Fetch secret by path
        let secret = store
            .get_secret(secret_ref, None)
            .await
            .context("Failed to fetch credentials from secret store")?;

        // Parse credentials from secret value
        use graphica_core::secrets::SecretValue;
        let credentials = match &secret.value {
            SecretValue::KeyValue(map) => {
                // Direct key-value format
                let username = map
                    .get("username")
                    .ok_or_else(|| anyhow!("Missing 'username' in secret"))?
                    .clone();
                let password = map
                    .get("password")
                    .ok_or_else(|| anyhow!("Missing 'password' in secret"))?
                    .clone();

                let mut additional = HashMap::new();
                for (k, v) in map.iter() {
                    if k != "username" && k != "password" {
                        additional.insert(k.clone(), v.clone());
                    }
                }

                Credentials {
                    username,
                    password,
                    additional,
                }
            }
            SecretValue::String(s) | SecretValue::Json(serde_json::Value::String(s)) => {
                // JSON string format
                serde_json::from_str(s)
                    .context("Failed to parse credentials from JSON string in secret store")?
            }
            SecretValue::Json(json_value) => {
                // Direct JSON format
                serde_json::from_value(json_value.clone())
                    .context("Failed to parse credentials from JSON value in secret store")?
            }
            SecretValue::Binary(_) => {
                return Err(anyhow!(
                    "Binary secret format not supported for credentials"
                ));
            }
        };

        return Ok(credentials);
    }

    // If no secret registry configured, try parsing the secret_ref as inline JSON
    // This is a fallback for testing/development only
    warn!(
        secret_ref = %secret_ref,
        "No secret store configured, attempting to parse secret_ref as inline credentials"
    );

    let credentials: Credentials = serde_json::from_str(secret_ref)
        .context("Failed to parse inline credentials - no secret store configured")?;

    Ok(credentials)
}

/// Background task: Run discovery with progress updates
async fn run_discovery(
    orchestrator: Arc<DiscoveryOrchestrator>,
    state_manager: Arc<DiscoveryStateManager>,
    discovery_id: String,
    datasource: DataSource,
    credentials: Credentials,
    config: DiscoveryConfig,
    rdf_store: Option<Arc<crate::governance::rdf_store::GraphicaRdfStore>>,
) -> Result<()> {
    info!(
        discovery_id = %discovery_id,
        datasource_id = %datasource.id,
        "Running discovery task"
    );

    // Update progress callback
    let progress_callback = {
        let state_manager = state_manager.clone();
        let discovery_id = discovery_id.clone();
        move |step: String, tables_discovered: usize, total_tables: Option<usize>| {
            let _ = state_manager.update_progress(&discovery_id, |progress| {
                progress.current_step = step;
                progress.tables_discovered = tables_discovered;
                progress.total_tables = total_tables;
                progress.update_percent();
            });
        }
    };

    // Run discovery with progress updates
    match orchestrator
        .discover_schema_with_progress(&datasource, &credentials, config, progress_callback)
        .await
    {
        Ok(schema) => {
            // Materialize discovered schema into RDF datasets so it is visible via /datasets APIs.
            if let Some(ref rdf_store) = rdf_store {
                use crate::governance::rdf_store::RdfStore;

                let schema_definition = discovered_schema_to_definition(&schema);
                let dataset_columns_turtle = crate::catalog_to_dataset::schema_to_column_triples(
                    &datasource.id,
                    &schema_definition,
                );
                let table_datasets_turtle =
                    crate::catalog_to_dataset::schema_to_table_dataset_triples(
                        &datasource,
                        &schema_definition,
                    );

                if let Err(e) = rdf_store.load_turtle(&dataset_columns_turtle, None) {
                    warn!(
                        discovery_id = %discovery_id,
                        datasource_id = %datasource.id,
                        error = %e,
                        "Failed to sync discovered schema columns to RDF"
                    );
                }

                if let Err(e) = rdf_store.load_turtle(&table_datasets_turtle, None) {
                    warn!(
                        discovery_id = %discovery_id,
                        datasource_id = %datasource.id,
                        error = %e,
                        "Failed to sync discovered table datasets to RDF"
                    );
                }
            }

            state_manager.complete_discovery(&discovery_id, schema)?;
            info!(
                discovery_id = %discovery_id,
                "Discovery completed successfully"
            );
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Discovery failed: {}", e);
            state_manager.fail_discovery(&discovery_id, error_msg.clone())?;
            error!(
                discovery_id = %discovery_id,
                error = %e,
                "Discovery failed"
            );
            Err(e)
        }
    }
}

fn discovered_schema_to_definition(
    discovered: &crate::mapping::discovery::types::DiscoveredSchema,
) -> graphica_core::catalog::api_types::SchemaDefinition {
    use graphica_core::catalog::api_types::{
        ColumnDefinition, RelationshipType, SchemaDefinition, TableDefinition,
        TableRelationshipDefinition,
    };

    let tables = discovered
        .tables
        .iter()
        .map(|table| TableDefinition {
            name: table.name.clone(),
            columns: table
                .columns
                .iter()
                .map(|column| ColumnDefinition {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    nullable: column.nullable,
                    primary_key: column.primary_key,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                })
                .collect(),
            estimated_rows: table.row_count,
        })
        .collect();

    let relationships = discovered
        .relationships
        .iter()
        .map(|rel| TableRelationshipDefinition {
            name: rel.name.clone(),
            source_table: rel.source_table.clone(),
            source_columns: rel.source_columns.clone(),
            target_table: rel.target_table.clone(),
            target_columns: rel.target_columns.clone(),
            relationship_type: RelationshipType::ForeignKey,
            on_delete: None,
            on_update: None,
        })
        .collect();

    SchemaDefinition {
        name: discovered.schema_name.clone(),
        tables,
        relationships,
        indexes: vec![],
        inferred_at: chrono::Utc::now(),
    }
}
