//! Core types for unified mapping sessions
//!
//! This module defines the data structures for consolidating multiple
//! source mapping sessions into a single unified mapping that targets
//! a normalized relational database schema.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Target database configuration for unified mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TargetDatabaseConfig {
    /// Data source ID of the target database
    pub datasource_id: String,

    /// Schema name (e.g., "public" for PostgreSQL)
    pub schema: String,

    /// Table definitions in the target schema
    pub tables: HashMap<String, TargetTableConfig>,
}

/// Target table configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TargetTableConfig {
    /// Table name
    pub name: String,

    /// Column definitions
    pub columns: HashMap<String, TargetColumnConfig>,

    /// Primary key column names
    pub primary_keys: Vec<String>,

    /// Foreign key relationships
    pub foreign_keys: Vec<ForeignKeyConfig>,
}

impl TargetTableConfig {
    /// Validate all identifiers and nested configs in this table config
    ///
    /// # Security
    ///
    /// Prevents SQL injection by validating all table names, column names,
    /// primary keys, and foreign keys before use in SQL construction.
    ///
    /// This is the primary defense against config-based SQL injection attacks.
    ///
    /// # Errors
    ///
    /// Returns error if any identifier is invalid or nested validation fails
    pub fn validate(&self) -> anyhow::Result<()> {
        use graphica_core::security::validate_identifier;

        // Validate table name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid table name '{}': {}", self.name, e))?;

        // Validate all column configs (both HashMap keys and column names)
        for (col_key, col_config) in &self.columns {
            // Validate HashMap key
            validate_identifier(col_key).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid column key '{}' in table '{}': {}",
                    col_key,
                    self.name,
                    e
                )
            })?;

            // Validate the column config itself (name, data_type)
            col_config.validate().map_err(|e| {
                anyhow::anyhow!(
                    "Invalid column config for key '{}' in table '{}': {}",
                    col_key,
                    self.name,
                    e
                )
            })?;

            // Ensure HashMap key matches column name (consistency check)
            if col_key != &col_config.name {
                return Err(anyhow::anyhow!(
                    "Column key '{}' does not match column name '{}' in table '{}'",
                    col_key,
                    col_config.name,
                    self.name
                ));
            }
        }

        // Validate all primary key column names
        for pk in &self.primary_keys {
            validate_identifier(pk).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid primary key column '{}' in table '{}': {}",
                    pk,
                    self.name,
                    e
                )
            })?;

            // Ensure primary key exists in columns
            if !self.columns.contains_key(pk) {
                return Err(anyhow::anyhow!(
                    "Primary key column '{}' not found in table '{}' columns",
                    pk,
                    self.name
                ));
            }
        }

        // Validate all foreign key configs
        for (idx, fk) in self.foreign_keys.iter().enumerate() {
            fk.validate().map_err(|e| {
                anyhow::anyhow!(
                    "Invalid foreign key #{} in table '{}': {}",
                    idx + 1,
                    self.name,
                    e
                )
            })?;

            // Ensure FK column exists in this table's columns
            if !self.columns.contains_key(&fk.column) {
                return Err(anyhow::anyhow!(
                    "Foreign key column '{}' not found in table '{}' columns",
                    fk.column,
                    self.name
                ));
            }
        }

        Ok(())
    }
}

/// Target column configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TargetColumnConfig {
    /// Column name
    pub name: String,

    /// SQL data type (e.g., "VARCHAR(255)", "INTEGER")
    pub data_type: String,

    /// Whether the column is nullable
    pub nullable: bool,

    /// Whether this column is part of a primary key
    pub is_primary_key: bool,

    /// Default value if any
    pub default_value: Option<String>,
}

