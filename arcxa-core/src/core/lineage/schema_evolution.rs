//! Schema Evolution Tracking
//!
//! Tracks changes to database schemas, table structures, and column definitions over time.
//! Helps identify breaking changes, schema drift, and migration impact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// =============================================================================
// Core Types
// =============================================================================

/// Schema change event
///
/// Records a single schema change (table/column add/drop/modify)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaChangeEvent {
    /// Unique event ID
    pub id: String,

    /// Datasource ID where change occurred
    pub datasource_id: String,

    /// Schema name (if applicable)
    pub schema_name: Option<String>,

    /// Table name
    pub table_name: String,

    /// Column name (None for table-level changes)
    pub column_name: Option<String>,

    /// Type of change
    pub change_type: SchemaChangeType,

    /// Previous state (before change)
    pub before_state: Option<SchemaElement>,

    /// New state (after change)
    pub after_state: Option<SchemaElement>,

    /// Timestamp when change was detected
    pub detected_at: DateTime<Utc>,

    /// Migration script or DDL that caused the change
    pub migration_script: Option<String>,

    /// Migration ID or version number
    pub migration_id: Option<String>,

    /// User or system that initiated the change
    pub initiated_by: String,

    /// Tenant ID for multi-tenancy
    pub tenant_id: String,

    /// Breaking change flag (requires code changes downstream)
    pub is_breaking: bool,

    /// Impact analysis summary
    pub impact_summary: Option<String>,
}

/// Type of schema change
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchemaChangeType {
    /// Table created
    TableAdded,

    /// Table dropped
    TableDropped,

    /// Table renamed
    TableRenamed { old_name: String, new_name: String },

    /// Column added to table
    ColumnAdded,

    /// Column dropped from table
    ColumnDropped,

    /// Column renamed
    ColumnRenamed { old_name: String, new_name: String },

    /// Column data type changed
    DataTypeChanged { old_type: String, new_type: String },

    /// Column nullability changed
    NullabilityChanged {
        old_nullable: bool,
        new_nullable: bool,
    },

    /// Column default value changed
    DefaultValueChanged {
        old_default: Option<String>,
        new_default: Option<String>,
    },

    /// Primary key added
    PrimaryKeyAdded { columns: Vec<String> },

    /// Primary key dropped
    PrimaryKeyDropped { columns: Vec<String> },

    /// Foreign key added
    ForeignKeyAdded {
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },

    /// Foreign key dropped
    ForeignKeyDropped {
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },

    /// Index added
    IndexAdded {
        index_name: String,
        columns: Vec<String>,
    },

    /// Index dropped
    IndexDropped {
        index_name: String,
        columns: Vec<String>,
    },

    /// Constraint added
    ConstraintAdded {
        constraint_name: String,
        constraint_type: String,
    },

    /// Constraint dropped
    ConstraintDropped {
        constraint_name: String,
        constraint_type: String,
    },
}

/// Schema element (table or column) state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaElement {
    /// Element type (table or column)
    pub element_type: SchemaElementType,

    /// Element name
    pub name: String,

    /// Data type (for columns)
    pub data_type: Option<String>,

    /// Nullable flag (for columns)
    pub nullable: Option<bool>,

    /// Default value (for columns)
    pub default_value: Option<String>,

    /// Column position/ordinal (for columns)
    pub position: Option<i32>,

    /// Comment/description
    pub comment: Option<String>,

    /// Additional properties as JSON
    pub properties: Option<serde_json::Value>,
}

/// Schema element type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchemaElementType {
    /// Table
    Table,

    /// Column
    Column,

    /// Index
    Index,

    /// Constraint
    Constraint,
}

// =============================================================================
// Schema Version Types
// =============================================================================

/// Schema version snapshot
///
/// Captures the complete schema state at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaVersion {
    /// Version ID
    pub version_id: String,

    /// Datasource ID
    pub datasource_id: String,

    /// Schema name (if applicable)
    pub schema_name: Option<String>,

    /// Timestamp of this version
    pub created_at: DateTime<Utc>,

    /// Migration ID that created this version
    pub migration_id: Option<String>,

    /// All tables in this version
    pub tables: Vec<TableSchema>,

    /// Previous version ID (for linked list traversal)
    pub previous_version: Option<String>,

    /// Git commit hash (if schema is in version control)
    pub git_commit: Option<String>,

    /// Tags for this version (e.g., "production", "v1.2.3")
    pub tags: Vec<String>,
}

/// Table schema definition
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TableSchema {
    /// Table name
    pub name: String,

    /// All columns
    pub columns: Vec<ColumnSchema>,

    /// Primary key columns
    pub primary_key: Option<Vec<String>>,

    /// Foreign keys
    pub foreign_keys: Vec<ForeignKeySchema>,

    /// Indexes
    pub indexes: Vec<IndexSchema>,

    /// Table comment
    pub comment: Option<String>,
}

/// Column schema definition
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnSchema {
    /// Column name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Nullable
    pub nullable: bool,

    /// Default value
    pub default_value: Option<String>,

    /// Column position
    pub position: i32,

    /// Column comment
    pub comment: Option<String>,
}

/// Foreign key schema
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForeignKeySchema {
    /// Foreign key name
    pub name: String,

    /// Local columns
    pub columns: Vec<String>,

    /// Referenced table
    pub referenced_table: String,

    /// Referenced columns
    pub referenced_columns: Vec<String>,
}

