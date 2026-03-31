//! DB Loader Callback Implementation
//!
//! Provides the callback function for workflow executor DB loading steps.
//! This allows the graphica-core workflow executor to load data to databases
//! without creating a dependency from core to coordinator.
//!
//! ## Performance Evolution
//!
//! - **Phase 1.0 (Row-by-row)**: ~2-5K rows/sec  - Original implementation
//! - **Phase 1.1 (Multi-row INSERT)**: ~20-50K rows/sec - Current (10-50x faster)
//! - **Phase 2.0 (ODBC array binding)**: ~200K+ rows/sec - Future (100x faster)
//!
//! Current implementation uses batched multi-row INSERTs with configurable batch size.
//! This provides significant performance improvement while maintaining transaction safety.

use crate::common::datasource_readiness::{evaluate_datasource_readiness, DatasourceOperation};
use crate::governance::rdf_store::GraphicaRdfStore;
use crate::mapping::loader::odbc_db2_connection::OdbcDB2Connection;
use crate::mapping::loader::{DB2Connection, SqlParam, SqlParamType};
use crate::workflows::db_loader;
use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::{types::SourceConfig, Credentials, DataSourceCatalog};
use graphica_core::core::lineage::row_level::{DatabaseType, RowId};
use graphica_core::secrets::providers::SecretStoreRegistry;
use graphica_core::secrets::{get_secret_by_ref, SecretValue};
use serde_json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Batch size for multi-row INSERTs
///
/// Tuned for optimal performance:
/// - Small enough to avoid exceeding SQL statement size limits (~32KB for DB2)
/// - Large enough to minimize round-trips to database
/// - Tested with 100K+ row datasets
const BATCH_SIZE: usize = 100;

/// Maximum identifier length for DB2
/// DB2 supports up to 128 characters for identifiers
const MAX_IDENTIFIER_LENGTH: usize = 128;

/// DB2 reserved words that cannot be used as table or column names
/// This is a subset of commonly-used DB2 reserved words that pose the highest injection risk.
/// Full list: https://www.ibm.com/docs/en/db2/11.5?topic=words-reserved-schema-names
const DB2_RESERVED_WORDS: &[&str] = &[
    "DATE",
    "TIME",
    "TIMESTAMP",
    "USER",
    "TABLE",
    "INDEX",
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "DROP",
    "ALTER",
    "GRANT",
    "REVOKE",
    "WHERE",
    "FROM",
    "JOIN",
    "INNER",
    "OUTER",
    "LEFT",
    "RIGHT",
    "UNION",
    "GROUP",
    "ORDER",
    "BY",
    "HAVING",
    "AS",
    "AND",
    "OR",
    "NOT",
    "NULL",
    "IS",
    "IN",
    "LIKE",
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "PRIMARY",
    "KEY",
    "FOREIGN",
    "REFERENCES",
    "UNIQUE",
    "CHECK",
    "DEFAULT",
    "CONSTRAINT",
    "VIEW",
    "TRIGGER",
    "PROCEDURE",
    "FUNCTION",
    "SCHEMA",
    "DATABASE",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "TRANSACTION",
    "SET",
    "DECLARE",
    "CURSOR",
    "FETCH",
    "OPEN",
    "CLOSE",
    "VALUES",
    "INTO",
    "DISTINCT",
    "ALL",
    "ANY",
    "SOME",
    "ASC",
    "DESC",
    "ON",
    "FOR",
    "WITH",
];

/// Validates a SQL identifier (table name or column name) to prevent injection attacks.
///
/// # Validation Rules
/// 1. Length: Must be between 1 and 128 characters (DB2 limit)
/// 2. First character: Must be a letter (A-Z, a-z) or underscore (_)
/// 3. Subsequent characters: Must be alphanumeric (A-Z, a-z, 0-9) or underscore (_)
/// 4. Reserved words: Must not be a DB2 reserved word (case-insensitive)
///
/// # Security
/// This function prevents SQL injection by ensuring identifiers cannot contain:
/// - Whitespace, semicolons, quotes, or SQL operators
/// - SQL keywords that could be exploited
/// - Special characters that could break out of identifier context
///
/// # Arguments
/// * `identifier` - The identifier to validate
/// * `identifier_type` - Type of identifier for error messages ("table name", "column name")
///
/// # Returns
/// * `Ok(())` if validation passes
/// * `Err` with descriptive message if validation fails
///
/// # Examples
/// ```rust,ignore
/// validate_sql_identifier("my_table", "table name")?; // OK
/// validate_sql_identifier("123invalid", "table name")?; // Error: starts with digit
/// validate_sql_identifier("SELECT", "table name")?; // Error: reserved word
/// ```
fn validate_sql_identifier(identifier: &str, identifier_type: &str) -> Result<()> {
    // Check: Not empty
    if identifier.is_empty() {
        return Err(anyhow!(
            "Invalid {}: identifier cannot be empty",
            identifier_type
        ));
    }

    // Check: Length constraint (DB2 max is 128 characters)
    if identifier.len() > MAX_IDENTIFIER_LENGTH {
        return Err(anyhow!(
            "Invalid {}: '{}' exceeds maximum length of {} characters (actual: {})",
            identifier_type,
            identifier,
            MAX_IDENTIFIER_LENGTH,
            identifier.len()
        ));
    }

    // Check: First character must be letter or underscore
    let first_char = identifier
        .chars()
        .next()
        .ok_or_else(|| anyhow!("Invalid {}: identifier is empty", identifier_type))?;

    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(anyhow!(
            "Invalid {}: '{}' must start with a letter (A-Z, a-z) or underscore (_), found '{}'",
            identifier_type,
            identifier,
            first_char
        ));
    }

    // Check: All characters must be alphanumeric or underscore
    for (idx, ch) in identifier.chars().enumerate() {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            return Err(anyhow!(
                "Invalid {}: '{}' contains invalid character '{}' at position {}. Only letters, digits, and underscores are allowed",
                identifier_type,
                identifier,
                ch,
                idx
            ));
        }
    }

    // Check: Not a reserved word (case-insensitive)
    let identifier_upper = identifier.to_uppercase();
    if DB2_RESERVED_WORDS.contains(&identifier_upper.as_str()) {
        return Err(anyhow!(
            "Invalid {}: '{}' is a DB2 reserved word and cannot be used as an identifier",
            identifier_type,
            identifier
        ));
    }

    Ok(())
}