impl TargetColumnConfig {
    /// Validate all identifiers and types in this column config
    ///
    /// # Security
    ///
    /// Prevents SQL injection by validating column name and data type
    /// before use in SQL construction.
    ///
    /// # Errors
    ///
    /// Returns error if column name or data type is invalid
    pub fn validate(&self) -> anyhow::Result<()> {
        use graphica_core::security::{validate_identifier, validate_sql_type};

        // Validate column name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid column name '{}': {}", self.name, e))?;

        // Validate SQL data type
        validate_sql_type(&self.data_type)
            .map_err(|e| anyhow::anyhow!("Invalid SQL data type '{}': {}", self.data_type, e))?;

        // Note: default_value is not validated here as it may be complex
        // (NULL, expressions, etc.). It should be handled carefully when
        // constructing SQL, potentially using parameterized queries.

        Ok(())
    }
}

/// Foreign key relationship
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ForeignKeyConfig {
    /// Column name in this table
    pub column: String,

    /// Referenced table name
    pub references_table: String,

    /// Referenced column name
    pub references_column: String,

    /// ON DELETE behavior (e.g., "CASCADE", "SET NULL")
    pub on_delete: Option<String>,
}

impl ForeignKeyConfig {
    /// Validate all identifiers and actions in this foreign key config
    ///
    /// # Security
    ///
    /// Prevents SQL injection by validating all table and column names
    /// and foreign key actions before use in SQL construction.
    ///
    /// # Errors
    ///
    /// Returns error if any identifier or action is invalid
    pub fn validate(&self) -> anyhow::Result<()> {
        use graphica_core::security::{validate_fk_action, validate_identifier};

        // Validate column name
        validate_identifier(&self.column).map_err(|e| {
            anyhow::anyhow!("Invalid foreign key column name '{}': {}", self.column, e)
        })?;

        // Validate referenced table name
        validate_identifier(&self.references_table).map_err(|e| {
            anyhow::anyhow!(
                "Invalid referenced table name '{}': {}",
                self.references_table,
                e
            )
        })?;

        // Validate referenced column name
        validate_identifier(&self.references_column).map_err(|e| {
            anyhow::anyhow!(
                "Invalid referenced column name '{}': {}",
                self.references_column,
                e
            )
        })?;

        // Validate ON DELETE action if present
        if let Some(ref action) = self.on_delete {
            validate_fk_action(action)
                .map_err(|e| anyhow::anyhow!("Invalid ON DELETE action '{}': {}", action, e))?;
        }

        Ok(())
    }
}

/// Unified field mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct UnifiedFieldMapping {
    /// Unique ID for this mapping
    pub id: String,

    /// Source field identifiers (may be multiple if consolidating)
    pub source_fields: Vec<SourceFieldRef>,

    /// Ontology term URI that connects source to target
    pub ontology_term_uri: String,

    /// Target database column
    pub target_column: TargetColumnRef,

    /// Conflict resolution strategy if multiple sources map to same target
    pub conflict_resolution: ConflictResolution,

    /// Optional transformation to apply
    pub transformation: Option<String>,

    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
}

/// Reference to a source field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SourceFieldRef {
    /// Source mapping session ID
    pub session_id: String,

    /// Data source ID
    pub datasource_id: String,

    /// Table name (for CSV, this is the file name)
    pub table_name: String,

    /// Field/column name
    pub field_name: String,

    /// Original data type from source
    pub source_data_type: String,
}

/// Reference to a target database column
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TargetColumnRef {
    /// Table name
    pub table_name: String,

    /// Column name
    pub column_name: String,

    /// SQL data type
    pub data_type: String,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Use the first source (primary)
    UsePrimary {
        /// Which source field is primary
        primary_source: String,
    },

    /// Merge values (concatenate with separator)
    Merge {
        /// Separator string
        separator: String,
    },

    /// Take first non-null value
    Coalesce,

    /// Use custom transformation rule
    CustomRule {
        /// Rule expression
        rule: String,
    },

    /// No conflict (single source)
    NoConflict,
}

/// Unified mapping session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedSessionStatus {
    /// Session created, building mappings
    Building,

    /// Conflicts detected, needs resolution
    ConflictsDetected,

    /// All conflicts resolved, ready to load
    ReadyToLoad,

    /// Loading to target database in progress
    Loading,

    /// Load completed successfully
    Completed,

    /// Error occurred
    Failed {
        /// Error message
        error: String,
    },
}

