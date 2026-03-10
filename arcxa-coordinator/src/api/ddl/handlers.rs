//! DDL API Handlers
//!
//! HTTP handlers for DDL generation endpoints.

use super::types::*;
use crate::api::auth::Claims;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::mapping::ddl::evolution::versioning::record_schema_version;
use crate::mapping::ddl::{
    convert_shape_to_table, get_dialect, DbDialect, DdlParser, MigrationGenerator,
    SchemaDiffEngine, ShaclParser,
};
use axum::extract::{Extension, State};
use axum::Json;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Generate DDL from SHACL shape
#[utoipa::path(
    post,
    path = "/api/v1/ddl/generate",
    request_body = GenerateDdlRequest,
    responses(
        (status = 200, description = "DDL generated successfully from SHACL shape", body = GenerateDdlResponse),
        (status = 400, description = "Invalid SHACL URI or unsupported SQL dialect"),
        (status = 503, description = "RDF store not available"),
        (status = 500, description = "Internal error - failed to parse SHACL or generate DDL"),
    ),
    tag = "DDL Generation"
)]
pub async fn generate_ddl(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GenerateDdlRequest>,
) -> Result<Json<GenerateDdlResponse>, ApiError> {
    info!(
        "Generating DDL for shape: {}, dialect: {}",
        request.shacl_uri, request.dialect
    );

    // Get SQL dialect
    let dialect = get_dialect(&request.dialect)
        .map_err(|e| ApiError::bad_request(format!("Invalid dialect: {}", e)))?;

    // Create SHACL parser
    let parser = ShaclParser::new();

    // Get RDF store
    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("RDF store not available".to_string()))?
        .clone();

    // Create SPARQL query function
    let sparql_query_fn = move |query: &str| {
        use crate::governance::rdf_store::RdfStore;

        let results = rdf_store.query(query)?;

        // Convert SPARQL results to HashMap format
        let mut rows = Vec::new();
        for row in results {
            let mut map = std::collections::HashMap::new();
            if let serde_json::Value::Object(obj) = row {
                for (key, value) in obj {
                    if let serde_json::Value::String(s) = value {
                        map.insert(key, s);
                    }
                }
            }
            rows.push(map);
        }

        Ok(rows)
    };

    // Parse SHACL shape from RDF store
    let shape = parser
        .parse_node_shape(&request.shacl_uri, sparql_query_fn)
        .map_err(|e| ApiError::internal(format!("Failed to parse SHACL shape: {}", e)))?;

    // Convert SHACL shape to table definition
    let mut table_def = convert_shape_to_table(&shape, &*dialect);

    // Remove indexes if not requested
    if !request.include_indexes {
        table_def.indexes.clear();
    }

    // Remove foreign keys if not requested
    if !request.include_foreign_keys {
        table_def.foreign_keys.clear();
    }

    // Generate DDL statements
    let mut ddl_statements = Vec::new();

    // CREATE TABLE statement
    let create_table_sql = if request.idempotent {
        let check_sql = dialect.check_table_exists(&table_def.name);
        format!(
            "-- Create table if not exists\nDO $$\nBEGIN\n  IF NOT EXISTS ({}) THEN\n    {};\n  END IF;\nEND $$",
            check_sql,
            dialect.create_table(&table_def)
        )
    } else {
        dialect.create_table(&table_def)
    };

    ddl_statements.push(create_table_sql);

    // CREATE INDEX statements
    if request.include_indexes {
        for index in &table_def.indexes {
            ddl_statements.push(dialect.create_index(index));
        }
    }

    // CREATE FOREIGN KEY statements
    if request.include_foreign_keys {
        for fk in &table_def.foreign_keys {
            ddl_statements.push(dialect.create_foreign_key(&table_def.name, fk));
        }
    }

    // Generate SQL script
    let sql_script = ddl_statements.join(";\n\n") + ";";

    info!(
        "Generated DDL for shape {}: {} statements",
        request.shacl_uri,
        ddl_statements.len()
    );

    Ok(Json(GenerateDdlResponse {
        ddl_statements,
        tables_generated: 1,
        dialect: request.dialect,
        sql_script,
    }))
}

