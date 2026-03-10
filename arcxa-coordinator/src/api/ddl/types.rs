//! DDL API Types
//!
//! Request and response DTOs for DDL generation API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to generate DDL from SHACL
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateDdlRequest {
    /// SHACL shape URI to generate DDL from
    pub shacl_uri: String,

    /// SQL dialect (postgresql, db2, oracle)
    pub dialect: String,

    /// Whether to include indexes in the generated DDL
    #[serde(default = "default_true")]
    pub include_indexes: bool,

    /// Whether to include foreign keys in the generated DDL
    #[serde(default = "default_true")]
    pub include_foreign_keys: bool,

    /// Whether to make the DDL idempotent (IF NOT EXISTS checks)
    #[serde(default)]
    pub idempotent: bool,
}

/// Response from DDL generation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateDdlResponse {
    /// Generated DDL statements
    pub ddl_statements: Vec<String>,

    /// Number of tables generated
    pub tables_generated: usize,

    /// SQL dialect used
    pub dialect: String,

    /// Full SQL script
    pub sql_script: String,
}

/// Request to generate schema migration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateMigrationRequest {
    /// Current SHACL shape URIs (empty for new schema)
    pub from_shacl: Vec<String>,

    /// Desired SHACL shape URIs
    pub to_shacl: Vec<String>,

    /// SQL dialect (postgresql, db2, oracle)
    pub dialect: String,

    /// Whether to generate idempotent migration
    #[serde(default = "default_true")]
    pub idempotent: bool,
}

/// Response from migration generation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateMigrationResponse {
    /// Migration SQL statements
    pub migration_sql: Vec<String>,

    /// Whether the migration is safe (no data loss)
    pub safe: bool,

    /// Warnings about unsafe operations
    pub warnings: Vec<String>,

    /// Number of migration steps
    pub steps: usize,

    /// Full migration script
    pub migration_script: String,
}

/// Request to validate DDL
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateDdlRequest {
    /// DDL SQL to validate
    pub ddl_sql: String,

    /// SQL dialect (postgresql, db2, oracle)
    pub dialect: String,
}

/// Response from DDL validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateDdlResponse {
    /// Whether the DDL is syntactically valid
    pub valid: bool,

    /// Validation errors (if any)
    pub errors: Vec<String>,

    /// Validation warnings
    pub warnings: Vec<String>,
}

/// List all SHACL shapes available for DDL generation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListShapesRequest {
    /// Optional filter by target class prefix
    pub target_class_prefix: Option<String>,
}

/// Response with list of SHACL shapes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListShapesResponse {
    /// Available SHACL shape URIs
    pub shapes: Vec<ShapeInfo>,
}

/// SHACL shape metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShapeInfo {
    /// Shape URI
    pub uri: String,

    /// Target class
    pub target_class: String,

    /// Human-readable label
    pub label: Option<String>,

    /// Number of properties
    pub property_count: usize,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// DDL Execution Types (NEW for automatic execution)
// ============================================================================

/// Database connection configuration for DDL execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatabaseConnectionConfig {
    /// Database type
    pub db_type: DatabaseType,

    /// Database host
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password (should use secrets management in production)
    pub password: String,

    /// Additional connection options
    #[serde(default)]
    pub options: std::collections::HashMap<String, String>,
}

/// Database type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    DB2,
    PostgreSQL,
    Oracle,
    MySQL,
}

/// Request to execute DDL statements
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteDdlRequest {
    /// DDL statements to execute
    pub ddl_statements: Vec<String>,

    /// Database connection configuration
    pub database_config: DatabaseConnectionConfig,

    /// Whether to execute in a transaction (rollback all if any fails)
    #[serde(default = "default_true")]
    pub transactional: bool,

    /// Whether to continue on error (only if not transactional)
    #[serde(default)]
    pub continue_on_error: bool,

    /// Optional SHACL shape URI for lineage tracking
    pub shacl_uri: Option<String>,
}

/// Response from DDL execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteDdlResponse {
    /// Whether all statements executed successfully
    pub success: bool,

    /// Number of statements executed successfully
    pub statements_executed: usize,

    /// Number of tables created/modified
    pub tables_affected: usize,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Execution errors (if any)
    pub errors: Vec<DdlExecutionError>,

    /// Success message
    pub message: String,
}

/// DDL execution error details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DdlExecutionError {
    /// Index of the statement that failed
    pub statement_index: usize,

    /// The DDL statement that failed
    pub statement: String,

    /// Error message
    pub error: String,

    /// SQL state code (if available)
    pub sql_state: Option<String>,
}
