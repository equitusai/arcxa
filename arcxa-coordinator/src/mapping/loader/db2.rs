//! DB2 Bulk Loader
//!
//! This module provides functionality for bulk loading data from CSV files
//! into IBM DB2 databases using unified mapping sessions.
//!
//! ## Deprecation Notice
//!
//! **DDL Generation**: The `generate_create_table_ddl()` method in this module is
//! deprecated in favor of `Db2Dialect::create_table()` from the `mapping::ddl::dialects::db2`
//! module. The dialect approach provides:
//! - Single source of truth for DB2 SQL generation
//! - Better type mapping consistency
//! - Unified DDL generation across all database backends
//!
//! Use `Db2Dialect` for all new code. This method is kept only for backward
//! compatibility with existing tests.
//!
//! ## Features
//!
//! - DML generation (INSERT/MERGE statements with ? placeholders)
//! - Batch execution for performance
//! - Transaction management
//! - Lineage tracking via RDF triples
//!
//! ## DB2-specific considerations
//!
//! - Uses GENERATED ALWAYS AS IDENTITY instead of SERIAL
//! - Uses ? placeholders instead of PostgreSQL's $1, $2
//! - BOOLEAN mapped to SMALLINT (0/1)
//! - JSON mapped to CLOB

use crate::mapping::multi_source::conflict::{ConflictResolver, ResolvedValue, SourceValue};
use crate::mapping::multi_source::types::{
    ConflictResolution, TargetColumnRef, TargetDatabaseConfig, TargetTableConfig,
    UnifiedFieldMapping, UnifiedMappingSession,
};
use anyhow::Result;
use std::collections::HashMap;

use super::{LoadResult, LoaderConfig, SourceRow};

/// DB2 bulk loader
pub struct DB2Loader {
    /// Database connection configuration
    config: LoaderConfig,

    /// Conflict resolver for handling field conflicts
    resolver: ConflictResolver,
}

impl DB2Loader {
    /// Create a new DB2 loader
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