/// Validates a table name for SQL safety.
///
/// This is a convenience wrapper around `validate_sql_identifier` specifically for table names.
///
/// # Arguments
/// * `name` - The table name to validate
///
/// # Returns
/// * `Ok(())` if validation passes
/// * `Err` with descriptive message if validation fails
///
/// # Examples
/// ```rust,ignore
/// validate_table_name("orders")?; // OK
/// validate_table_name("order-history")?; // Error: contains hyphen
/// ```
fn validate_table_name(name: &str) -> Result<()> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.is_empty() {
        return Err(anyhow!("table name cannot be empty"));
    }
    for part in parts {
        validate_sql_identifier(part, "table name")?;
    }
    Ok(())
}

/// Validates a list of column names for SQL safety.
///
/// This function validates each column name and returns the first validation error encountered.
///
/// # Arguments
/// * `columns` - Slice of column name strings to validate
///
/// # Returns
/// * `Ok(())` if all column names are valid
/// * `Err` with descriptive message for the first invalid column name
///
/// # Examples
/// ```rust,ignore
/// validate_column_names(&["id", "name", "created_at"])?; // OK
/// validate_column_names(&["id", "user-name"])?; // Error: 'user-name' contains hyphen
/// ```
fn validate_column_names(columns: &[String]) -> Result<()> {
    for column in columns {
        validate_sql_identifier(column, "column name")
            .with_context(|| format!("Column validation failed for '{}'", column))?;
    }
    Ok(())
}

/// Quote a SQL identifier for DB2 compatibility
///
/// DB2 requires quoting identifiers that contain underscores (especially leading underscores)
/// to prevent them from being interpreted as conditional compilation directives.
///
/// # Arguments
/// * `name` - The identifier to quote (table name, column name, etc.)
///
/// # Returns
/// Quoted identifier string with double quotes, escaping any embedded quotes
///
/// # Example
/// ```rust,ignore
/// quote_identifier("PIPELINE_TEST_PATIENTS"); // Returns: "PIPELINE_TEST_PATIENTS"
/// quote_identifier("my_table"); // Returns: "my_table"
/// quote_identifier("table\"name"); // Returns: "table""name" (escaped)
/// ```
fn quote_identifier(name: &str) -> String {
    // Escape any embedded double quotes by doubling them (SQL standard)
    let escaped = name.replace("\"", "\"\"");
    format!("\"{}\"", escaped)
}

/// Generate multi-row INSERT SQL statement
///
/// Creates a SQL INSERT statement with multiple value tuples for batch insertion.
/// All identifiers are quoted to prevent DB2 conditional compilation issues.
///
/// # Arguments
/// * `table_name` - Target table name (must be pre-validated)
/// * `columns` - Column names (must be pre-validated)
/// * `batch_size` - Number of rows in this batch
///
/// # Returns
/// SQL string like: `INSERT INTO "table" ("col1", "col2") VALUES (?, ?), (?, ?), ...`
///
/// # Example
/// ```rust,ignore
/// let sql = generate_batch_insert("orders", &["id", "amount"], 3);
/// // Returns: "INSERT INTO "orders" ("id", "amount") VALUES (?, ?), (?, ?), (?, ?)"
/// ```
fn generate_batch_insert(table_name: &str, columns: &[String], batch_size: usize) -> String {
    // Quote table name to handle underscores and special characters
    let quoted_table = quote_identifier(table_name);

    // Quote each column name
    let column_list = columns
        .iter()
        .map(|col| quote_identifier(col))
        .collect::<Vec<_>>()
        .join(", ");

    let single_row_placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

    // Generate (?, ?, ...), (?, ?, ...), ... for batch_size rows
    let values_clause = (0..batch_size)
        .map(|_| format!("({})", single_row_placeholders))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "INSERT INTO {} ({}) VALUES {}",
        quoted_table, column_list, values_clause
    )
}

/// Wrapper struct to implement SqlParam for JSON-derived values
struct JsonSqlParam {
    value: String,
    param_type: SqlParamType,
}

impl SqlParam for JsonSqlParam {
    fn to_sql_string(&self) -> String {
        self.value.clone()
    }

    fn param_type(&self) -> SqlParamType {
        self.param_type
    }
}

/// Resolve a datasource identifier to a DataSourceResponse
///
/// Supports two resolution methods:
/// 1. URN lookup (if identifier starts with "urn:")
/// 2. Title lookup (for human-readable names like "db2_professional_demo")
///
/// # Arguments
/// * `catalog` - The datasource catalog to query
/// * `identifier` - The datasource URN or title
///
/// # Returns
/// The resolved datasource response
///
/// # Errors
/// Returns an error if the datasource is not found by either method
async fn resolve_datasource(
    catalog: &Arc<dyn DataSourceCatalog>,
    identifier: &str,
) -> Result<graphica_core::catalog::api_types::DataSourceResponse> {
    // Try URN lookup first (if starts with "urn:")
    if identifier.starts_with("urn:") {
        tracing::debug!("Resolving datasource by URN: {}", identifier);
        match catalog.get_source(identifier).await {
            Ok(response) => {
                tracing::info!(
                    "Resolved datasource by URN: {} -> {} ({})",
                    identifier,
                    response.source.title,
                    response.source.source_type
                );
                return Ok(response);
            }
            Err(e) => {
                tracing::debug!("URN lookup failed for {}: {}", identifier, e);
                // Fall through to title lookup
            }
        }
    }

    // Try title lookup
    tracing::debug!("Resolving datasource by title: {}", identifier);
    match catalog.get_source_by_title(identifier).await {
        Ok(response) => {
            tracing::info!(
                "Resolved datasource by title: {} -> {} ({})",
                identifier,
                response.source.id,
                response.source.source_type
            );
            Ok(response)
        }
        Err(e) => Err(anyhow!(
            "Failed to resolve datasource '{}': not found by URN or title. Error: {}",
            identifier,
            e
        )),
    }
}

