//! PostgreSQL Bulk Loader
//!
//! This module provides functionality for bulk loading data from CSV files
//! into PostgreSQL databases using unified mapping sessions.
//!
//! Features:
//! - DDL generation (CREATE TABLE statements)
//! - DML generation (INSERT statements with conflict resolution)
//! - Batch execution for performance
//! - Transaction management
//! - Lineage tracking via RDF triples

use crate::mapping::multi_source::conflict::{ConflictResolver, ResolvedValue, SourceValue};
use crate::mapping::multi_source::types::{
    ConflictResolution, TargetColumnRef, TargetDatabaseConfig, TargetTableConfig,
    UnifiedFieldMapping, UnifiedMappingSession,
};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// PostgreSQL bulk loader for mapping workflows
///
/// This loader integrates with unified mapping sessions and provides:
/// - DDL generation (CREATE TABLE statements)
/// - DML generation (INSERT statements with conflict resolution)
/// - Batch execution for performance
/// - Transaction management
/// - Lineage tracking
pub struct MappingPostgresLoader {
    /// Database connection configuration
    config: LoaderConfig,

    /// Conflict resolver for handling field conflicts
    resolver: ConflictResolver,
}

/// Deprecated type alias for backward compatibility
///
/// This was renamed to clarify the distinction between mapping loaders
/// and ETL loaders. The mapping loader focuses on unified mapping sessions,
/// DDL generation, and conflict resolution, while the ETL loader (in
/// `etl::loaders::database::PostgreSQLLoader`) focuses on high-performance
/// COPY operations and the DatabaseLoader trait.
///
/// # Migration
///
/// ```rust
/// // Old (still works, with deprecation warning)
/// use graphica_coordinator::mapping::loader::PostgreSQLLoader;
///
/// // New (recommended)
/// use graphica_coordinator::mapping::loader::MappingPostgresLoader;
/// ```
#[deprecated(
    since = "2.2.0",
    note = "Renamed to MappingPostgresLoader to avoid naming conflict with etl::loaders::database::PostgreSQLLoader. Use MappingPostgresLoader for mapping workflows."
)]
pub type PostgreSQLLoader = MappingPostgresLoader;

/// Loader configuration
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Batch size for INSERT operations
    pub batch_size: usize,

    /// Whether to create tables if they don't exist
    pub create_tables: bool,

    /// Whether to drop existing tables first
    pub drop_existing: bool,

    /// Whether to use transactions
    pub use_transactions: bool,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            create_tables: true,
            drop_existing: false,
            use_transactions: true,
        }
    }
}

/// Load operation result
#[derive(Debug, Clone)]
pub struct LoadResult {
    /// Number of rows processed from source
    pub rows_processed: usize,

    /// Number of rows inserted into target
    pub rows_inserted: usize,

    /// Number of rows skipped (due to fusion or errors)
    pub rows_skipped: usize,

    /// Errors encountered
    pub errors: Vec<String>,

    /// Lineage graph URI
    pub lineage_graph_uri: String,
}

/// Row data with source metadata
#[derive(Debug, Clone)]
pub struct SourceRow {
    /// Source session ID
    pub session_id: String,

    /// Table name
    pub table_name: String,

    /// Field values (field_name -> value)
    pub values: HashMap<String, Option<String>>,
}