    /// Generate CREATE TABLE DDL for DB2
    ///
    /// **DEPRECATED**: This method duplicates functionality in `Db2Dialect::create_table()`.
    /// Use `Db2Dialect` from `mapping::ddl::dialects::db2` for DDL generation instead.
    ///
    /// This method is kept for backward compatibility with existing tests but should
    /// not be used in new code. The canonical DDL generation is in `Db2Dialect`.
    ///
    /// # Migration Path
    ///
    /// ```ignore
    /// // Old (deprecated):
    /// let loader = DB2Loader::with_defaults();
    /// let ddl = loader.generate_create_table_ddl(&table_config)?;
    ///
    /// // New (recommended):
    /// use graphica_coordinator::mapping::ddl::dialects::db2::Db2Dialect;
    /// use graphica_coordinator::mapping::ddl::dialects::SqlDialect;
    /// let dialect = Db2Dialect;
    /// let ddl = dialect.create_table(&table_definition);
    /// ```
    #[deprecated(
        since = "0.4.0",
        note = "Use Db2Dialect::create_table() instead. This duplicates DDL generation logic."
    )]
    pub fn generate_create_table_ddl(&self, table_config: &TargetTableConfig) -> Result<String> {
        // Validate table configuration to prevent SQL injection
        table_config.validate().map_err(|e| {
            anyhow::anyhow!("Invalid table configuration for DB2 DDL generation: {}", e)
        })?;

        let mut ddl = format!("CREATE TABLE {} (\n", table_config.name);

        let mut column_defs = Vec::new();

        // Generate column definitions
        for (column_name, column_config) in &table_config.columns {
            let mut col_def = format!("    {} {}", column_name, column_config.data_type);

            // Handle DB2-specific type mappings
            let upper_type = column_config.data_type.to_uppercase();
            if upper_type.contains("SERIAL") {
                // SERIAL -> GENERATED ALWAYS AS IDENTITY
                col_def = format!("    {} INTEGER GENERATED ALWAYS AS IDENTITY", column_name);
            } else if upper_type.contains("BOOLEAN") || upper_type.contains("BOOL") {
                // BOOLEAN -> SMALLINT for DB2
                col_def = format!("    {} SMALLINT", column_name);
            }

            if !column_config.nullable {
                col_def.push_str(" NOT NULL");
            }

            if column_config.is_primary_key {
                col_def.push_str(" PRIMARY KEY");
            }

            if let Some(default_value) = &column_config.default_value {
                col_def.push_str(&format!(" DEFAULT {}", default_value));
            }

            column_defs.push(col_def);
        }

        // Add foreign key constraints
        for fk in &table_config.foreign_keys {
            let mut fk_def = format!(
                "    FOREIGN KEY ({}) REFERENCES {}({})",
                fk.column, fk.references_table, fk.references_column
            );

            // Add ON DELETE clause if specified
            if let Some(on_delete) = &fk.on_delete {
                fk_def.push_str(&format!(" ON DELETE {}", on_delete));
            }

            column_defs.push(fk_def);
        }

        ddl.push_str(&column_defs.join(",\n"));
        ddl.push_str("\n)");

        Ok(ddl)
    }

    /// Generate INSERT statement for a batch of rows
    /// DB2 uses ? placeholders instead of PostgreSQL's $1, $2
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
        validate_identifier(table_name)
            .map_err(|e| anyhow::anyhow!("Invalid table name for DB2 INSERT: {}", e))?;

        // Validate all column names to prevent SQL injection
        for column in columns {
            validate_identifier(column).map_err(|e| {
                anyhow::anyhow!("Invalid column name '{}' for DB2 INSERT: {}", column, e)
            })?;
        }

        let mut sql = format!("INSERT INTO {} (", table_name);
        sql.push_str(&columns.join(", "));
        sql.push_str(") VALUES\n");

        let mut value_clauses = Vec::new();
        for _i in 0..row_count {
            let placeholders = vec!["?"; columns.len()];
            value_clauses.push(format!("    ({})", placeholders.join(", ")));
        }

        sql.push_str(&value_clauses.join(",\n"));

        Ok(sql)
    }

    /// Generate MERGE statement for DB2 (UPSERT capability)
    ///
    /// Creates a DB2 MERGE statement that handles both INSERT and UPDATE operations
    /// based on primary key matching. This enables idempotent loads and duplicate key handling.
    ///
    /// DB2 MERGE syntax:
    /// ```sql
    /// MERGE INTO target_table AS T
    /// USING (VALUES (?, ?), (?, ?)) AS S (col1, col2)
    /// ON T.pk_col = S.pk_col AND T.pk_col2 = S.pk_col2
    /// WHEN MATCHED THEN UPDATE SET T.col1 = S.col1, T.col2 = S.col2
    /// WHEN NOT MATCHED THEN INSERT (col1, col2) VALUES (S.col1, S.col2)
    /// ```
    ///
    /// # Arguments
    /// * `table_name` - Target table name
    /// * `columns` - All columns to be inserted/updated
    /// * `primary_keys` - Primary key columns for the ON clause
    /// * `row_count` - Number of rows in the batch
    ///
    /// # Errors
    /// Returns an error if:
    /// - No columns specified
    /// - No primary keys specified
    /// - Row count is 0
    /// - Primary key columns are not in the column list
    pub fn generate_merge_statement(
        &self,
        table_name: &str,
        columns: &[String],
        primary_keys: &[String],
        row_count: usize,
    ) -> Result<String> {
        if columns.is_empty() {
            return Err(anyhow::anyhow!("No columns specified for MERGE"));
        }

        if primary_keys.is_empty() {
            return Err(anyhow::anyhow!(
                "No primary keys specified for MERGE. Cannot determine ON clause."
            ));
        }

        if row_count == 0 {
            return Err(anyhow::anyhow!("Row count must be greater than 0"));
        }

        // Validate that all primary keys are in the column list
        for pk in primary_keys {
            if !columns.contains(pk) {
                return Err(anyhow::anyhow!(
                    "Primary key '{}' not found in column list",
                    pk
                ));
            }
        }

        let mut sql = String::new();

        // MERGE INTO clause
        sql.push_str(&format!("MERGE INTO {} AS T\n", table_name));

        // USING clause with VALUES
        sql.push_str("USING (\n");
        sql.push_str("    VALUES\n");

        // Generate multiple rows of placeholders
        let mut value_rows = Vec::new();
        for _i in 0..row_count {
            let placeholders = vec!["?"; columns.len()];
            value_rows.push(format!("        ({})", placeholders.join(", ")));
        }
        sql.push_str(&value_rows.join(",\n"));
        sql.push_str("\n");

        // AS S (column_list)
        sql.push_str(&format!(") AS S ({})\n", columns.join(", ")));

        // ON clause (primary key matching)
        sql.push_str("ON ");
        let on_conditions: Vec<String> = primary_keys
            .iter()
            .map(|pk| format!("T.{} = S.{}", pk, pk))
            .collect();
        sql.push_str(&on_conditions.join(" AND "));
        sql.push_str("\n");

        // WHEN MATCHED THEN UPDATE
        // Only update non-primary-key columns
        let update_columns: Vec<&String> = columns
            .iter()
            .filter(|col| !primary_keys.contains(col))
            .collect();

        if !update_columns.is_empty() {
            sql.push_str("WHEN MATCHED THEN\n");
            sql.push_str("    UPDATE SET ");
            let update_clauses: Vec<String> = update_columns
                .iter()
                .map(|col| format!("T.{} = S.{}", col, col))
                .collect();
            sql.push_str(&update_clauses.join(", "));
            sql.push_str("\n");
        }

        // WHEN NOT MATCHED THEN INSERT
        sql.push_str("WHEN NOT MATCHED THEN\n");
        sql.push_str(&format!("    INSERT ({})\n", columns.join(", ")));
        sql.push_str("    VALUES (");
        let insert_values: Vec<String> = columns.iter().map(|col| format!("S.{}", col)).collect();
        sql.push_str(&insert_values.join(", "));
        sql.push_str(")");

        Ok(sql)
    }

    /// Extract primary key columns from table configuration
    ///
    /// This is a helper method for MERGE operations to determine the ON clause.
    ///
    /// # Arguments
    /// * `table_config` - Target table configuration
    ///
    /// # Returns
    /// Vector of primary key column names. If no primary keys are explicitly defined
    /// in `primary_keys`, falls back to columns marked with `is_primary_key = true`.
    pub fn get_primary_keys(&self, table_config: &TargetTableConfig) -> Vec<String> {
        // First check the explicit primary_keys field
        if !table_config.primary_keys.is_empty() {
            return table_config.primary_keys.clone();
        }

        // Fallback: find columns with is_primary_key = true
        table_config
            .columns
            .iter()
            .filter_map(|(name, config)| {
                if config.is_primary_key {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Map source data type to DB2 data type
    pub fn map_data_type(&self, source_type: &str) -> String {
        let upper = source_type.to_uppercase();
        match upper.as_str() {
            "TEXT" | "STRING" | "VARCHAR" => "VARCHAR(255)".to_string(),
            "VARCHAR(255)" => "VARCHAR(255)".to_string(),
            "INT" | "INTEGER" => "INTEGER".to_string(),
            "BIGINT" => "BIGINT".to_string(),
            "FLOAT" | "REAL" => "REAL".to_string(),
            "DOUBLE" | "DECIMAL" => "DECIMAL(18,2)".to_string(),
            "BOOL" | "BOOLEAN" => "SMALLINT".to_string(), // DB2 uses SMALLINT for boolean (0/1)
            "DATE" => "DATE".to_string(),
            "TIMESTAMP" | "DATETIME" => "TIMESTAMP".to_string(),
            "JSON" | "JSONB" => "CLOB".to_string(), // DB2 doesn't have native JSON, use CLOB
            "SERIAL" => "INTEGER GENERATED ALWAYS AS IDENTITY".to_string(),
            _ => "VARCHAR(255)".to_string(), // Default fallback
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

        // Target database (DB2)
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}targetDatabase>", gph_ns),
            format!("\"{}\"", session.target_database.datasource_id),
        ));

        // Database type
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}databaseType>", gph_ns),
            "\"DB2\"".to_string(),
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
            "active".to_string(),
            TargetColumnConfig {
                name: "active".to_string(),
                data_type: "BOOLEAN".to_string(),
                nullable: true,
                is_primary_key: false,
                default_value: Some("1".to_string()),
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
        let loader = DB2Loader::with_defaults();
        let table_config = create_test_table_config();

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        assert!(ddl.contains("CREATE TABLE customers"));
        assert!(ddl.contains("id INTEGER GENERATED ALWAYS AS IDENTITY NOT NULL PRIMARY KEY"));
        assert!(ddl.contains("email VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("active SMALLINT DEFAULT 1"));

        Ok(())
    }

    #[test]
    fn test_generate_create_table_ddl_with_foreign_key() -> Result<()> {
        let loader = DB2Loader::with_defaults();
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
            on_delete: Some("CASCADE".to_string()),
        });

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        assert!(ddl.contains("FOREIGN KEY (customer_id) REFERENCES customers(id)"));
        assert!(ddl.contains("ON DELETE CASCADE"));

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "email".to_string(), "active".to_string()];

        let sql = loader.generate_insert_statement("customers", &columns, 2)?;

        assert!(sql.contains("INSERT INTO customers (id, email, active)"));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("(?, ?, ?)"));
        // DB2 uses ? placeholders, should have 2 rows
        assert_eq!(sql.matches("(?, ?, ?)").count(), 2);

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement_single_row() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["email".to_string()];

        let sql = loader.generate_insert_statement("test_table", &columns, 1)?;

        assert!(sql.contains("INSERT INTO test_table (email)"));
        assert!(sql.contains("(?)"));
        assert_eq!(sql.matches("(?)").count(), 1);

        Ok(())
    }

    #[test]
    fn test_map_data_type() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        assert_eq!(loader.map_data_type("TEXT"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("STRING"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("VARCHAR"), "VARCHAR(255)");
        assert_eq!(loader.map_data_type("INT"), "INTEGER");
        assert_eq!(loader.map_data_type("INTEGER"), "INTEGER");
        assert_eq!(loader.map_data_type("BIGINT"), "BIGINT");
        assert_eq!(loader.map_data_type("FLOAT"), "REAL");
        assert_eq!(loader.map_data_type("DOUBLE"), "DECIMAL(18,2)");
        assert_eq!(loader.map_data_type("BOOLEAN"), "SMALLINT"); // DB2 specific
        assert_eq!(loader.map_data_type("DATE"), "DATE");
        assert_eq!(loader.map_data_type("TIMESTAMP"), "TIMESTAMP");
        assert_eq!(loader.map_data_type("JSON"), "CLOB"); // DB2 specific
        assert_eq!(
            loader.map_data_type("SERIAL"),
            "INTEGER GENERATED ALWAYS AS IDENTITY"
        ); // DB2 specific
        assert_eq!(loader.map_data_type("UNKNOWN"), "VARCHAR(255)");

        Ok(())
    }

    #[test]
    fn test_apply_transformation_upper() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        let result = loader.apply_transformation(
            Some("hello".to_string()),
            Some(&"UPPER({value})".to_string()),
        )?;

        assert_eq!(result, Some("HELLO".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_lower() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        let result = loader.apply_transformation(
            Some("HELLO".to_string()),
            Some(&"LOWER({value})".to_string()),
        )?;

        assert_eq!(result, Some("hello".to_string()));

        Ok(())
    }

    #[test]
    fn test_build_target_row() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        // Create test unified session
        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "db2_001".to_string(),
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
        let loader = DB2Loader::with_defaults();
        let table_config = create_test_table_config();

        let mut row = HashMap::new();
        row.insert("id".to_string(), Some("1".to_string()));
        row.insert("email".to_string(), Some("test@example.com".to_string()));
        row.insert("active".to_string(), None);

        let result = loader.validate_row(&row, &table_config);
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_validate_row_missing_required() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let table_config = create_test_table_config();

        let mut row = HashMap::new();
        row.insert("id".to_string(), Some("1".to_string()));
        // Missing required 'email' column

        let result = loader.validate_row(&row, &table_config);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_generate_lineage_triples() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "db2_001".to_string(),
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
        assert!(triples.iter().any(|(_, _, o)| o.contains("DB2")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("rowsInserted")));

        Ok(())
    }

    #[test]
    fn test_db2_specific_serial_to_identity() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        // Verify SERIAL is converted to GENERATED ALWAYS AS IDENTITY
        let data_type = loader.map_data_type("SERIAL");
        assert_eq!(data_type, "INTEGER GENERATED ALWAYS AS IDENTITY");

        Ok(())
    }

    #[test]
    fn test_db2_specific_boolean_to_smallint() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        // Verify BOOLEAN is converted to SMALLINT
        let data_type = loader.map_data_type("BOOLEAN");
        assert_eq!(data_type, "SMALLINT");

        Ok(())
    }

    #[test]
    fn test_db2_specific_json_to_clob() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        // Verify JSON is converted to CLOB
        let data_type = loader.map_data_type("JSON");
        assert_eq!(data_type, "CLOB");

        Ok(())
    }

    // ==================== MERGE STATEMENT TESTS ====================

    #[test]
    fn test_generate_merge_statement_single_primary_key() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "email".to_string(), "active".to_string()];
        let primary_keys = vec!["id".to_string()];

        let sql = loader.generate_merge_statement("customers", &columns, &primary_keys, 1)?;

        // Verify MERGE INTO clause
        assert!(sql.contains("MERGE INTO customers AS T"));

        // Verify USING clause with VALUES
        assert!(sql.contains("USING ("));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("(?, ?, ?)"));

        // Verify AS S clause with column list
        assert!(sql.contains(") AS S (id, email, active)"));

        // Verify ON clause (single primary key)
        assert!(sql.contains("ON T.id = S.id"));

        // Verify WHEN MATCHED THEN UPDATE (excludes primary key)
        assert!(sql.contains("WHEN MATCHED THEN"));
        assert!(sql.contains("UPDATE SET T.email = S.email, T.active = S.active"));

        // Verify WHEN NOT MATCHED THEN INSERT
        assert!(sql.contains("WHEN NOT MATCHED THEN"));
        assert!(sql.contains("INSERT (id, email, active)"));
        assert!(sql.contains("VALUES (S.id, S.email, S.active)"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_composite_primary_key() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec![
            "customer_id".to_string(),
            "order_id".to_string(),
            "product_name".to_string(),
            "quantity".to_string(),
        ];
        let primary_keys = vec!["customer_id".to_string(), "order_id".to_string()];

        let sql = loader.generate_merge_statement("orders", &columns, &primary_keys, 1)?;

        // Verify composite ON clause
        assert!(sql.contains("ON T.customer_id = S.customer_id AND T.order_id = S.order_id"));

        // Verify UPDATE excludes both primary keys
        assert!(sql.contains("UPDATE SET T.product_name = S.product_name, T.quantity = S.quantity"));

        // Verify INSERT includes all columns
        assert!(sql.contains("INSERT (customer_id, order_id, product_name, quantity)"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_multiple_rows() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "name".to_string()];
        let primary_keys = vec!["id".to_string()];

        let sql = loader.generate_merge_statement("products", &columns, &primary_keys, 3)?;

        // Verify multiple rows in VALUES clause
        assert_eq!(sql.matches("(?, ?)").count(), 3);
        assert!(sql.contains("        (?, ?),\n        (?, ?),\n        (?, ?)"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_no_columns() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns: Vec<String> = vec![];
        let primary_keys = vec!["id".to_string()];

        let result = loader.generate_merge_statement("test_table", &columns, &primary_keys, 1);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No columns specified for MERGE"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_no_primary_keys() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "name".to_string()];
        let primary_keys: Vec<String> = vec![];

        let result = loader.generate_merge_statement("test_table", &columns, &primary_keys, 1);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("No primary keys specified"));
        assert!(err_msg.contains("Cannot determine ON clause"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_primary_key_not_in_columns() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["name".to_string(), "email".to_string()];
        let primary_keys = vec!["id".to_string()]; // 'id' not in columns

        let result = loader.generate_merge_statement("test_table", &columns, &primary_keys, 1);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Primary key 'id' not found in column list"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_zero_rows() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "name".to_string()];
        let primary_keys = vec!["id".to_string()];

        let result = loader.generate_merge_statement("test_table", &columns, &primary_keys, 0);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Row count must be greater than 0"));

        Ok(())
    }

    #[test]
    fn test_generate_merge_statement_all_columns_are_primary_keys() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let columns = vec!["id".to_string(), "sub_id".to_string()];
        let primary_keys = vec!["id".to_string(), "sub_id".to_string()];

        let sql = loader.generate_merge_statement("link_table", &columns, &primary_keys, 1)?;

        // When all columns are primary keys, there should be no UPDATE clause
        // (or an empty UPDATE clause)
        assert!(sql.contains("ON T.id = S.id AND T.sub_id = S.sub_id"));
        assert!(sql.contains("WHEN NOT MATCHED THEN"));
        assert!(sql.contains("INSERT (id, sub_id)"));

        // Verify no UPDATE clause when all columns are PKs
        // The UPDATE SET clause should not exist or be empty
        let has_update_set = sql.contains("UPDATE SET");
        assert!(
            !has_update_set,
            "Should not have UPDATE SET when all columns are primary keys"
        );

        Ok(())
    }

    #[test]
    fn test_get_primary_keys_from_explicit_field() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let table_config = create_test_table_config();

        let primary_keys = loader.get_primary_keys(&table_config);

        assert_eq!(primary_keys.len(), 1);
        assert_eq!(primary_keys[0], "id");

        Ok(())
    }

    #[test]
    fn test_get_primary_keys_from_column_config() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let mut columns = HashMap::new();
        columns.insert(
            "user_id".to_string(),
            TargetColumnConfig {
                name: "user_id".to_string(),
                data_type: "INTEGER".to_string(),
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

        let table_config = TargetTableConfig {
            name: "users".to_string(),
            columns,
            primary_keys: vec![], // Empty - should fall back to is_primary_key
            foreign_keys: vec![],
        };

        let primary_keys = loader.get_primary_keys(&table_config);

        assert_eq!(primary_keys.len(), 1);
        assert_eq!(primary_keys[0], "user_id");

        Ok(())
    }

    #[test]
    fn test_get_primary_keys_composite() -> Result<()> {
        let loader = DB2Loader::with_defaults();
        let mut columns = HashMap::new();
        columns.insert(
            "customer_id".to_string(),
            TargetColumnConfig {
                name: "customer_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );
        columns.insert(
            "order_id".to_string(),
            TargetColumnConfig {
                name: "order_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );

        let table_config = TargetTableConfig {
            name: "order_items".to_string(),
            columns,
            primary_keys: vec!["customer_id".to_string(), "order_id".to_string()],
            foreign_keys: vec![],
        };

        let primary_keys = loader.get_primary_keys(&table_config);

        assert_eq!(primary_keys.len(), 2);
        assert!(primary_keys.contains(&"customer_id".to_string()));
        assert!(primary_keys.contains(&"order_id".to_string()));

        Ok(())
    }

    #[test]
    fn test_merge_statement_realistic_scenario() -> Result<()> {
        let loader = DB2Loader::with_defaults();

        // Simulate a real customer table merge
        let columns = vec![
            "customer_id".to_string(),
            "email".to_string(),
            "first_name".to_string(),
            "last_name".to_string(),
            "phone".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
        ];
        let primary_keys = vec!["customer_id".to_string()];

        let sql = loader.generate_merge_statement("customers", &columns, &primary_keys, 5)?;

        // Verify structure
        assert!(sql.contains("MERGE INTO customers AS T"));
        assert!(sql.contains("ON T.customer_id = S.customer_id"));

        // Verify all non-PK columns are in UPDATE
        assert!(sql.contains("T.email = S.email"));
        assert!(sql.contains("T.first_name = S.first_name"));
        assert!(sql.contains("T.last_name = S.last_name"));
        assert!(sql.contains("T.phone = S.phone"));
        assert!(sql.contains("T.created_at = S.created_at"));
        assert!(sql.contains("T.updated_at = S.updated_at"));

        // Verify 5 rows in VALUES
        assert_eq!(sql.matches("(?, ?, ?, ?, ?, ?, ?)").count(), 5);

        // Verify INSERT includes all columns
        assert!(sql.contains(
            "INSERT (customer_id, email, first_name, last_name, phone, created_at, updated_at)"
        ));

        Ok(())
    }
}