/// Detect entity URI from workflow context or row metadata
///
/// This function checks multiple strategies to determine if ontology-driven loading should be used:
/// 1. Direct entity_uri metadata field in rows
/// 2. RDF @type or rdf:type annotations
/// 3. __entity_uri__ special field (convention for ETL pipelines)
///
/// # Returns
/// * `Some(uri)` if entity URI is detected
/// * `None` if no entity URI found (use legacy loading)
fn detect_entity_uri(rows: &[serde_json::Map<String, serde_json::Value>]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let first_row = &rows[0];

    // Strategy 1: Check for @type or rdf:type field (RDF/JSON-LD convention)
    if let Some(type_val) = first_row.get("@type").or_else(|| first_row.get("rdf:type")) {
        if let Some(uri) = type_val.as_str() {
            tracing::debug!("Detected entity URI from @type field: {}", uri);
            return Some(uri.to_string());
        }
    }

    // Strategy 2: Check for __entity_uri__ metadata field (ETL convention)
    if let Some(uri) = first_row.get("__entity_uri__").and_then(|v| v.as_str()) {
        tracing::debug!("Detected entity URI from __entity_uri__ field: {}", uri);
        return Some(uri.to_string());
    }

    // Strategy 3: Check for entity_uri field (direct convention)
    if let Some(uri) = first_row.get("entity_uri").and_then(|v| v.as_str()) {
        tracing::debug!("Detected entity URI from entity_uri field: {}", uri);
        return Some(uri.to_string());
    }

    None
}

async fn resolve_credentials(
    source: &graphica_core::catalog::types::DataSource,
    registry: Option<Arc<SecretStoreRegistry>>,
) -> Result<Credentials> {
    if !source.connection.credentials.is_empty() {
        return credentials_from_map(&source.connection.credentials, "connection.credentials");
    }

    if let Some(registry) = registry {
        if !source.connection.secret_ref.trim().is_empty() {
            let store = registry
                .default()
                .or_else(|| registry.get("default"))
                .ok_or_else(|| anyhow!("No default secret store configured"))?;

            let secret = get_secret_by_ref(store.as_ref(), &source.connection.secret_ref, None)
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve secretRef '{}' for datasource '{}'",
                        source.connection.secret_ref, source.id
                    )
                })?;

            return credentials_from_secret_value(&secret.value);
        }
    }

    if !source.metadata.is_empty() {
        return credentials_from_map(&source.metadata, "metadata");
    }

    Err(anyhow!(
        "Missing credentials for datasource {} (no secretRef credentials available)",
        source.id
    ))
}

fn credentials_from_secret_value(value: &SecretValue) -> Result<Credentials> {
    match value {
        SecretValue::KeyValue(map) => credentials_from_map(map, "secret value"),
        SecretValue::String(raw) => credentials_from_json_str(raw),
        SecretValue::Json(json) => credentials_from_json_value(json),
        SecretValue::Binary(_) => Err(anyhow!(
            "Binary secret values are not supported for datasource credentials"
        )),
    }
}

fn credentials_from_json_str(raw: &str) -> Result<Credentials> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| anyhow!("Failed to parse secret JSON credentials: {}", e))?;
    credentials_from_json_value(&value)
}

fn credentials_from_json_value(value: &serde_json::Value) -> Result<Credentials> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("Credentials JSON must be an object with credential fields"))?;

    let mut map = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_string());
        } else {
            map.insert(k.clone(), v.to_string());
        }
    }

    credentials_from_map(&map, "credentials JSON")
}