/// Generate schema migration
#[utoipa::path(
    post,
    path = "/api/v1/ddl/migrate",
    request_body = GenerateMigrationRequest,
    responses(
        (status = 200, description = "Migration plan generated successfully with ALTER/DROP statements", body = GenerateMigrationResponse),
        (status = 400, description = "Invalid SHACL URI, unsupported dialect, or failed to parse desired shapes"),
        (status = 503, description = "RDF store not available"),
        (status = 500, description = "Internal error - failed to compute schema diff or generate migration"),
    ),
    tag = "DDL Generation"
)]
pub async fn generate_migration(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GenerateMigrationRequest>,
) -> Result<Json<GenerateMigrationResponse>, ApiError> {
    info!(
        "Generating migration: {} shapes -> {} shapes, dialect: {}",
        request.from_shacl.len(),
        request.to_shacl.len(),
        request.dialect
    );

    // Get SQL dialect
    let dialect = get_dialect(&request.dialect)
        .map_err(|e| ApiError::bad_request(format!("Invalid dialect: {}", e)))?;

    // Create SHACL parser
    let parser = ShaclParser::new();

    // Get RDF store
    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("RDF store not available".to_string()))?
        .clone();

    // Create SPARQL query function
    let sparql_query_fn = move |query: &str| {
        use crate::governance::rdf_store::RdfStore;

        let results = rdf_store.query(query)?;

        // Convert SPARQL results to HashMap format
        let mut rows = Vec::new();
        for row in results {
            let mut map = std::collections::HashMap::new();
            if let serde_json::Value::Object(obj) = row {
                for (key, value) in obj {
                    if let serde_json::Value::String(s) = value {
                        map.insert(key, s);
                    }
                }
            }
            rows.push(map);
        }

        Ok(rows)
    };

    // Parse current shapes
    let mut current_shapes = Vec::new();
    for shape_uri in &request.from_shacl {
        match parser.parse_node_shape(shape_uri, sparql_query_fn.clone()) {
            Ok(shape) => current_shapes.push(shape),
            Err(e) => {
                warn!("Failed to parse current shape {}: {}", shape_uri, e);
                // Continue with other shapes
            }
        }
    }

    // Parse desired shapes
    let mut desired_shapes = Vec::new();
    for shape_uri in &request.to_shacl {
        match parser.parse_node_shape(shape_uri, sparql_query_fn.clone()) {
            Ok(shape) => desired_shapes.push(shape),
            Err(e) => {
                return Err(ApiError::bad_request(format!(
                    "Failed to parse desired shape {}: {}",
                    shape_uri, e
                )));
            }
        }
    }

    // Compute schema diff
    let diff_engine = SchemaDiffEngine::new();
    let diff = diff_engine.compute_diff_from_shapes(&current_shapes, &desired_shapes, |shape| {
        convert_shape_to_table(shape, &*dialect)
    });

    // Generate migration
    let migration_gen = MigrationGenerator::new();
    let plan = if request.idempotent {
        migration_gen.generate_idempotent_migration(&diff, &*dialect)
    } else {
        migration_gen.generate_migration(&diff, &*dialect)
    }
    .map_err(|e| ApiError::internal(format!("Failed to generate migration: {}", e)))?;

    let migration_sql: Vec<_> = plan.steps.iter().map(|s| s.sql.clone()).collect();
    let migration_script = plan.to_sql();

    info!(
        "Generated migration: {} steps, safe: {}",
        plan.steps.len(),
        plan.safe
    );

    Ok(Json(GenerateMigrationResponse {
        migration_sql,
        safe: plan.safe,
        warnings: plan.warnings,
        steps: plan.steps.len(),
        migration_script,
    }))
}