impl MappingPostgresLoader {
    /// Create a new PostgreSQL loader
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            config,
            resolver: ConflictResolver::new(),
        }
    }

    /// Create loader with default configuration
    pub fn with_defaults() -> Self {
        Self::new(LoaderConfig::default())
    }

    /// Generate CREATE TABLE DDL for target database
    pub fn generate_create_table_ddl(&self, table_config: &TargetTableConfig) -> Result<String> {
        // Validate table configuration to prevent SQL injection
        table_config
            .validate()
            .context("Invalid table configuration for PostgreSQL DDL generation")?;

        let mut ddl = format!("CREATE TABLE {} (\n", table_config.name);

        let mut column_defs = Vec::new();

        // Generate column definitions
        for (column_name, column_config) in &table_config.columns {
            let mut col_def = format!("    {} {}", column_name, column_config.data_type);

            if !column_config.nullable {
                col_def.push_str(" NOT NULL");
            }

            if column_config.is_primary_key {
                col_def.push_str(" PRIMARY KEY");
            }

            column_defs.push(col_def);
        }

        // Add foreign key constraints
        for fk in &table_config.foreign_keys {
            let fk_def = format!(
                "    FOREIGN KEY ({}) REFERENCES {}({})",
                fk.column, fk.references_table, fk.references_column
            );
            column_defs.push(fk_def);
        }

        ddl.push_str(&column_defs.join(",\n"));
        ddl.push_str("\n)");

        Ok(ddl)
    }

    /// Generate INSERT statement for a batch of rows
    pub fn generate_insert_statement(
        &self,
        table_name: &str,
        columns: &[String],
        row_count: usize,
    ) -> Result<String> {
        use graphica_core::security::validate_identifier;

        if columns.is_empty() {
            return Err(anyhow::anyhow!("No columns specified for INSERT"));
        }

        if row_count == 0 {
            return Err(anyhow::anyhow!("Row count must be greater than 0"));
        }

        // Validate table name to prevent SQL injection
        validate_identifier(table_name).context(format!(
            "Invalid table name for PostgreSQL INSERT: {}",
            table_name
        ))?;

        // Validate all column names to prevent SQL injection
        for column in columns {
            validate_identifier(column).context(format!(
                "Invalid column name '{}' for PostgreSQL INSERT",
                column
            ))?;
        }

        let mut sql = format!("INSERT INTO {} (", table_name);
        sql.push_str(&columns.join(", "));
        sql.push_str(") VALUES\n");

        let mut value_clauses = Vec::new();
        for i in 0..row_count {
            let mut placeholders = Vec::new();
            for j in 0..columns.len() {
                let param_num = i * columns.len() + j + 1;
                placeholders.push(format!("${}", param_num));
            }
            value_clauses.push(format!("    ({})", placeholders.join(", ")));
        }

        sql.push_str(&value_clauses.join(",\n"));

        Ok(sql)
    }

    /// Map source data type to PostgreSQL data type
    pub fn map_data_type(&self, source_type: &str) -> String {
        let upper = source_type.to_uppercase();
        match upper.as_str() {
            "TEXT" | "STRING" | "VARCHAR" => "VARCHAR(255)".to_string(),
            "VARCHAR(255)" => "VARCHAR(255)".to_string(),
            "INT" | "INTEGER" => "INTEGER".to_string(),
            "BIGINT" => "BIGINT".to_string(),
            "FLOAT" | "REAL" => "REAL".to_string(),
            "DOUBLE" | "DECIMAL" => "DECIMAL".to_string(),
            "BOOL" | "BOOLEAN" => "BOOLEAN".to_string(),
            "DATE" => "DATE".to_string(),
            "TIMESTAMP" | "DATETIME" => "TIMESTAMP".to_string(),
            "JSON" | "JSONB" => "JSONB".to_string(),
            _ => "TEXT".to_string(), // Default fallback
        }
    }

    /// Resolve field values using conflict resolution strategies
    pub fn resolve_field_values(
        &self,
        mapping: &UnifiedFieldMapping,
        source_rows: &[SourceRow],
    ) -> Result<ResolvedValue> {
        // Build SourceValue list from source rows
        let source_values: Vec<SourceValue> = mapping
            .source_fields
            .iter()
            .filter_map(|source_ref| {
                // Find matching source row
                for row in source_rows {
                    if row.session_id == source_ref.session_id
                        && row.table_name == source_ref.table_name
                    {
                        if let Some(value) = row.values.get(&source_ref.field_name) {
                            return Some(SourceValue {
                                source_id: source_ref.canonical_source_id(),
                                value: value.clone(),
                                confidence: Some(mapping.confidence),
                            });
                        }
                    }
                }
                None
            })
            .collect();

        if source_values.is_empty() {
            // No matching source data
            return Ok(ResolvedValue {
                value: None,
                contributing_sources: vec![],
                strategy_used: "NoData".to_string(),
            });
        }

        // Apply conflict resolution strategy
        self.resolver
            .resolve(&mapping.conflict_resolution, &source_values)
    }

    /// Apply transformation to a resolved value
    pub fn apply_transformation(
        &self,
        value: Option<String>,
        transformation: Option<&String>,
    ) -> Result<Option<String>> {
        let Some(val) = value else {
            return Ok(None);
        };

        let Some(transform) = transformation else {
            return Ok(Some(val));
        };

        // Simple transformation support
        // In production, this would use a more sophisticated expression engine
        let result = if transform.contains("UPPER") {
            val.to_uppercase()
        } else if transform.contains("LOWER") {
            val.to_lowercase()
        } else if transform.contains("TRIM") {
            val.trim().to_string()
        } else {
            // For complex transformations, just return the value
            val
        };

        Ok(Some(result))
    }

    /// Build target row from source rows using unified mapping
    pub fn build_target_row(
        &self,
        session: &UnifiedMappingSession,
        source_rows: &[SourceRow],
        table_name: &str,
    ) -> Result<HashMap<String, Option<String>>> {
        let mut target_row = HashMap::new();

        // Find all mappings for this table
        let table_mappings: Vec<&UnifiedFieldMapping> = session
            .field_mappings
            .iter()
            .filter(|m| m.target_column.table_name == table_name)
            .collect();

        for mapping in table_mappings {
            // Resolve field values
            let resolved = self.resolve_field_values(mapping, source_rows)?;

            // Apply transformation
            let transformed =
                self.apply_transformation(resolved.value, mapping.transformation.as_ref())?;

            target_row.insert(mapping.target_column.column_name.clone(), transformed);
        }

        Ok(target_row)
    }

    /// Generate lineage RDF triples for a load operation
    pub fn generate_lineage_triples(
        &self,
        load_id: &str,
        session: &UnifiedMappingSession,
        rows_inserted: usize,
    ) -> Vec<(String, String, String)> {
        let mut triples = Vec::new();

        let load_uri = format!("http://graphica.io/load/{}", load_id);
        let gph_ns = "http://graphica.io/ontology#";
        let prov_ns = "http://www.w3.org/ns/prov#";
        let xsd_ns = "http://www.w3.org/2001/XMLSchema#";

        // Load operation type
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}type>", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
            format!("<{}LoadOperation>", gph_ns),
        ));

        // Link to unified session
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}fromUnifiedSession>", gph_ns),
            format!("<http://graphica.io/mapping/unified/{}>", session.id),
        ));

        // Target database
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}targetDatabase>", gph_ns),
            format!("\"{}\"", session.target_database.datasource_id),
        ));

        // Rows inserted
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}rowsInserted>", gph_ns),
            format!("\"{}\"^^<{}integer>", rows_inserted, xsd_ns),
        ));

        // Timestamp
        let now = chrono::Utc::now().to_rfc3339();
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}endedAtTime>", prov_ns),
            format!("\"{}\"^^<{}dateTime>", now, xsd_ns),
        ));

        triples
    }

    /// Validate target row against table schema
    pub fn validate_row(
        &self,
        row: &HashMap<String, Option<String>>,
        table_config: &TargetTableConfig,
    ) -> Result<()> {
        // Check required columns
        for (column_name, column_config) in &table_config.columns {
            if !column_config.nullable {
                let value = row.get(column_name);
                if value.is_none() || value.unwrap().is_none() {
                    return Err(anyhow::anyhow!(
                        "Required column '{}' is missing or null",
                        column_name
                    ));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::multi_source::types::{
        ForeignKeyConfig, SourceFieldRef, TargetColumnConfig, UnifiedSessionStatus,
    };

    fn create_test_table_config() -> TargetTableConfig {
        let mut columns = HashMap::new();
        columns.insert(
            "id".to_string(),
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "SERIAL".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );
        columns.insert(
            "email".to_string(),
            TargetColumnConfig {
                name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );
        columns.insert(
            "name".to_string(),
            TargetColumnConfig {
                name: "name".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: true,
                is_primary_key: false,
                default_value: None,
            },
        );

        TargetTableConfig {
            name: "customers".to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn test_generate_create_table_ddl() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let table_config = create_test_table_config();

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        assert!(ddl.contains("CREATE TABLE customers"));
        assert!(ddl.contains("id SERIAL NOT NULL PRIMARY KEY"));
        assert!(ddl.contains("email VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("name VARCHAR(255)"));

        Ok(())
    }

    #[test]
    fn test_generate_create_table_ddl_with_foreign_key() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let mut table_config = create_test_table_config();

        table_config.columns.insert(
            "customer_id".to_string(),
            TargetColumnConfig {
                name: "customer_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );

        table_config.foreign_keys.push(ForeignKeyConfig {
            column: "customer_id".to_string(),
            references_table: "customers".to_string(),
            references_column: "id".to_string(),
            on_delete: None,
        });

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        assert!(ddl.contains("FOREIGN KEY (customer_id) REFERENCES customers(id)"));

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let columns = vec!["id".to_string(), "email".to_string(), "name".to_string()];

        let sql = loader.generate_insert_statement("customers", &columns, 2)?;

        assert!(sql.contains("INSERT INTO customers (id, email, name)"));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("($1, $2, $3)"));
        assert!(sql.contains("($4, $5, $6)"));

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement_single_row() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let columns = vec!["email".to_string()];

        let sql = loader.generate_insert_statement("test_table", &columns, 1)?;

        assert!(sql.contains("INSERT INTO test_table (email)"));
        assert!(sql.contains("($1)"));

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement_no_columns() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let columns = vec![];

        let result = loader.generate_insert_statement("test_table", &columns, 1);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement_zero_rows() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let columns = vec!["email".to_string()];

        let result = loader.generate_insert_statement("test_table", &columns, 0);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_map_data_type() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        assert_eq!(loader.map_data_type("TEXT"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("STRING"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("VARCHAR"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("INT"), "INTEGER");
        assert_eq!(loader.map_data_type("INTEGER"), "INTEGER");
        assert_eq!(loader.map_data_type("BIGINT"), "BIGINT");
        assert_eq!(loader.map_data_type("FLOAT"), "REAL");
        assert_eq!(loader.map_data_type("DOUBLE"), "DECIMAL");
        assert_eq!(loader.map_data_type("BOOL"), "BOOLEAN");
        assert_eq!(loader.map_data_type("DATE"), "DATE");
        assert_eq!(loader.map_data_type("TIMESTAMP"), "TIMESTAMP");
        assert_eq!(loader.map_data_type("JSONB"), "JSONB");
        assert_eq!(loader.map_data_type("UNKNOWN"), "TEXT");

        Ok(())
    }

    #[test]
    fn test_apply_transformation_upper() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let result = loader.apply_transformation(
            Some("hello".to_string()),
            Some(&"UPPER({value})".to_string()),
        )?;

        assert_eq!(result, Some("HELLO".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_lower() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let result = loader.apply_transformation(
            Some("HELLO".to_string()),
            Some(&"LOWER({value})".to_string()),
        )?;

        assert_eq!(result, Some("hello".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_trim() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let result = loader.apply_transformation(
            Some("  hello  ".to_string()),
            Some(&"TRIM({value})".to_string()),
        )?;

        assert_eq!(result, Some("hello".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_none() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let result = loader.apply_transformation(Some("hello".to_string()), None)?;

        assert_eq!(result, Some("hello".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_null_value() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let result = loader.apply_transformation(None, Some(&"UPPER({value})".to_string()))?;

        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_build_target_row() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        // Create test unified session
        let mut session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "postgres_001".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            },
            field_mappings: vec![UnifiedFieldMapping {
                id: "mapping_001".to_string(),
                source_fields: vec![SourceFieldRef {
                    session_id: "sess_001".to_string(),
                    datasource_id: "csv_001".to_string(),
                    table_name: "data".to_string(),
                    field_name: "email".to_string(),
                    source_data_type: "VARCHAR".to_string(),
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
            }],
            conflicts: vec![],
            status: UnifiedSessionStatus::ReadyToLoad,
            created_at: 1697356800,
            created_by: "test_user".to_string(),
            updated_at: 1697356800,
        };

        // Create source row
        let mut values = HashMap::new();
        values.insert("email".to_string(), Some("john@example.com".to_string()));

        let source_rows = vec![SourceRow {
            session_id: "sess_001".to_string(),
            table_name: "data".to_string(),
            values,
        }];

        let target_row = loader.build_target_row(&session, &source_rows, "customers")?;

        assert_eq!(
            target_row.get("email"),
            Some(&Some("john@example.com".to_string()))
        );

        Ok(())
    }

    #[test]
    fn test_validate_row_success() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let table_config = create_test_table_config();

        let mut row = HashMap::new();
        row.insert("id".to_string(), Some("1".to_string()));
        row.insert("email".to_string(), Some("test@example.com".to_string()));
        row.insert("name".to_string(), None);

        let result = loader.validate_row(&row, &table_config);
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_validate_row_missing_required() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let table_config = create_test_table_config();

        let mut row = HashMap::new();
        row.insert("id".to_string(), Some("1".to_string()));
        // Missing required 'email' column

        let result = loader.validate_row(&row, &table_config);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_validate_row_null_required() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();
        let table_config = create_test_table_config();

        let mut row = HashMap::new();
        row.insert("id".to_string(), Some("1".to_string()));
        row.insert("email".to_string(), None); // NULL in required column

        let result = loader.validate_row(&row, &table_config);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_generate_lineage_triples() -> Result<()> {
        let loader = MappingPostgresLoader::with_defaults();

        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "postgres_001".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            },
            field_mappings: vec![],
            conflicts: vec![],
            status: UnifiedSessionStatus::ReadyToLoad,
            created_at: 1697356800,
            created_by: "test_user".to_string(),
            updated_at: 1697356800,
        };

        let triples = loader.generate_lineage_triples("load_001", &session, 100);

        assert!(!triples.is_empty());
        assert!(triples.iter().any(|(s, _, _)| s.contains("load_001")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("targetDatabase")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("rowsInserted")));

        Ok(())
    }
}