fn credentials_from_map(map: &HashMap<String, String>, context: &str) -> Result<Credentials> {
    let (username, password) =
        if let (Some(user), Some(pass)) = (map.get("username"), map.get("password")) {
            (user.to_string(), pass.to_string())
        } else if let (Some(user), Some(pass)) = (map.get("user"), map.get("pass")) {
            (user.to_string(), pass.to_string())
        } else {
            return Err(anyhow!(
                "Missing credentials in {} (expected username/password or user/pass)",
                context
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

/// Create a DB loader callback that uses the datasource catalog
///
/// This callback supports two loading modes:
/// 1. **Ontology-driven loading** (when entity_uri is provided):
///    - Retrieves entity schema from RDF ontology via SPARQL
///    - Generates DDL automatically from ontology
///    - Maps JSON data to ontology-defined schema
///    - Resolves relationships to foreign keys
///
/// 2. **Legacy direct loading** (when no entity_uri):
///    - Uses existing table schema
///    - Direct INSERT/UPSERT operations
///    - Manual schema management
///
/// # Arguments
/// * `catalog` - Datasource catalog for connection resolution
/// * `rdf_store` - Optional RDF store for ontology-driven loading
///
/// # Workflow Integration
/// The callback will:
/// 1. Look up the datasource from the catalog (supports URN or title)
/// 2. Check for entity_uri in workflow context or row metadata
/// 3. If entity_uri present and rdf_store available: use OntologyDrivenLoader
/// 4. Otherwise: use legacy direct loading
/// 5. Return the load result, including best-effort output row identifiers
pub fn create_db_loader_callback(
    catalog: Arc<dyn DataSourceCatalog>,
    rdf_store: Option<Arc<GraphicaRdfStore>>,
    secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
) -> Arc<graphica_core::orchestration::workflow::executor::DbLoaderCallback> {
    Arc::new(Box::new(
        move |datasource_id, table_name, rows, mode, key_fields| {
            let catalog = catalog.clone();
            let rdf_store = rdf_store.clone();
            let secret_store_registry = secret_store_registry.clone();
            let datasource_id = datasource_id.to_string();
            let table_name = table_name.to_string();
            let mode = mode.to_string();
            let key_fields = key_fields.clone();

            Box::pin(async move {
                tracing::info!(
                    "DB Loader Callback: Loading {} rows to {}.{} (mode: {})",
                    rows.len(),
                    datasource_id,
                    table_name,
                    mode
                );

                // Resolve datasource from catalog (supports both URN and title lookup)
                let datasource_response = resolve_datasource(&catalog, &datasource_id)
                    .await
                    .with_context(|| format!("Failed to resolve datasource: {}", datasource_id))?;
                evaluate_datasource_readiness(
                    &datasource_response,
                    DatasourceOperation::WorkflowWrite,
                )
                .map_err(|failure| anyhow!(failure.message))?;

                let credentials =
                    resolve_credentials(&datasource_response.source, secret_store_registry.clone())
                        .await
                        .with_context(|| {
                            format!(
                                "Failed to resolve credentials for datasource {}",
                                datasource_id
                            )
                        })?;

                // Check for entity URI in row metadata (enables ontology-driven loading)
                let entity_uri = detect_entity_uri(&rows);

                if let Some(uri) = entity_uri {
                    if let Some(store) = rdf_store {
                        tracing::info!("Ontology-driven loading enabled for entity: {}", uri);

                        // Use ontology-driven loading path (DB2-only for now)
                        let db2_config = match &datasource_response.source.connection.config {
                            SourceConfig::DB2(config) => config,
                            other => {
                                return Err(anyhow!(
                                "Ontology-driven loading currently only supports DB2, found: {:?}",
                                other
                            ));
                            }
                        };

                        let rows_loaded = use_ontology_driven_loader(
                            &uri,
                            store,
                            &datasource_id,
                            &table_name,
                            db2_config,
                            &credentials,
                            rows,
                            &mode,
                        )
                        .await
                        .with_context(|| {
                            format!("Ontology-driven loading failed for entity: {}", uri)
                        })?;

                        return Ok(
                            graphica_core::orchestration::workflow::executor::DbLoadResult {
                                rows_loaded,
                                output_row_ids: Vec::new(),
                            },
                        );
                    } else {
                        tracing::warn!(
                        "Entity URI detected ({}) but no RDF store available, falling back to legacy loading",
                        uri
                    );
                    }
                }

                // Fallback to legacy direct loading
                tracing::debug!(
                    "Using legacy direct loading for {}.{}",
                    datasource_id,
                    table_name
                );

                let rows = db_loader::common::sanitize_rows_for_database_load(rows);
                let lineage_rows = rows.clone();

                // Extract connection config
                let connection_config = &datasource_response.source.connection.config;

                // Match on datasource type and load data
                let load_result = match connection_config {
                    SourceConfig::DB2(config) => {
                        load_to_db2(config, &credentials, &table_name, rows, &mode).await
                    }
                    SourceConfig::PostgreSQL(config) => {
                        db_loader::postgres::load(
                            config,
                            &credentials,
                            &table_name,
                            rows,
                            &mode,
                            key_fields.as_deref(),
                        )
                        .await
                    }
                    SourceConfig::Oracle(_) => {
                        db_loader::oracle::load(
                            &datasource_response.source,
                            &table_name,
                            rows,
                            &mode,
                            key_fields.as_deref(),
                            &credentials,
                        )
                        .await
                    }
                    SourceConfig::SAPHANA(_config) => {
                        Err(anyhow!("SAP HANA loading not yet implemented"))
                    }
                    SourceConfig::Databricks(config) => {
                        db_loader::databricks::load(
                            config,
                            &credentials,
                            &table_name,
                            rows,
                            &mode,
                            key_fields.as_deref(),
                        )
                        .await
                    }
                    _ => Err(anyhow!(
                        "Unsupported datasource type for DB loading: {:?}",
                        datasource_response.source.source_type
                    )),
                };

                let rows_loaded = load_result.map_err(|error| {
                    tracing::error!(
                        "DB Loader Callback failed for datasource={} table={} mode={}: {:#}",
                        datasource_id,
                        table_name,
                        mode,
                        error
                    );
                    error
                })?;

                let output_row_ids = build_output_row_ids_for_loaded_rows(
                    &datasource_response.source,
                    connection_config,
                    &table_name,
                    &lineage_rows,
                    key_fields.as_deref(),
                    &credentials,
                )
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        "Failed to derive DB load output row ids for {}.{}: {}",
                        datasource_id,
                        table_name,
                        error
                    );
                    Vec::new()
                });

                Ok(
                    graphica_core::orchestration::workflow::executor::DbLoadResult {
                        rows_loaded,
                        output_row_ids,
                    },
                )
            })
                as Pin<
                    Box<
                        dyn Future<
                                Output = Result<
                                    graphica_core::orchestration::workflow::executor::DbLoadResult,
                                >,
                            > + Send,
                    >,
                >
        },
    ))
}

async fn build_output_row_ids_for_loaded_rows(
    source: &graphica_core::catalog::types::DataSource,
    connection_config: &SourceConfig,
    table_name: &str,
    rows: &[serde_json::Map<String, serde_json::Value>],
    key_fields: Option<&[String]>,
    credentials: &Credentials,
) -> Result<Vec<Option<RowId>>> {
    let database_type = match database_type_for_source_config(connection_config) {
        Some(database_type) => database_type,
        None => return Ok(Vec::new()),
    };

    let effective_key_fields =
        if let Some(key_fields) = key_fields.filter(|fields| !fields.is_empty()) {
            key_fields.to_vec()
        } else if matches!(connection_config, SourceConfig::Oracle(_)) {
            crate::etl::loaders::database::oracle::query_primary_key_columns(
                source,
                table_name,
                credentials,
            )
            .await?
        } else {
            Vec::new()
        };

    if effective_key_fields.is_empty() {
        return Ok(Vec::new());
    }

    if matches!(connection_config, SourceConfig::Oracle(_)) {
        return Ok(crate::etl::loaders::database::oracle::build_output_row_ids(
            table_name,
            rows,
            &effective_key_fields,
        ));
    }

    Ok(rows
        .iter()
        .map(|row| {
            build_database_row_id(
                database_type.clone(),
                table_name,
                row,
                &effective_key_fields,
            )
        })
        .collect())
}

fn database_type_for_source_config(connection_config: &SourceConfig) -> Option<DatabaseType> {
    match connection_config {
        SourceConfig::DB2(_) => Some(DatabaseType::DB2),
        SourceConfig::PostgreSQL(_) => Some(DatabaseType::Postgres),
        SourceConfig::Oracle(_) => Some(DatabaseType::Oracle),
        SourceConfig::Databricks(_) => Some(DatabaseType::Databricks),
        SourceConfig::SAPHANA(_) => Some(DatabaseType::SAPHANA),
        _ => None,
    }
}

fn build_database_row_id(
    database_type: DatabaseType,
    table_name: &str,
    row: &serde_json::Map<String, serde_json::Value>,
    key_fields: &[String],
) -> Option<RowId> {
    let mut primary_key = std::collections::BTreeMap::new();

    for key_field in key_fields {
        let value = row.get(key_field)?;
        if value.is_null() {
            return None;
        }
        primary_key.insert(key_field.clone(), row_value_to_string(value));
    }

    if primary_key.is_empty() {
        return None;
    }

    Some(RowId::database(
        database_type,
        table_name.to_string(),
        primary_key,
    ))
}

fn row_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
    }
}

/// Convert catalog DB2Config to coordinator DB2Config using resolved credentials.
fn convert_db2_config(
    core_config: &graphica_core::catalog::types::DB2Config,
    credentials: &graphica_core::catalog::Credentials,
) -> Result<crate::mapping::loader::DB2Config> {
    if credentials.username.is_empty() || credentials.password.is_empty() {
        return Err(anyhow!(
            "DB2 credentials missing (username/password required)"
        ));
    }

    Ok(crate::mapping::loader::DB2Config {
        host: core_config.host.clone(),
        port: core_config.port,
        database: core_config.database.clone(),
        username: credentials.username.clone(),
        password: credentials.password.clone(),
        max_connections: 10,
        min_idle_connections: Some(2),
        connection_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(60),
        auto_commit: false, // Default to false for transactional safety
        max_retry_attempts: 3,
        retry_backoff_ms: 1000,
    })
}