/// Unified mapping session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnifiedMappingSession {
    /// Unique session ID
    pub id: String,

    /// Source mapping session IDs being consolidated
    pub source_sessions: Vec<String>,

    /// Target database configuration
    pub target_database: TargetDatabaseConfig,

    /// Unified field mappings
    pub field_mappings: Vec<UnifiedFieldMapping>,

    /// Detected conflicts that need resolution
    pub conflicts: Vec<MappingConflict>,

    /// Current status
    pub status: UnifiedSessionStatus,

    /// Created timestamp (Unix epoch seconds)
    pub created_at: i64,

    /// Created by user ID
    pub created_by: String,

    /// Last updated timestamp
    pub updated_at: i64,
}

/// Detected mapping conflict
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct MappingConflict {
    /// Conflict ID
    pub id: String,

    /// Ontology term that has conflicting sources
    pub ontology_term_uri: String,

    /// Source fields that conflict
    pub conflicting_sources: Vec<SourceFieldRef>,

    /// Target column they both map to
    pub target_column: TargetColumnRef,

    /// Suggested resolution
    pub suggested_resolution: ConflictResolution,

    /// Whether conflict is resolved
    pub resolved: bool,
}

/// Lifecycle status for a unified load job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedLoadJobStatus {
    Queued,
    Running,
    Submitted,
    Completed,
    Failed,
    Cancelled,
}

/// Progress information for a unified load job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct UnifiedLoadProgress {
    pub total_rows: usize,
    pub rows_processed: usize,
    pub rows_succeeded: usize,
    pub rows_failed: usize,
    pub percentage_complete: f64,
}

impl Default for UnifiedLoadProgress {
    fn default() -> Self {
        Self {
            total_rows: 0,
            rows_processed: 0,
            rows_succeeded: 0,
            rows_failed: 0,
            percentage_complete: 0.0,
        }
    }
}

