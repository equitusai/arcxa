//! Core data types for ETL orchestration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified mapping session ID
pub type UnifiedSessionId = String;

/// Source mapping session ID
pub type SourceSessionId = String;

/// Target database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetDatabase {
    /// Database type (PostgreSQL, DB2, Oracle)
    pub database_type: String,

    /// Connection string or datasource ID
    pub connection: TargetConnection,

    /// Target schema name (optional)
    pub schema: Option<String>,
}

/// Target database connection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TargetConnection {
    /// Reference to existing datasource
    DataSourceRef { source_id: String },

    /// Direct connection string
    ConnectionString { connection_string: String },
}

/// Target table schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetTableSchema {
    /// Table name
    pub table_name: String,

    /// Column definitions (column_name -> SQL type)
    pub columns: HashMap<String, ColumnDefinition>,

    /// Primary key columns
    pub primary_keys: Vec<String>,

    /// Foreign key constraints
    pub foreign_keys: Vec<ForeignKeyConstraint>,
}

/// Column definition in target schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// SQL data type
    pub data_type: String,

    /// Whether column is nullable
    pub nullable: bool,

    /// Whether column is unique
    pub unique: bool,

    /// Default value expression
    pub default: Option<String>,
}

/// Foreign key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyConstraint {
    /// Column name in this table
    pub column: String,

    /// Referenced table
    pub references_table: String,

    /// Referenced column
    pub references_column: String,
}

/// Mapping rule from ontology term to target column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetMappingRule {
    /// Ontology term URI
    pub ontology_term: String,

    /// Target table name
    pub target_table: String,

    /// Target column name
    pub target_column: String,

    /// Transformation expression (SQL-like)
    pub transformation: Option<String>,

    /// Whether this mapping is required
    pub required: bool,

    /// Source fields that contribute to this mapping
    pub source_fields: Vec<SourceFieldMapping>,
}

/// Source field mapping (CSV field → ontology term)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFieldMapping {
    /// Source session ID
    pub session_id: String,

    /// Datasource ID (for CSV file)
    pub datasource_id: String,

    /// CSV field name
    pub csv_field: String,

    /// Table name in CSV (usually "data" for flat files)
    pub table_name: String,

    /// Ontology term this field maps to
    pub ontology_term: String,

    /// Field-level transformation (applied before target transformation)
    pub field_transformation: Option<String>,
}

/// Status of unified mapping session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedMappingStatus {
    /// Pending user review
    PendingReview,

    /// Review completed, ready to apply
    Reviewed,

    /// Mappings applied and active
    Active,

    /// Session failed
    Failed,
}

/// Mapping conflict between sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConflictInfo {
    /// Ontology term causing conflict
    pub ontology_term: String,

    /// Conflicting source fields
    pub conflicting_fields: Vec<ConflictingField>,

    /// Suggested target column
    pub suggested_target_column: String,

    /// Suggested resolution strategy
    pub suggested_resolution: String,
}

/// Field involved in a conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictingField {
    /// Source session ID
    pub session_id: String,

    /// Table name in source
    pub table_name: String,

    /// Field name in source
    pub field_name: String,

    /// Data type
    pub data_type: String,
}

/// Load configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    /// User ID executing the load
    pub user_id: String,

    /// Batch size for processing
    pub batch_size: usize,

    /// Whether to respect entity fusion
    pub respect_fusion: bool,

    /// Dry run mode (validate without loading)
    pub dry_run: bool,

    /// Load mode (Insert, Upsert, Replace)
    pub load_mode: crate::etl::loaders::database::LoadMode,

    /// Key fields for upsert (if using Upsert mode)
    pub key_fields: Option<Vec<String>>,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            batch_size: 1000,
            respect_fusion: true,
            dry_run: false,
            load_mode: crate::etl::loaders::database::LoadMode::Insert,
            key_fields: None,
        }
    }
}

/// Load execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadExecutionStats {
    /// Total CSV records read
    pub csv_records_read: u64,

    /// Entities processed (after fusion)
    pub entities_processed: u64,

    /// Entities skipped due to fusion
    pub fused_entities_skipped: u64,

    /// Database rows inserted
    pub db_rows_inserted: u64,

    /// Errors encountered
    pub errors_count: u64,

    /// Error details
    pub errors: Vec<LoadError>,

    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl LoadExecutionStats {
    pub fn new() -> Self {
        Self {
            csv_records_read: 0,
            entities_processed: 0,
            fused_entities_skipped: 0,
            db_rows_inserted: 0,
            errors_count: 0,
            errors: Vec::new(),
            duration_ms: 0,
        }
    }
}

/// Load error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadError {
    /// Error message
    pub message: String,

    /// Source session ID
    pub source_session: Option<String>,

    /// Table name
    pub table: Option<String>,

    /// Record index
    pub record_index: Option<usize>,

    /// Field name
    pub field: Option<String>,
}

/// Field lineage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLineageInfo {
    /// Target field (table.column)
    pub target_field: String,

    /// Source CSV files and fields
    pub sources: Vec<SourceFieldInfo>,

    /// Ontology term
    pub ontology_term: String,

    /// Transformation applied
    pub transformation: Option<String>,

    /// Fusion operations count
    pub fusion_operations_count: u64,
}

/// Source field information in lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFieldInfo {
    /// CSV file path
    pub csv_file: String,

    /// CSV field name
    pub csv_field: String,

    /// Source session ID
    pub session_id: String,

    /// Records contributed
    pub records_contributed: u64,
}