/// Load data to DB2 using ODBC connection with batched multi-row INSERTs
///
/// ## Performance Optimization (Phase 1.1)
///
/// This function uses batched multi-row INSERT statements for optimal performance:
/// - Processes rows in batches of BATCH_SIZE (default 100)
/// - Generates SQL like: `INSERT INTO table (cols...) VALUES (?,...), (?,...), ...`
/// - Reduces round-trips to database by 100x compared to row-by-row insertion
/// - Maintains transactional safety with rollback on error
///
/// ## Error Handling
///
/// - Validates table and column names to prevent SQL injection
/// - Uses parameterized queries (NOT string concatenation) for values
/// - Reports batch-level errors with row index tracking
/// - Rolls back entire transaction on any batch failure
///
/// ## Future Optimization (Phase 2.0)
///
/// Next phase will use ODBC array parameter binding for even better performance:
/// - Native ODBC array binding API
/// - Server-side batch processing
/// - Expected 10x improvement over current implementation
async fn load_to_db2(
    config: &graphica_core::catalog::types::DB2Config,
    credentials: &graphica_core::catalog::Credentials,
    table_name: &str,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    mode: &str,
) -> Result<u64> {
    tracing::info!(
        "load_to_db2 START: table={}, rows={}, mode={}",
        table_name,
        rows.len(),
        mode
    );

    // SECURITY: Validate table name FIRST to prevent SQL injection
    tracing::info!("Validating table name: {}", table_name);
    validate_table_name(table_name).with_context(|| {
        format!(
            "Security validation failed for table name '{}'. \
             Table names must start with a letter or underscore, contain only alphanumeric \
             characters and underscores, be at most {} characters, and not be a reserved word.",
            table_name, MAX_IDENTIFIER_LENGTH
        )
    })?;
    tracing::info!("Table name validation passed");

    tracing::info!(
        "Connecting to DB2: {}:{}/{}",
        config.host,
        config.port,
        config.database
    );

    // Convert config type
    let loader_config = convert_db2_config(config, credentials)?;

    // Create ODBC connection
    let mut connection = OdbcDB2Connection::connect(&loader_config)
        .map_err(|e| anyhow!("Failed to connect to DB2: {}", e))?;

    // Verify connection is alive
    if !connection.is_alive() {
        return Err(anyhow!("DB2 connection is not alive"));
    }

    tracing::info!("Successfully connected to DB2, loading {} rows", rows.len());

    // PRE-VALIDATE: Check table existence BEFORE attempting INSERT
    // If table doesn't exist, we'll auto-create it from the data schema
    tracing::info!("Checking if target table exists: {}", table_name);
    match connection.validate_table_exists(table_name) {
        Ok(_) => {
            tracing::info!("Table {} exists - validation passed", table_name);
        }
        Err(e) => {
            tracing::warn!(
                "Table {} does not exist: {} - will auto-create",
                table_name,
                e
            );
            // TODO: Auto-create table from inferred schema
            // For now, return error to avoid ODBC panic
            return Err(anyhow!(
                "Table {} does not exist. Please create it first. Error: {}",
                table_name,
                e
            ));
        }
    }

    // Get column names from first row
    let columns: Vec<String> = if let Some(first_row) = rows.first() {
        first_row.keys().cloned().collect()
    } else {
        return Ok(0); // No rows to load
    };

    // SECURITY: Validate ALL column names to prevent SQL injection
    validate_column_names(&columns).with_context(|| {
        format!(
            "Security validation failed for column names in table '{}'. \
             Column names must start with a letter or underscore, contain only alphanumeric \
             characters and underscores, be at most {} characters, and not be reserved words.",
            table_name, MAX_IDENTIFIER_LENGTH
        )
    })?;

    tracing::info!(
        "Validated table '{}' and {} column names for SQL injection safety",
        table_name,
        columns.len()
    );

    // Begin transaction
    connection
        .begin_transaction()
        .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

    let mut loaded_count = 0u64;
    let total_rows = rows.len();

    // Process rows in batches
    for (batch_idx, batch) in rows.chunks(BATCH_SIZE).enumerate() {
        let batch_size = batch.len();
        let start_row_idx = batch_idx * BATCH_SIZE;

        // Generate multi-row INSERT SQL for this batch
        let insert_sql = match mode {
            "Insert" => generate_batch_insert(table_name, &columns, batch_size),
            "Upsert" => {
                // For DB2, we'd use MERGE statement, but for now just use INSERT
                // TODO: Implement proper MERGE for upsert mode
                generate_batch_insert(table_name, &columns, batch_size)
            }
            _ => generate_batch_insert(table_name, &columns, batch_size),
        };

        // Flatten all row values into a single parameter array
        // For 100 rows x 5 columns = 500 parameters
        let mut batch_params: Vec<JsonSqlParam> = Vec::with_capacity(batch_size * columns.len());

        for row in batch.iter() {
            for col in columns.iter() {
                let sql_param =
                    row.get(col)
                        .map(json_to_sql_param)
                        .unwrap_or_else(|| JsonSqlParam {
                            value: "NULL".to_string(),
                            param_type: SqlParamType::Null,
                        });
                batch_params.push(sql_param);
            }
        }

        // Convert to trait object references for ODBC binding
        let param_refs: Vec<&dyn SqlParam> =
            batch_params.iter().map(|p| p as &dyn SqlParam).collect();

        // Execute batch insert with panic recovery
        // Panic recovery is needed because the ODBC library (odbc-api) can panic
        // during handle cleanup when DB2 returns certain errors (like SQL20521N)
        let execute_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            connection.execute(&insert_sql, &param_refs)
        }));

        match execute_result {
            Ok(Ok(_)) => {
                // Success - batch inserted
                loaded_count += batch_size as u64;

                // Log progress every 10 batches or on last batch
                if (batch_idx + 1) % 10 == 0 || start_row_idx + batch_size >= total_rows {
                    tracing::info!(
                        "Batch progress: Loaded {} / {} rows ({:.1}%)",
                        loaded_count,
                        total_rows,
                        (loaded_count as f64 / total_rows as f64) * 100.0
                    );
                }
            }
            Ok(Err(e)) => {
                // DB error (clean error handling)
                tracing::error!(
                    "Failed to insert batch {} (rows {}-{}) into {}: {}",
                    batch_idx,
                    start_row_idx,
                    start_row_idx + batch_size - 1,
                    table_name,
                    e
                );
                // Rollback transaction on error (also wrap in panic recovery)
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    connection.rollback()
                }));
                return Err(anyhow!(
                    "Failed to insert batch {} (rows {}-{}): {}",
                    batch_idx,
                    start_row_idx,
                    start_row_idx + batch_size - 1,
                    e
                ));
            }
            Err(panic_info) => {
                // ODBC library panicked (likely during handle cleanup after SQL error)
                tracing::error!(
                    "ODBC library panicked during batch {} (rows {}-{}) - likely handle cleanup failure after SQL error",
                    batch_idx,
                    start_row_idx,
                    start_row_idx + batch_size - 1
                );
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    tracing::error!("Panic message: {}", s);
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    tracing::error!("Panic message: {}", s);
                }
                // Attempt rollback (also with panic recovery)
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    connection.rollback()
                }));
                return Err(anyhow!(
                    "ODBC library panic during batch insert - handle cleanup failure (likely after SQL error)"
                ));
            }
        }
    }

    // Commit transaction
    connection.commit().map_err(|e| {
        connection.rollback().ok();
        anyhow!("Failed to commit transaction: {}", e)
    })?;

    tracing::info!(
        "Successfully loaded {} rows to {}.{} using batched multi-row INSERTs (Phase 1.1 optimization)",
        loaded_count,
        config.database,
        table_name
    );

    Ok(loaded_count)
}