/// Persistent unified load job record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct UnifiedLoadJob {
    pub id: String,
    pub session_id: String,
    pub database_type: String,
    pub status: UnifiedLoadJobStatus,
    pub progress: UnifiedLoadProgress,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_message: Option<String>,
    pub external_run_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_database_config_creation() {
        let mut tables = HashMap::new();
        tables.insert(
            "customers".to_string(),
            TargetTableConfig {
                name: "customers".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: vec![],
            },
        );

        let config = TargetDatabaseConfig {
            datasource_id: "prod_postgres".to_string(),
            schema: "public".to_string(),
            tables,
        };

        assert_eq!(config.datasource_id, "prod_postgres");
        assert_eq!(config.schema, "public");
        assert_eq!(config.tables.len(), 1);
        assert!(config.tables.contains_key("customers"));
    }

    #[test]
    fn test_target_column_config_with_defaults() {
        let column = TargetColumnConfig {
            name: "email".to_string(),
            data_type: "VARCHAR(255)".to_string(),
            nullable: false,
            is_primary_key: false,
            default_value: None,
        };

        assert_eq!(column.name, "email");
        assert_eq!(column.data_type, "VARCHAR(255)");
        assert!(!column.nullable);
        assert!(!column.is_primary_key);
        assert!(column.default_value.is_none());
    }

    #[test]
    fn test_foreign_key_config() {
        let fk = ForeignKeyConfig {
            column: "customer_id".to_string(),
            references_table: "customers".to_string(),
            references_column: "id".to_string(),
            on_delete: Some("CASCADE".to_string()),
        };

        assert_eq!(fk.column, "customer_id");
        assert_eq!(fk.references_table, "customers");
        assert_eq!(fk.references_column, "id");
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[test]
    fn test_source_field_ref() {
        let source = SourceFieldRef {
            session_id: "session_001".to_string(),
            datasource_id: "csv_customers".to_string(),
            table_name: "customers.csv".to_string(),
            field_name: "email".to_string(),
            source_data_type: "TEXT".to_string(),
        };

        assert_eq!(source.session_id, "session_001");
        assert_eq!(source.datasource_id, "csv_customers");
        assert_eq!(source.table_name, "customers.csv");
        assert_eq!(source.field_name, "email");
    }

    #[test]
    fn test_target_column_ref() {
        let target = TargetColumnRef {
            table_name: "customers".to_string(),
            column_name: "email_address".to_string(),
            data_type: "VARCHAR(255)".to_string(),
        };

        assert_eq!(target.table_name, "customers");
        assert_eq!(target.column_name, "email_address");
        assert_eq!(target.data_type, "VARCHAR(255)");
    }

    #[test]
    fn test_conflict_resolution_use_primary() {
        let resolution = ConflictResolution::UsePrimary {
            primary_source: "csv1.email".to_string(),
        };

        match resolution {
            ConflictResolution::UsePrimary { primary_source } => {
                assert_eq!(primary_source, "csv1.email");
            }
            _ => panic!("Wrong conflict resolution type"),
        }
    }

    #[test]
    fn test_conflict_resolution_merge() {
        let resolution = ConflictResolution::Merge {
            separator: "; ".to_string(),
        };

        match resolution {
            ConflictResolution::Merge { separator } => {
                assert_eq!(separator, "; ");
            }
            _ => panic!("Wrong conflict resolution type"),
        }
    }

    #[test]
    fn test_conflict_resolution_coalesce() {
        let resolution = ConflictResolution::Coalesce;
        assert!(matches!(resolution, ConflictResolution::Coalesce));
    }

    #[test]
    fn test_conflict_resolution_custom_rule() {
        let resolution = ConflictResolution::CustomRule {
            rule: "UPPER(COALESCE(src1, src2))".to_string(),
        };

        match resolution {
            ConflictResolution::CustomRule { rule } => {
                assert_eq!(rule, "UPPER(COALESCE(src1, src2))");
            }
            _ => panic!("Wrong conflict resolution type"),
        }
    }

    #[test]
    fn test_unified_field_mapping_no_conflict() {
        let mapping = UnifiedFieldMapping {
            id: "mapping_001".to_string(),
            source_fields: vec![SourceFieldRef {
                session_id: "session_001".to_string(),
                datasource_id: "csv_customers".to_string(),
                table_name: "customers.csv".to_string(),
                field_name: "email".to_string(),
                source_data_type: "TEXT".to_string(),
            }],
            ontology_term_uri: "http://schema.org/email".to_string(),
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            conflict_resolution: ConflictResolution::NoConflict,
            transformation: None,
            confidence: 0.95,
        };

        assert_eq!(mapping.id, "mapping_001");
        assert_eq!(mapping.source_fields.len(), 1);
        assert_eq!(mapping.ontology_term_uri, "http://schema.org/email");
        assert_eq!(mapping.confidence, 0.95);
        assert!(matches!(
            mapping.conflict_resolution,
            ConflictResolution::NoConflict
        ));
    }

    #[test]
    fn test_unified_field_mapping_with_conflict() {
        let mapping = UnifiedFieldMapping {
            id: "mapping_002".to_string(),
            source_fields: vec![
                SourceFieldRef {
                    session_id: "session_001".to_string(),
                    datasource_id: "csv1".to_string(),
                    table_name: "file1.csv".to_string(),
                    field_name: "email".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
                SourceFieldRef {
                    session_id: "session_002".to_string(),
                    datasource_id: "csv2".to_string(),
                    table_name: "file2.csv".to_string(),
                    field_name: "customer_email".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
            ],
            ontology_term_uri: "http://schema.org/email".to_string(),
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            conflict_resolution: ConflictResolution::UsePrimary {
                primary_source: "csv1.email".to_string(),
            },
            transformation: None,
            confidence: 0.85,
        };

        assert_eq!(mapping.source_fields.len(), 2);
        assert_eq!(mapping.confidence, 0.85);
    }

    #[test]
    fn test_mapping_conflict_detection() {
        let conflict = MappingConflict {
            id: "conflict_001".to_string(),
            ontology_term_uri: "http://schema.org/email".to_string(),
            conflicting_sources: vec![
                SourceFieldRef {
                    session_id: "session_001".to_string(),
                    datasource_id: "csv1".to_string(),
                    table_name: "file1.csv".to_string(),
                    field_name: "email".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
                SourceFieldRef {
                    session_id: "session_002".to_string(),
                    datasource_id: "csv2".to_string(),
                    table_name: "file2.csv".to_string(),
                    field_name: "customer_email".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
            ],
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            suggested_resolution: ConflictResolution::UsePrimary {
                primary_source: "csv1.email".to_string(),
            },
            resolved: false,
        };

        assert_eq!(conflict.id, "conflict_001");
        assert_eq!(conflict.conflicting_sources.len(), 2);
        assert!(!conflict.resolved);
    }

    #[test]
    fn test_unified_session_status_transitions() {
        let statuses = vec![
            UnifiedSessionStatus::Building,
            UnifiedSessionStatus::ConflictsDetected,
            UnifiedSessionStatus::ReadyToLoad,
            UnifiedSessionStatus::Loading,
            UnifiedSessionStatus::Completed,
        ];

        for status in statuses {
            match status {
                UnifiedSessionStatus::Building => assert!(true),
                UnifiedSessionStatus::ConflictsDetected => assert!(true),
                UnifiedSessionStatus::ReadyToLoad => assert!(true),
                UnifiedSessionStatus::Loading => assert!(true),
                UnifiedSessionStatus::Completed => assert!(true),
                _ => panic!("Unexpected status"),
            }
        }
    }

    #[test]
    fn test_unified_session_status_failed() {
        let status = UnifiedSessionStatus::Failed {
            error: "Database connection failed".to_string(),
        };

        match status {
            UnifiedSessionStatus::Failed { error } => {
                assert_eq!(error, "Database connection failed");
            }
            _ => panic!("Wrong status type"),
        }
    }

    #[test]
    fn test_unified_mapping_session_creation() {
        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["session_001".to_string(), "session_002".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "prod_postgres".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            },
            field_mappings: vec![],
            conflicts: vec![],
            status: UnifiedSessionStatus::Building,
            created_at: 1697356800,
            created_by: "user_123".to_string(),
            updated_at: 1697356800,
        };

        assert_eq!(session.id, "unified_001");
        assert_eq!(session.source_sessions.len(), 2);
        assert_eq!(session.field_mappings.len(), 0);
        assert_eq!(session.conflicts.len(), 0);
        assert!(matches!(session.status, UnifiedSessionStatus::Building));
    }

    #[test]
    fn test_serde_serialization_target_database_config() {
        let config = TargetDatabaseConfig {
            datasource_id: "prod_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TargetDatabaseConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_serde_serialization_unified_field_mapping() {
        let mapping = UnifiedFieldMapping {
            id: "mapping_001".to_string(),
            source_fields: vec![],
            ontology_term_uri: "http://schema.org/name".to_string(),
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "name".to_string(),
                data_type: "VARCHAR(100)".to_string(),
            },
            conflict_resolution: ConflictResolution::NoConflict,
            transformation: None,
            confidence: 0.9,
        };

        let json = serde_json::to_string(&mapping).unwrap();
        let deserialized: UnifiedFieldMapping = serde_json::from_str(&json).unwrap();

        assert_eq!(mapping, deserialized);
    }

    #[test]
    fn test_serde_serialization_conflict_resolution() {
        let resolutions = vec![
            ConflictResolution::NoConflict,
            ConflictResolution::Coalesce,
            ConflictResolution::UsePrimary {
                primary_source: "src1".to_string(),
            },
            ConflictResolution::Merge {
                separator: ", ".to_string(),
            },
            ConflictResolution::CustomRule {
                rule: "CONCAT(a, b)".to_string(),
            },
        ];

        for resolution in resolutions {
            let json = serde_json::to_string(&resolution).unwrap();
            let deserialized: ConflictResolution = serde_json::from_str(&json).unwrap();
            assert_eq!(resolution, deserialized);
        }
    }
}