/// Index schema
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IndexSchema {
    /// Index name
    pub name: String,

    /// Indexed columns
    pub columns: Vec<String>,

    /// Unique index flag
    pub unique: bool,

    /// Index type (btree, hash, etc.)
    pub index_type: Option<String>,
}

// =============================================================================
// Analysis Types
// =============================================================================

/// Schema drift analysis
///
/// Compares two schema versions and identifies drift
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaDriftAnalysis {
    /// Source version (baseline)
    pub source_version_id: String,

    /// Target version (current)
    pub target_version_id: String,

    /// All detected changes
    pub changes: Vec<SchemaChangeEvent>,

    /// Breaking changes count
    pub breaking_changes_count: usize,

    /// Non-breaking changes count
    pub non_breaking_changes_count: usize,

    /// Tables added
    pub tables_added: Vec<String>,

    /// Tables dropped
    pub tables_dropped: Vec<String>,

    /// Tables modified
    pub tables_modified: Vec<String>,

    /// Drift severity
    pub severity: DriftSeverity,

    /// Analysis timestamp
    pub analyzed_at: DateTime<Utc>,
}

/// Drift severity level
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriftSeverity {
    /// No changes detected
    None,

    /// Minor non-breaking changes
    Low,

    /// Non-breaking changes with impact
    Medium,

    /// Breaking changes detected
    High,

    /// Critical breaking changes (data loss risk)
    Critical,
}

/// Schema migration impact analysis
///
/// Analyzes the downstream impact of a schema change
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MigrationImpactAnalysis {
    /// Change being analyzed
    pub change: SchemaChangeEvent,

    /// Affected queries (from query logs)
    pub affected_queries: Vec<String>,

    /// Affected ETL jobs
    pub affected_jobs: Vec<String>,

    /// Affected workflows
    pub affected_workflows: Vec<String>,

    /// Affected dashboards/reports
    pub affected_dashboards: Vec<String>,

    /// Estimated impact (number of systems affected)
    pub impact_score: f64,

    /// Recommended migration steps
    pub migration_steps: Vec<String>,

    /// Risk level
    pub risk_level: RiskLevel,
}

/// Risk level for migrations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    /// No risk
    None,

    /// Low risk (easily reversible)
    Low,

    /// Medium risk (requires testing)
    Medium,

    /// High risk (potential data issues)
    High,

    /// Critical risk (data loss possible)
    Critical,
}

// =============================================================================
// Helper Functions
// =============================================================================

impl SchemaChangeEvent {
    /// Create a new schema change event
    pub fn new(
        datasource_id: impl Into<String>,
        table_name: impl Into<String>,
        change_type: SchemaChangeType,
        initiated_by: impl Into<String>,
        tenant_id: impl Into<String>,
    ) -> Self {
        let is_breaking = Self::is_change_breaking(&change_type);

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            datasource_id: datasource_id.into(),
            schema_name: None,
            table_name: table_name.into(),
            column_name: None,
            change_type,
            before_state: None,
            after_state: None,
            detected_at: Utc::now(),
            migration_script: None,
            migration_id: None,
            initiated_by: initiated_by.into(),
            tenant_id: tenant_id.into(),
            is_breaking,
            impact_summary: None,
        }
    }

    /// Determine if a change type is breaking
    fn is_change_breaking(change_type: &SchemaChangeType) -> bool {
        matches!(
            change_type,
            SchemaChangeType::TableDropped
                | SchemaChangeType::ColumnDropped
                | SchemaChangeType::DataTypeChanged { .. }
                | SchemaChangeType::NullabilityChanged {
                    new_nullable: false,
                    ..
                }
                | SchemaChangeType::PrimaryKeyDropped { .. }
                | SchemaChangeType::ForeignKeyDropped { .. }
        )
    }

    /// Set column name
    pub fn with_column(mut self, column_name: impl Into<String>) -> Self {
        self.column_name = Some(column_name.into());
        self
    }

    /// Set schema name
    pub fn with_schema(mut self, schema_name: impl Into<String>) -> Self {
        self.schema_name = Some(schema_name.into());
        self
    }

    /// Set before state
    pub fn with_before_state(mut self, state: SchemaElement) -> Self {
        self.before_state = Some(state);
        self
    }

    /// Set after state
    pub fn with_after_state(mut self, state: SchemaElement) -> Self {
        self.after_state = Some(state);
        self
    }

    /// Set migration info
    pub fn with_migration(
        mut self,
        migration_id: impl Into<String>,
        script: impl Into<String>,
    ) -> Self {
        self.migration_id = Some(migration_id.into());
        self.migration_script = Some(script.into());
        self
    }
}

impl SchemaElement {
    /// Create a column element
    pub fn column(name: impl Into<String>, data_type: impl Into<String>, nullable: bool) -> Self {
        Self {
            element_type: SchemaElementType::Column,
            name: name.into(),
            data_type: Some(data_type.into()),
            nullable: Some(nullable),
            default_value: None,
            position: None,
            comment: None,
            properties: None,
        }
    }

    /// Create a table element
    pub fn table(name: impl Into<String>) -> Self {
        Self {
            element_type: SchemaElementType::Table,
            name: name.into(),
            data_type: None,
            nullable: None,
            default_value: None,
            position: None,
            comment: None,
            properties: None,
        }
    }
}