/// Load data to PostgreSQL using the ETL PostgreSQLLoader
/// Convert JSON value to SQL string representation
///
/// SECURITY NOTE: This function is only used for parameter values, NOT for identifiers.
/// All SQL identifiers (table names, column names) are validated separately.
/// Parameter values are bound through ODBC parameters, not string concatenation.
fn json_to_sql_param(value: &serde_json::Value) -> JsonSqlParam {
    match value {
        serde_json::Value::Null => JsonSqlParam {
            value: "NULL".to_string(),
            param_type: SqlParamType::Null,
        },
        serde_json::Value::Bool(b) => JsonSqlParam {
            value: if *b { "1" } else { "0" }.to_string(),
            param_type: SqlParamType::Boolean,
        },
        serde_json::Value::Number(n) => {
            let is_float = n.as_f64().is_some() && n.as_i64().is_none() && n.as_u64().is_none();
            JsonSqlParam {
                value: n.to_string(),
                param_type: if is_float {
                    SqlParamType::Decimal
                } else {
                    SqlParamType::Integer
                },
            }
        }
        serde_json::Value::String(s) => JsonSqlParam {
            value: s.clone(),
            param_type: SqlParamType::String,
        },
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => JsonSqlParam {
            value: value.to_string(),
            param_type: SqlParamType::String,
        },
    }
}

/// Use ontology-driven loader for entity-based loading
///
/// This function wires together all the ontology loading components:
/// - SparqlSchemaProvider: retrieves entity definitions from RDF store
/// - DB2TypeMapper: maps XSD types to DB2 SQL types
/// - DB2DDLGenerator: generates CREATE TABLE DDL
/// - HybridStrategy: determines normalization approach
/// - DefaultDataTransformer: transforms JSON to match schema
/// - DefaultRelationshipResolver: resolves entity references to FKs
/// - LruSchemaCache: caches schemas for performance
/// - DB2 executor wrapper: executes DDL and DML
///
/// # Arguments
/// * `entity_uri` - URI of the entity in the ontology (e.g., "http://example.org/Patient")
/// * `rdf_store` - RDF store containing the ontology
/// * `datasource_id` - Datasource identifier
/// * `table_name` - Target table name (may be overridden by ontology)
/// * `db2_config` - DB2 connection configuration
/// * `credentials` - Resolved datasource credentials
/// * `rows` - JSON data rows to load
/// * `mode` - Load mode (insert, upsert, replace)
///
/// # Returns
/// Number of rows successfully loaded
async fn use_ontology_driven_loader(
    entity_uri: &str,
    rdf_store: Arc<GraphicaRdfStore>,
    datasource_id: &str,
    table_name: &str,
    db2_config: &graphica_core::catalog::types::DB2Config,
    credentials: &graphica_core::catalog::Credentials,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    mode: &str,
) -> Result<u64> {
    use crate::workflows::ontology::*;

    tracing::info!(
        "Starting two-phase ontology-driven loading for entity '{}' ({} rows, target_table='{}', mode='{}')",
        entity_uri,
        rows.len(),
        table_name,
        mode
    );

    // ========================================
    // PHASE 1: ONTOLOGY VALIDATION (NO ODBC)
    // ========================================
    tracing::info!("Phase 1: Querying ontology for entity definition");

    // Schema provider: queries RDF store for entity definitions
    let schema_provider = Arc::new(SparqlSchemaProvider::new(rdf_store.clone()));

    // Normalization strategy: decides table structure
    let normalization = Arc::new(HybridStrategy::new());

    // Schema cache: LRU cache for entity definitions and schemas
    use crate::workflows::ontology::cache::LruCacheConfig;
    let cache = Arc::new(LruSchemaCache::with_config(LruCacheConfig {
        max_entities: 1000,
        max_schemas: 5000,
        max_ddl: 5000,
    }));

    let entity_def = schema_provider
        .get_entity_definition(entity_uri)
        .await
        .with_context(|| {
            format!(
                "Phase 1 Failed: Entity '{}' not found in ontology. \
                 Check that the entity exists in the RDF store.",
                entity_uri
            )
        })?;

    tracing::info!(
        "Phase 1: Entity '{}' resolved ({} properties, {} relationships)",
        entity_def.label,
        entity_def.properties.len(),
        entity_def.relationships.len()
    );

    let schemas = normalization
        .generate_schemas(&entity_def)
        .await
        .with_context(|| {
            format!(
                "Phase 1 Failed: Could not generate table schemas for entity '{}'",
                entity_uri
            )
        })?;

    if schemas.is_empty() {
        return Err(anyhow!(
            "Phase 1 Failed: Ontology entity '{}' generated no table schemas",
            entity_uri
        ));
    }

    // Prime cache so Phase 2 won't re-run SPARQL during loader execution.
    cache
        .cache_entity_def(entity_uri.to_string(), entity_def)
        .await;
    cache
        .cache_table_schemas(entity_uri.to_string(), schemas.clone())
        .await;

    tracing::info!(
        "Phase 1 Complete: Generated {} schema(s) for '{}'",
        schemas.len(),
        entity_uri
    );

    // ========================================
    // PHASE 2: DB CONNECT + LOAD (WITH ODBC)
    // ========================================
    tracing::info!("Phase 2: Resolving datasource and connecting to DB2");

    tracing::debug!(
        "Datasource resolved: {} -> {}:{}/{}",
        datasource_id,
        db2_config.host,
        db2_config.port,
        db2_config.database
    );

    // Type mapper: XSD -> DB2 SQL types
    let type_mapper = Arc::new(DB2TypeMapper::new());

    // DDL generator: generates CREATE TABLE statements
    let ddl_generator = Arc::new(DB2DDLGenerator::new(
        std::env::var("DB2_SCHEMA").unwrap_or_else(|_| "DB2INST1".to_string()),
    ));

    // Data transformer: transforms JSON to match schema
    let transformer = Arc::new(DefaultDataTransformer::new());

    // Relationship resolver: resolves entity references
    let resolver = Arc::new(DefaultRelationshipResolver::new());

    // Create DB executor wrapper for DB2
    tracing::debug!("Creating DB executor for {}...", db2_config.database);
    let db_executor = Arc::new(create_db2_executor(db2_config, credentials).with_context(
        || {
            format!(
                "Phase 2 Failed: Could not connect to DB2 at {}:{}/{}",
                db2_config.host, db2_config.port, db2_config.database
            )
        },
    )?);

    // Create ontology-driven loader
    let loader = OntologyDrivenLoader::new(
        schema_provider,
        type_mapper,
        ddl_generator,
        normalization,
        transformer,
        resolver,
        cache,
        db_executor,
    );

    tracing::info!(
        "Phase 2: Ontology loader ready, loading {} rows",
        rows.len()
    );

    // Load data (auto-creates tables based on ontology)
    let result = loader
        .load_ontology_data(entity_uri, rows, true)
        .await
        .with_context(|| {
            format!(
                "Phase 2 Failed: Failed to load ontology data for entity '{}'",
                entity_uri
            )
        })?;

    tracing::info!(
        "Phase 2 Complete: Successfully loaded {} rows using ontology-driven loader (entity: {})",
        result,
        entity_uri
    );

    Ok(result)
}