/// Validate DDL SQL
#[utoipa::path(
    post,
    path = "/api/v1/ddl/validate",
    request_body = ValidateDdlRequest,
    responses(
        (status = 200, description = "DDL validation complete - returns syntax errors and warnings (DROP TABLE, missing semicolons, etc.)", body = ValidateDdlResponse),
    ),
    tag = "DDL Generation"
)]
pub async fn validate_ddl(
    Json(request): Json<ValidateDdlRequest>,
) -> Result<Json<ValidateDdlResponse>, ApiError> {
    info!("Validating DDL for dialect: {}", request.dialect);

    // Basic validation
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check dialect is supported
    if get_dialect(&request.dialect).is_err() {
        errors.push(format!("Unsupported dialect: {}", request.dialect));
    }

    // Basic SQL syntax checks
    let sql_upper = request.ddl_sql.to_uppercase();

    if !sql_upper.contains("CREATE TABLE") && !sql_upper.contains("ALTER TABLE") {
        warnings.push("DDL does not contain CREATE TABLE or ALTER TABLE statements".to_string());
    }

    // Check for potentially dangerous operations
    if sql_upper.contains("DROP TABLE") {
        warnings.push("DDL contains DROP TABLE (data loss risk)".to_string());
    }

    if sql_upper.contains("DROP COLUMN") {
        warnings.push("DDL contains DROP COLUMN (data loss risk)".to_string());
    }

    // Check for missing semicolons
    if !request.ddl_sql.trim().ends_with(';') {
        warnings.push("DDL does not end with semicolon".to_string());
    }

    let valid = errors.is_empty();

    info!(
        "DDL validation complete: valid={}, errors={}, warnings={}",
        valid,
        errors.len(),
        warnings.len()
    );

    Ok(Json(ValidateDdlResponse {
        valid,
        errors,
        warnings,
    }))
}

/// List available SHACL shapes
#[utoipa::path(
    post,
    path = "/api/v1/ddl/shapes",
    request_body = ListShapesRequest,
    responses(
        (status = 200, description = "SHACL node shapes retrieved from RDF store with metadata (target class, property count, labels)", body = ListShapesResponse),
        (status = 503, description = "RDF store not available"),
        (status = 500, description = "Internal error - failed to list or parse SHACL shapes"),
    ),
    tag = "DDL Generation"
)]
pub async fn list_shapes(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ListShapesRequest>,
) -> Result<Json<ListShapesResponse>, ApiError> {
    info!("Listing SHACL shapes");

    // Create SHACL parser
    let parser = ShaclParser::new();

    // Get RDF store
    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("RDF store not available".to_string()))?
        .clone();

    // Create SPARQL query function
    let sparql_query_fn = move |query: &str| {
        use crate::governance::rdf_store::RdfStore;

        let results = rdf_store.query(query)?;

        // Convert SPARQL results to HashMap format
        let mut rows = Vec::new();
        for row in results {
            let mut map = std::collections::HashMap::new();
            if let serde_json::Value::Object(obj) = row {
                for (key, value) in obj {
                    if let serde_json::Value::String(s) = value {
                        map.insert(key, s);
                    }
                }
            }
            rows.push(map);
        }

        Ok(rows)
    };

    // List all node shapes
    let shape_uris = parser
        .list_node_shapes(sparql_query_fn.clone())
        .map_err(|e| ApiError::internal(format!("Failed to list shapes: {}", e)))?;

    // Parse each shape to get metadata
    let mut shapes = Vec::new();
    for shape_uri in shape_uris {
        match parser.parse_node_shape(&shape_uri, sparql_query_fn.clone()) {
            Ok(shape) => {
                // Apply filter if specified
                if let Some(prefix) = &request.target_class_prefix {
                    if !shape.target_class.starts_with(prefix) {
                        continue;
                    }
                }

                shapes.push(ShapeInfo {
                    uri: shape.uri,
                    target_class: shape.target_class,
                    label: shape.label,
                    property_count: shape.properties.len(),
                });
            }
            Err(e) => {
                warn!("Failed to parse shape {}: {}", shape_uri, e);
                // Continue with other shapes
            }
        }
    }

    info!("Found {} SHACL shapes", shapes.len());

    Ok(Json(ListShapesResponse { shapes }))
}