/// Create a DB2 executor wrapper that implements the DbExecutor trait
///
/// This wrapper adapts the ODBC DB2 connection to the DbExecutor trait
/// required by the OntologyDrivenLoader.
fn create_db2_executor(
    config: &graphica_core::catalog::types::DB2Config,
    credentials: &graphica_core::catalog::Credentials,
) -> Result<impl crate::workflows::ontology::loader::DbExecutor> {
    // Convert catalog config to loader config
    let loader_config = convert_db2_config(config, credentials)?;

    // Create ODBC connection
    let mut connection = OdbcDB2Connection::connect(&loader_config)
        .map_err(|e| anyhow!("Failed to connect to DB2: {}", e))?;

    // Verify connection
    if !connection.is_alive() {
        return Err(anyhow!("DB2 connection is not alive"));
    }

    tracing::debug!(
        "DB2 executor created: {}:{}/{}",
        config.host,
        config.port,
        config.database
    );

    Ok(DB2ExecutorWrapper {
        connection: std::sync::Mutex::new(connection),
    })
}

/// Wrapper that implements DbExecutor trait for ODBC DB2 connections
struct DB2ExecutorWrapper {
    connection: std::sync::Mutex<OdbcDB2Connection>,
}

#[async_trait::async_trait]
impl crate::workflows::ontology::loader::DbExecutor for DB2ExecutorWrapper {
    async fn execute_ddl(&self, sql: &str) -> Result<()> {
        tracing::debug!("Executing DDL: {}", sql);

        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        // Execute with panic recovery to handle ODBC handle cleanup failures
        let execute_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            connection.execute(sql, &[])
        }));

        match execute_result {
            Ok(Ok(_)) => {
                tracing::debug!("DDL executed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!("DDL execution failed: {}", e);
                Err(anyhow!("Failed to execute DDL: {}", e))
            }
            Err(panic_info) => {
                tracing::error!("ODBC library panicked during DDL execution");
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    tracing::error!("Panic message: {}", s);
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    tracing::error!("Panic message: {}", s);
                }
                Err(anyhow!(
                    "ODBC library panic during DDL execution - handle cleanup failure"
                ))
            }
        }
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        tracing::debug!("Checking if table exists: {}", table_name);

        // Query DB2 system catalog to check if table exists
        let query = format!(
            "SELECT 1 FROM SYSIBM.SYSTABLES WHERE NAME = '{}' FETCH FIRST 1 ROWS ONLY",
            table_name.to_uppercase()
        );

        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        // Execute with panic recovery to handle ODBC handle cleanup failures
        // This is necessary because the ODBC library (odbc-api) can panic
        // during handle cleanup when DB2 returns certain errors (like table not found)
        let execute_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            connection.execute(&query, &[])
        }));

        match execute_result {
            Ok(Ok(count)) => {
                tracing::debug!("Table '{}' exists: {}", table_name, count > 0);
                Ok(count > 0)
            }
            Ok(Err(e)) => {
                // Query error - table likely doesn't exist
                tracing::debug!(
                    "Table '{}' check failed (likely doesn't exist): {}",
                    table_name,
                    e
                );
                Ok(false)
            }
            Err(panic_info) => {
                // ODBC library panicked (likely during handle cleanup after error)
                tracing::warn!(
                    "ODBC library panicked during table existence check for '{}' - assuming table does not exist",
                    table_name
                );
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    tracing::debug!("Panic message: {}", s);
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    tracing::debug!("Panic message: {}", s);
                }
                // Assume table doesn't exist on panic
                Ok(false)
            }
        }
    }

    async fn execute_batch_insert(
        &self,
        sql: &str,
        rows: Vec<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<u64> {
        if rows.is_empty() {
            return Ok(0);
        }

        let mut total_inserted = 0u64;
        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        // `OntologyDrivenLoader::generate_insert_sql` creates a single-row INSERT statement.
        // Execute one statement per row so placeholders and params always match.
        let columns: Vec<String> = rows[0].keys().cloned().collect();
        let total_rows = rows.len();

        for (row_idx, row) in rows.iter().enumerate() {
            let mut row_params: Vec<JsonSqlParam> = Vec::with_capacity(columns.len());

            for col in columns.iter() {
                let sql_param =
                    row.get(col)
                        .map(json_to_sql_param)
                        .unwrap_or_else(|| JsonSqlParam {
                            value: "NULL".to_string(),
                            param_type: SqlParamType::Null,
                        });
                row_params.push(sql_param);
            }

            // Convert to trait object references
            let param_refs: Vec<&dyn SqlParam> =
                row_params.iter().map(|p| p as &dyn SqlParam).collect();

            // Execute with panic recovery to handle ODBC handle cleanup failures
            let execute_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                connection.execute(sql, &param_refs)
            }));

            match execute_result {
                Ok(Ok(_)) => {
                    total_inserted += 1;
                    if (row_idx + 1) % BATCH_SIZE == 0 || row_idx + 1 == total_rows {
                        tracing::debug!(
                            "Ontology insert progress: {}/{} rows",
                            row_idx + 1,
                            total_rows
                        );
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Row {} insert failed: {}", row_idx, e);
                    return Err(anyhow!("Batch insert failed: {}", e));
                }
                Err(panic_info) => {
                    tracing::error!("ODBC library panicked during row {} insert", row_idx);
                    if let Some(s) = panic_info.downcast_ref::<&str>() {
                        tracing::error!("Panic message: {}", s);
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        tracing::error!("Panic message: {}", s);
                    }
                    return Err(anyhow!("ODBC library panic during batch insert"));
                }
            }
        }

        Ok(total_inserted)
    }

    async fn begin_transaction(&self) -> Result<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            connection.begin_transaction()
        }));

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow!("Failed to begin transaction: {}", e)),
            Err(_) => Err(anyhow!("ODBC panic during transaction begin")),
        }
    }

    async fn commit(&self) -> Result<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| connection.commit()));

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow!("Failed to commit transaction: {}", e)),
            Err(_) => Err(anyhow!("ODBC panic during transaction commit")),
        }
    }

    async fn rollback(&self) -> Result<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection: {}", e))?;

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| connection.rollback()));

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow!("Failed to rollback transaction: {}", e)),
            Err(_) => Err(anyhow!("ODBC panic during transaction rollback")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::RdfStore;
    use graphica_core::catalog::Credentials;

    fn single_test_row() -> Vec<serde_json::Map<String, serde_json::Value>> {
        let mut row = serde_json::Map::new();
        row.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );
        vec![row]
    }

    fn dummy_db2_config() -> graphica_core::catalog::types::DB2Config {
        graphica_core::catalog::types::DB2Config {
            host: "localhost".to_string(),
            port: 50000,
            database: "TEST".to_string(),
            schema: None,
        }
    }

    fn dummy_credentials() -> Credentials {
        Credentials::new("db2inst1".to_string(), "secret".to_string())
    }

    #[tokio::test]
    async fn test_phase1_failure_happens_before_db_connect() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().expect("in-memory rdf store"));
        let db2_config = dummy_db2_config();
        let credentials = dummy_credentials();

        let err = use_ontology_driven_loader(
            "http://example.org/NonExistentEntity",
            rdf_store,
            "db2_test",
            "patients",
            &db2_config,
            &credentials,
            single_test_row(),
            "Insert",
        )
        .await
        .expect_err("invalid entity should fail in phase 1");

        let msg = err.to_string();
        assert!(
            msg.contains("Phase 1 Failed"),
            "expected phase 1 error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_phase2_reports_credentials_error_after_phase1_success() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().expect("in-memory rdf store"));
        let entity_uri = "http://example.org/Patient";

        // Minimal ontology signal for phase 1 entity existence check.
        rdf_store
            .insert_triple(
                entity_uri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://www.w3.org/2002/07/owl#Class",
                None,
            )
            .expect("insert class triple");

        let db2_config = dummy_db2_config();
        let credentials = Credentials::new(String::new(), String::new());

        let err = use_ontology_driven_loader(
            entity_uri,
            rdf_store,
            "db2_test",
            "patients",
            &db2_config,
            &credentials,
            single_test_row(),
            "Insert",
        )
        .await
        .expect_err("credential resolution should fail in phase 2");

        let msg = err.to_string();
        assert!(
            msg.contains("DB2 credentials missing"),
            "expected phase 2 credential failure, got: {}",
            msg
        );
    }

    #[test]
    fn test_build_databricks_insert_statement_uses_named_parameters() {
        use crate::common::databricks::build_insert_statement;

        let rows = vec![
            serde_json::Map::from_iter([
                ("id".to_string(), serde_json::json!(1)),
                ("name".to_string(), serde_json::json!("Alice")),
            ]),
            serde_json::Map::from_iter([
                ("id".to_string(), serde_json::json!(2)),
                ("name".to_string(), serde_json::json!("Bob")),
            ]),
        ];
        let columns = vec!["id".to_string(), "name".to_string()];
        let quoted_columns = vec!["`id`".to_string(), "`name`".to_string()];

        let (statement, parameters) = build_insert_statement(
            "`main`.`bronze`.`customers`",
            &columns,
            &quoted_columns,
            &rows,
        )
        .expect("insert statement");

        assert!(
            statement.starts_with("INSERT INTO `main`.`bronze`.`customers` (`id`, `name`) VALUES")
        );
        assert!(statement.contains(":r0_id"));
        assert!(statement.contains(":r1_name"));
        assert_eq!(parameters.get("r0_id"), Some(&serde_json::json!(1)));
        assert_eq!(parameters.get("r1_name"), Some(&serde_json::json!("Bob")));
    }

    #[test]
    fn test_build_databricks_merge_statement_requires_key_fields() {
        use crate::common::databricks::build_merge_statement;

        let rows = vec![serde_json::Map::from_iter([
            ("id".to_string(), serde_json::json!(1)),
            ("name".to_string(), serde_json::json!("Alice")),
        ])];
        let columns = vec!["id".to_string(), "name".to_string()];
        let quoted_columns = vec!["`id`".to_string(), "`name`".to_string()];

        let error = build_merge_statement(
            "`main`.`bronze`.`customers`",
            &columns,
            &quoted_columns,
            &rows,
            None,
        )
        .expect_err("missing key fields should fail");

        assert!(error
            .to_string()
            .contains("requires one or more key fields"));
    }
}