/// Execute DDL statements against target database
#[utoipa::path(
    post,
    path = "/api/v1/ddl/execute",
    request_body = ExecuteDdlRequest,
    responses(
        (status = 200, description = "DDL executed against target database (PostgreSQL, DB2, Oracle) with lineage tracking and schema versioning", body = ExecuteDdlResponse),
        (status = 400, description = "Invalid database configuration or failed to create executor"),
        (status = 503, description = "Database connection failed - check host, port, credentials"),
        (status = 500, description = "Internal error during DDL execution (may include partial success if continue_on_error enabled)"),
    ),
    tag = "DDL Generation"
)]
pub async fn execute_ddl(
    State(state): State<Arc<ApiState>>,
    claims: Option<Extension<Claims>>,
    Json(request): Json<ExecuteDdlRequest>,
) -> Result<Json<ExecuteDdlResponse>, ApiError> {
    use super::executor::DdlExecutorFactory;

    info!(
        "Executing {} DDL statements for {} database: {}",
        request.ddl_statements.len(),
        format!("{:?}", request.database_config.db_type),
        request.database_config.database
    );

    // Create executor for target database
    let executor = DdlExecutorFactory::create(&request.database_config)
        .map_err(|e| ApiError::bad_request(format!("Failed to create executor: {}", e)))?;

    // Test connection first
    executor
        .test_connection()
        .await
        .map_err(|e| ApiError::service_unavailable(format!("Database connection failed: {}", e)))?;

    // Execute DDL statements
    let start_time = std::time::Instant::now();
    let result = executor
        .execute(
            request.ddl_statements.clone(),
            request.transactional,
            request.continue_on_error,
        )
        .await;

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(stats) => {
            // Track lineage if SHACL URI provided
            if let Some(shacl_uri) = &request.shacl_uri {
                if let Err(e) = track_schema_lineage(
                    &state,
                    shacl_uri,
                    &request.ddl_statements,
                    &request.database_config,
                )
                .await
                {
                    warn!("Failed to track schema lineage: {}", e);
                    // Don't fail the request if lineage tracking fails
                }
            }

            // Save schema version if store is available
            if let Some(ref schema_store) = state.schema_version_store {
                // Extract user ID from auth claims (fallback to "system" for automated tasks)
                let user_id = claims
                    .as_ref()
                    .map(|c| c.sub.clone())
                    .unwrap_or_else(|| "system".to_string());

                // Map database type to parser dialect
                let dialect = match format!("{:?}", request.database_config.db_type)
                    .to_lowercase()
                    .as_str()
                {
                    s if s.contains("db2") => DbDialect::DB2,
                    s if s.contains("postgres") || s.contains("pg") => DbDialect::PostgreSQL,
                    s if s.contains("oracle") => DbDialect::Oracle,
                    _ => {
                        // Default to PostgreSQL if unknown (most permissive parser)
                        debug!(
                            "Unknown database type: {:?}, defaulting to PostgreSQL parser",
                            request.database_config.db_type
                        );
                        DbDialect::PostgreSQL
                    }
                };

                // Create DDL parser for this dialect
                let parser = DdlParser::new(dialect);
                let mut tables_versioned = 0;
                let mut parse_errors = 0;

                // Parse each DDL statement to extract table definitions
                for (idx, ddl_stmt) in request.ddl_statements.iter().enumerate() {
                    // Parse the DDL statement
                    match parser.parse_ddl_script(ddl_stmt) {
                        Ok(tables) => {
                            if tables.is_empty() {
                                debug!(
                                    "Statement #{} did not contain CREATE TABLE: {}",
                                    idx + 1,
                                    ddl_stmt.chars().take(100).collect::<String>()
                                );
                                continue;
                            }

                            // Record schema version for each table found
                            for table_def in tables {
                                let change_desc = format!(
                                    "DDL execution via API: {} columns, {} constraints",
                                    table_def.columns.len(),
                                    table_def.primary_key.len() + table_def.foreign_keys.len()
                                );

                                match record_schema_version(
                                    schema_store.as_ref(),
                                    &table_def.name,
                                    &table_def,
                                    &change_desc,
                                    &user_id,
                                )
                                .await
                                {
                                    Ok(version) => {
                                        tables_versioned += 1;
                                        debug!(
                                            "Recorded schema version {} for table {} (created by {})",
                                            version.version_id, table_def.name, user_id
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to record schema version for table {}: {}",
                                            table_def.name, e
                                        );
                                        // Continue processing other tables
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            parse_errors += 1;
                            warn!(
                                "Failed to parse DDL statement #{}: {} - Statement: {}",
                                idx + 1,
                                e,
                                ddl_stmt.chars().take(200).collect::<String>()
                            );
                            // Continue with other statements - parsing errors are not fatal
                        }
                    }
                }

                if tables_versioned > 0 {
                    info!(
                        "Schema versioning complete: {} tables versioned, {} parse errors",
                        tables_versioned, parse_errors
                    );
                } else if parse_errors > 0 {
                    warn!("No tables versioned due to {} parse errors", parse_errors);
                } else {
                    debug!("No CREATE TABLE statements found in DDL");
                }
            }

            info!(
                "DDL execution successful: {} statements, {} tables, {}ms",
                stats.statements_executed, stats.tables_affected, stats.execution_time_ms
            );

            Ok(Json(ExecuteDdlResponse {
                success: true,
                statements_executed: stats.statements_executed,
                tables_affected: stats.tables_affected,
                execution_time_ms: stats.execution_time_ms,
                errors: vec![],
                message: format!(
                    "Successfully executed {} DDL statements affecting {} tables",
                    stats.statements_executed, stats.tables_affected
                ),
            }))
        }
        Err(errors) => {
            let statements_executed = request.ddl_statements.len() - errors.len();

            warn!(
                "DDL execution completed with errors: {}/{} statements failed",
                errors.len(),
                request.ddl_statements.len()
            );

            Ok(Json(ExecuteDdlResponse {
                success: false,
                statements_executed,
                tables_affected: 0, // Can't determine partial table count
                execution_time_ms,
                errors,
                message: format!(
                    "DDL execution failed: {}/{} statements succeeded",
                    statements_executed,
                    request.ddl_statements.len()
                ),
            }))
        }
    }
}

/// Track schema changes in lineage system
async fn track_schema_lineage(
    state: &Arc<ApiState>,
    shacl_uri: &str,
    ddl_statements: &[String],
    db_config: &DatabaseConnectionConfig,
) -> Result<(), String> {
    use crate::governance::rdf_store::{NamedGraph, RdfTriple, RdfValue};

    // Check if RDF store is available - gracefully skip if not
    let rdf_store = match state.rdf_store.as_ref() {
        Some(store) => store,
        None => {
            debug!("RDF store not available, skipping schema lineage tracking");
            return Ok(());
        }
    };

    let timestamp = chrono::Utc::now();
    let schema_change_id = uuid::Uuid::new_v4();

    // Generate unique URIs for this schema change event
    let activity_uri = format!(
        "http://graphica.io/activity/schema_change_{}",
        schema_change_id
    );
    let db_entity_uri = format!(
        "http://graphica.io/entity/db_table_{}_{}",
        db_config.database,
        timestamp.timestamp()
    );

    debug!(
        "Tracking schema lineage: activity={}, shacl={}, database={}",
        activity_uri, shacl_uri, db_config.database
    );

    // Create W3C PROV-compliant triples for lineage tracking
    // See: https://www.w3.org/TR/prov-o/
    let mut triples = vec![
        // ========================================
        // Activity: Schema change operation
        // ========================================

        // Type declaration: this is a PROV Activity
        RdfTriple {
            subject: activity_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfValue::Uri("http://www.w3.org/ns/prov#Activity".to_string()),
        },
        // When did this activity start?
        RdfTriple {
            subject: activity_uri.clone(),
            predicate: "http://www.w3.org/ns/prov#startedAtTime".to_string(),
            object: RdfValue::TypedLiteral {
                value: timestamp.to_rfc3339(),
                datatype: "http://www.w3.org/2001/XMLSchema#dateTime".to_string(),
            },
        },
        // Activity used the SHACL shape as input
        RdfTriple {
            subject: activity_uri.clone(),
            predicate: "http://www.w3.org/ns/prov#used".to_string(),
            object: RdfValue::Uri(shacl_uri.to_string()),
        },
        // ========================================
        // Entity: Database table (output)
        // ========================================

        // Type declaration: this is a PROV Entity
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfValue::Uri("http://www.w3.org/ns/prov#Entity".to_string()),
        },
        // Type declaration: this is also a database table
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfValue::Uri("http://graphica.io/ontology#DatabaseTable".to_string()),
        },
        // Provenance: entity was generated by the activity
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://www.w3.org/ns/prov#wasGeneratedBy".to_string(),
            object: RdfValue::Uri(activity_uri.clone()),
        },
        // ========================================
        // Database metadata
        // ========================================
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://graphica.io/ontology#database".to_string(),
            object: RdfValue::Literal(db_config.database.clone()),
        },
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://graphica.io/ontology#host".to_string(),
            object: RdfValue::Literal(db_config.host.clone()),
        },
        RdfTriple {
            subject: db_entity_uri.clone(),
            predicate: "http://graphica.io/ontology#port".to_string(),
            object: RdfValue::TypedLiteral {
                value: db_config.port.to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_string(),
            },
        },
    ];

    // Add DDL statement metadata (limit to first 3 statements to avoid graph bloat)
    for (idx, ddl_stmt) in ddl_statements.iter().take(3).enumerate() {
        let stmt_uri = format!("{}/ddl_statement_{}", activity_uri, idx);

        // Link activity to DDL statement
        triples.push(RdfTriple {
            subject: activity_uri.clone(),
            predicate: "http://graphica.io/ontology#hasDdlStatement".to_string(),
            object: RdfValue::Uri(stmt_uri.clone()),
        });

        // Type the DDL statement
        triples.push(RdfTriple {
            subject: stmt_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfValue::Uri("http://graphica.io/ontology#DdlStatement".to_string()),
        });

        // Store truncated DDL text (first 500 chars to avoid bloat)
        let ddl_text = ddl_stmt.chars().take(500).collect::<String>();
        triples.push(RdfTriple {
            subject: stmt_uri,
            predicate: "http://graphica.io/ontology#statementText".to_string(),
            object: RdfValue::Literal(ddl_text),
        });
    }

    // If there are more than 3 statements, record the total count
    if ddl_statements.len() > 3 {
        triples.push(RdfTriple {
            subject: activity_uri.clone(),
            predicate: "http://graphica.io/ontology#totalDdlStatements".to_string(),
            object: RdfValue::TypedLiteral {
                value: ddl_statements.len().to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_string(),
            },
        });
    }

    // Create a named graph for schema lineage (one graph per day)
    // This allows for efficient querying and graph-level operations
    let lineage_graph = NamedGraph {
        uri: format!(
            "http://graphica.io/graph/schema_lineage/{}",
            timestamp.format("%Y%m%d")
        ),
    };

    // Insert triples into RDF store with error handling
    match rdf_store.insert_batch(&triples, Some(&lineage_graph)) {
        Ok(_) => {
            info!(
                "Tracked schema lineage: {} triples for SHACL URI {} -> Database {}.{} (activity: {})",
                triples.len(),
                shacl_uri,
                db_config.host,
                db_config.database,
                schema_change_id
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                "Failed to insert schema lineage triples for {}: {}",
                shacl_uri, e
            );
            // Don't fail the whole operation if lineage tracking fails
            // This ensures DDL execution succeeds even if lineage recording has issues
            Ok(())
        }
    }
}
