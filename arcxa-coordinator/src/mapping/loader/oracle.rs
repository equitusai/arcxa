//! Oracle Bulk Loader
//!
//! This module provides functionality for bulk loading data from CSV files
//! into Oracle databases using unified mapping sessions.
//!
//! Features:
//! - DDL generation (CREATE TABLE statements with Oracle syntax)
//! - DML generation (INSERT statements with :1, :2, :3 bind variables)
//! - Batch execution for performance
//! - Transaction management
//! - Lineage tracking via RDF triples
//!
//! Oracle-specific considerations:
//! - Uses SEQUENCE + trigger or GENERATED ALWAYS AS IDENTITY (Oracle 12c+)
//! - Uses :1, :2, :3 bind variables instead of PostgreSQL's $1 or DB2's ?
//! - BOOLEAN mapped to NUMBER(1) (0/1)
//! - JSON mapped to CLOB
//! - VARCHAR mapped to VARCHAR2
//! - INTEGER mapped to NUMBER

use crate::mapping::multi_source::conflict::{ConflictResolver, ResolvedValue, SourceValue};
use crate::mapping::multi_source::types::{
    ConflictResolution, TargetColumnRef, TargetDatabaseConfig, TargetTableConfig,
    UnifiedFieldMapping, UnifiedMappingSession,
};
use anyhow::Result;
use std::collections::HashMap;

use super::{LoadResult, LoaderConfig, SourceRow};

/// Oracle bulk loader
pub struct OracleLoader {
    /// Database connection configuration
    config: LoaderConfig,

    /// Conflict resolver for handling field conflicts
    resolver: ConflictResolver,

    /// Use Oracle 12c+ IDENTITY columns (default: true)
    /// If false, uses SEQUENCE + trigger approach
    use_identity_columns: bool,
}

impl OracleLoader {
    /// Create a new Oracle loader
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            config,
            resolver: ConflictResolver::new(),
            use_identity_columns: true, // Default to Oracle 12c+ syntax
        }
    }

    /// Create loader with default configuration
    pub fn with_defaults() -> Self {
        Self::new(LoaderConfig::default())
    }

    /// Create loader with legacy SEQUENCE mode (pre-12c)
    pub fn with_legacy_sequences() -> Self {
        let mut loader = Self::new(LoaderConfig::default());
        loader.use_identity_columns = false;
        loader
    }

    /// Generate CREATE TABLE DDL for Oracle
    pub fn generate_create_table_ddl(&self, table_config: &TargetTableConfig) -> Result<String> {
        // Validate table configuration to prevent SQL injection
        table_config.validate().map_err(|e| {
            anyhow::anyhow!(
                "Invalid table configuration for Oracle DDL generation: {}",
                e
            )
        })?;

        let mut ddl = format!("CREATE TABLE {} (\n", table_config.name);

        let mut column_defs = Vec::new();

        // Generate column definitions
        for (column_name, column_config) in &table_config.columns {
            let mut col_def = format!("    {} {}", column_name, column_config.data_type);

            // Handle Oracle-specific type mappings
            let upper_type = column_config.data_type.to_uppercase();
            if upper_type.contains("SERIAL") {
                if self.use_identity_columns {
                    // Oracle 12c+ IDENTITY syntax
                    col_def = format!("    {} NUMBER GENERATED ALWAYS AS IDENTITY", column_name);
                } else {
                    // Legacy sequence (will generate separate CREATE SEQUENCE + trigger)
                    col_def = format!("    {} NUMBER", column_name);
                }
            } else if upper_type.contains("BOOLEAN") || upper_type.contains("BOOL") {
                // BOOLEAN -> NUMBER(1) for Oracle
                col_def = format!("    {} NUMBER(1)", column_name);
            } else if upper_type.contains("VARCHAR") {
                // VARCHAR -> VARCHAR2 for Oracle
                let size = if upper_type.contains("VARCHAR(") {
                    // Extract size from VARCHAR(255)
                    upper_type
                        .split('(')
                        .nth(1)
                        .and_then(|s| s.split(')').next())
                        .unwrap_or("255")
                } else {
                    "255"
                };
                col_def = format!("    {} VARCHAR2({})", column_name, size);
            } else if upper_type.contains("INTEGER") || upper_type == "INT" {
                // INTEGER -> NUMBER for Oracle
                col_def = format!("    {} NUMBER", column_name);
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

    /// Generate CREATE SEQUENCE statements for legacy mode (pre-12c)
    pub fn generate_sequence_ddl(&self, table_config: &TargetTableConfig) -> Result<Vec<String>> {
        if self.use_identity_columns {
            return Ok(vec![]); // No sequences needed for IDENTITY columns
        }

        // Validate table configuration to prevent SQL injection
        table_config.validate().map_err(|e| {
            anyhow::anyhow!(
                "Invalid table configuration for Oracle sequence generation: {}",
                e
            )
        })?;

        let mut sequences = Vec::new();

        for (column_name, column_config) in &table_config.columns {
            if column_config.data_type.to_uppercase().contains("SERIAL") {
                let sequence_name = format!("{}_{}_seq", table_config.name, column_name);
                let sequence_ddl = format!(
                    "CREATE SEQUENCE {} START WITH 1 INCREMENT BY 1 NOCACHE",
                    sequence_name
                );
                sequences.push(sequence_ddl);

                // Generate trigger for auto-increment
                let trigger_ddl = format!(
                    "CREATE OR REPLACE TRIGGER {}_trg\n\
                     BEFORE INSERT ON {}\n\
                     FOR EACH ROW\n\
                     BEGIN\n  \
                         SELECT {}.NEXTVAL INTO :NEW.{} FROM DUAL;\n\
                     END;",
                    format!("{}_{}", table_config.name, column_name),
                    table_config.name,
                    sequence_name,
                    column_name
                );
                sequences.push(trigger_ddl);
            }
        }

        Ok(sequences)
    }

    /// Generate INSERT statement for a batch of rows
    /// Oracle uses :1, :2, :3 bind variables instead of PostgreSQL's $1 or DB2's ?
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
            .map_err(|e| anyhow::anyhow!("Invalid table name for Oracle INSERT: {}", e))?;

        // Validate all column names to prevent SQL injection
        for column in columns {
            validate_identifier(column).map_err(|e| {
                anyhow::anyhow!("Invalid column name '{}' for Oracle INSERT: {}", column, e)
            })?;
        }

        let mut sql = format!("INSERT INTO {} (", table_name);
        sql.push_str(&columns.join(", "));
        sql.push_str(") VALUES\n");

        let mut value_clauses = Vec::new();
        for i in 0..row_count {
            let mut placeholders = Vec::new();
            for j in 1..=columns.len() {
                // Oracle uses :1, :2, :3 bind variables
                placeholders.push(format!(":{}", i * columns.len() + j));
            }
            value_clauses.push(format!("    ({})", placeholders.join(", ")));
        }

        sql.push_str(&value_clauses.join(",\n"));

        Ok(sql)
    }

    /// Map source data type to Oracle data type
    pub fn map_data_type(&self, source_type: &str) -> String {
        let upper = source_type.to_uppercase();
        match upper.as_str() {
            "TEXT" | "STRING" => "VARCHAR2(4000)".to_string(), // Oracle max VARCHAR2 size
            "VARCHAR" => "VARCHAR2(255)".to_string(),
            "VARCHAR(255)" => "VARCHAR2(255)".to_string(),
            "INT" | "INTEGER" => "NUMBER".to_string(), // Oracle uses NUMBER for integers
            "BIGINT" => "NUMBER(19)".to_string(),      // 64-bit integer
            "FLOAT" | "REAL" => "BINARY_FLOAT".to_string(),
            "DOUBLE" | "DECIMAL" => "NUMBER(38,2)".to_string(),
            "BOOL" | "BOOLEAN" => "NUMBER(1)".to_string(), // Oracle uses NUMBER(1) for boolean (0/1)
            "DATE" => "DATE".to_string(),
            "TIMESTAMP" | "DATETIME" => "TIMESTAMP".to_string(),
            "JSON" | "JSONB" => "CLOB".to_string(), // Oracle doesn't have native JSON in older versions
            "SERIAL" => {
                if self.use_identity_columns {
                    "NUMBER GENERATED ALWAYS AS IDENTITY".to_string() // Oracle 12c+
                } else {
                    "NUMBER".to_string() // Will use SEQUENCE + trigger
                }
            }
            _ => "VARCHAR2(255)".to_string(), // Default fallback
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

        // Target database (Oracle)
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}targetDatabase>", gph_ns),
            format!("\"{}\"", session.target_database.datasource_id),
        ));

        // Database type
        triples.push((
            format!("<{}>", load_uri),
            format!("<{}databaseType>", gph_ns),
            "\"Oracle\"".to_string(),
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
        // Validate table configuration to prevent SQL injection
        table_config.validate().map_err(|e| {
            anyhow::anyhow!(
                "Invalid table configuration for Oracle row validation: {}",
                e
            )
        })?;

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
        let loader = OracleLoader::with_defaults();
        let table_config = create_test_table_config();

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        assert!(ddl.contains("CREATE TABLE customers"));
        assert!(ddl.contains("id NUMBER GENERATED ALWAYS AS IDENTITY NOT NULL PRIMARY KEY"));
        assert!(ddl.contains("email VARCHAR2(255) NOT NULL"));
        assert!(ddl.contains("active NUMBER(1) DEFAULT 1"));

        Ok(())
    }

    #[test]
    fn test_generate_create_table_ddl_legacy_sequence() -> Result<()> {
        let loader = OracleLoader::with_legacy_sequences();
        let table_config = create_test_table_config();

        let ddl = loader.generate_create_table_ddl(&table_config)?;

        // In legacy mode, SERIAL becomes just NUMBER
        assert!(ddl.contains("CREATE TABLE customers"));
        assert!(ddl.contains("id NUMBER NOT NULL PRIMARY KEY"));
        assert!(!ddl.contains("GENERATED ALWAYS AS IDENTITY"));

        Ok(())
    }

    #[test]
    fn test_generate_sequence_ddl() -> Result<()> {
        let loader = OracleLoader::with_legacy_sequences();
        let table_config = create_test_table_config();

        let sequences = loader.generate_sequence_ddl(&table_config)?;

        assert!(!sequences.is_empty());
        assert!(sequences.iter().any(|s| s.contains("CREATE SEQUENCE")));
        assert!(sequences
            .iter()
            .any(|s| s.contains("CREATE OR REPLACE TRIGGER")));
        assert!(sequences.iter().any(|s| s.contains("customers_id_seq")));

        Ok(())
    }

    #[test]
    fn test_generate_sequence_ddl_identity_mode() -> Result<()> {
        let loader = OracleLoader::with_defaults(); // Uses IDENTITY columns
        let table_config = create_test_table_config();

        let sequences = loader.generate_sequence_ddl(&table_config)?;

        // Should be empty in IDENTITY mode
        assert!(sequences.is_empty());

        Ok(())
    }

    #[test]
    fn test_generate_create_table_ddl_with_foreign_key() -> Result<()> {
        let loader = OracleLoader::with_defaults();
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
        let loader = OracleLoader::with_defaults();
        let columns = vec!["id".to_string(), "email".to_string(), "active".to_string()];

        let sql = loader.generate_insert_statement("customers", &columns, 2)?;

        assert!(sql.contains("INSERT INTO customers (id, email, active)"));
        assert!(sql.contains("VALUES"));
        // Oracle uses :1, :2, :3 bind variables
        assert!(sql.contains("(:1, :2, :3)"));
        assert!(sql.contains("(:4, :5, :6)"));

        Ok(())
    }

    #[test]
    fn test_generate_insert_statement_single_row() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let columns = vec!["email".to_string()];

        let sql = loader.generate_insert_statement("test_table", &columns, 1)?;

        assert!(sql.contains("INSERT INTO test_table (email)"));
        assert!(sql.contains("(:1)"));

        Ok(())
    }

    #[test]
    fn test_map_data_type() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        assert_eq!(loader.map_data_type("TEXT"), "VARCHAR2(4000)");
        assert_eq!(loader.map_data_type("STRING"), "VARCHAR2(4000)");
        assert_eq!(loader.map_data_type("VARCHAR"), "VARCHAR2(255)");
        assert_eq!(loader.map_data_type("INT"), "NUMBER");
        assert_eq!(loader.map_data_type("INTEGER"), "NUMBER");
        assert_eq!(loader.map_data_type("BIGINT"), "NUMBER(19)");
        assert_eq!(loader.map_data_type("FLOAT"), "BINARY_FLOAT");
        assert_eq!(loader.map_data_type("DOUBLE"), "NUMBER(38,2)");
        assert_eq!(loader.map_data_type("BOOLEAN"), "NUMBER(1)"); // Oracle specific
        assert_eq!(loader.map_data_type("DATE"), "DATE");
        assert_eq!(loader.map_data_type("TIMESTAMP"), "TIMESTAMP");
        assert_eq!(loader.map_data_type("JSON"), "CLOB"); // Oracle specific
        assert_eq!(
            loader.map_data_type("SERIAL"),
            "NUMBER GENERATED ALWAYS AS IDENTITY"
        ); // Oracle 12c+
        assert_eq!(loader.map_data_type("UNKNOWN"), "VARCHAR2(255)");

        Ok(())
    }

    #[test]
    fn test_map_data_type_legacy_mode() -> Result<()> {
        let loader = OracleLoader::with_legacy_sequences();

        // In legacy mode, SERIAL becomes NUMBER (sequence + trigger)
        assert_eq!(loader.map_data_type("SERIAL"), "NUMBER");

        Ok(())
    }

    #[test]
    fn test_apply_transformation_upper() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        let result = loader.apply_transformation(
            Some("hello".to_string()),
            Some(&"UPPER({value})".to_string()),
        )?;

        assert_eq!(result, Some("HELLO".to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_transformation_lower() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        let result = loader.apply_transformation(
            Some("HELLO".to_string()),
            Some(&"LOWER({value})".to_string()),
        )?;

        assert_eq!(result, Some("hello".to_string()));

        Ok(())
    }

    #[test]
    fn test_build_target_row() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Create test unified session
        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "oracle_001".to_string(),
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
                    data_type: "VARCHAR2(255)".to_string(),
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
        let loader = OracleLoader::with_defaults();
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
        let loader = OracleLoader::with_defaults();
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
        let loader = OracleLoader::with_defaults();

        let session = UnifiedMappingSession {
            id: "unified_001".to_string(),
            source_sessions: vec!["sess_001".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "oracle_001".to_string(),
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
        assert!(triples.iter().any(|(_, _, o)| o.contains("Oracle")));
        assert!(triples.iter().any(|(_, p, _)| p.contains("rowsInserted")));

        Ok(())
    }

    #[test]
    fn test_oracle_specific_varchar_to_varchar2() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Verify VARCHAR is converted to VARCHAR2
        let data_type = loader.map_data_type("VARCHAR");
        assert_eq!(data_type, "VARCHAR2(255)");

        Ok(())
    }

    #[test]
    fn test_oracle_specific_integer_to_number() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Verify INTEGER is converted to NUMBER
        let data_type = loader.map_data_type("INTEGER");
        assert_eq!(data_type, "NUMBER");

        Ok(())
    }

    #[test]
    fn test_oracle_specific_boolean_to_number1() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Verify BOOLEAN is converted to NUMBER(1)
        let data_type = loader.map_data_type("BOOLEAN");
        assert_eq!(data_type, "NUMBER(1)");

        Ok(())
    }

    #[test]
    fn test_oracle_specific_json_to_clob() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Verify JSON is converted to CLOB
        let data_type = loader.map_data_type("JSON");
        assert_eq!(data_type, "CLOB");

        Ok(())
    }

    #[test]
    fn test_oracle_bind_variables() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let columns = vec!["col1".to_string(), "col2".to_string()];

        let sql = loader.generate_insert_statement("test_table", &columns, 1)?;

        // Oracle uses :1, :2 bind variables
        assert!(sql.contains(":1"));
        assert!(sql.contains(":2"));
        assert!(!sql.contains("$1")); // Not PostgreSQL style
        assert!(!sql.contains("?")); // Not DB2 style

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious table name is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_table_name_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let mut columns = HashMap::new();
        columns.insert(
            "id".to_string(),
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );

        // Malicious table config with SQL injection attempt
        let malicious_config = TargetTableConfig {
            name: "users; DROP TABLE admin_users; --".to_string(), // SQL injection attempt
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![],
        };

        // Should reject malicious config
        let result = loader.generate_create_table_ddl(&malicious_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table configuration"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious column name is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_column_name_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let mut columns = HashMap::new();

        // Malicious column name with SQL injection attempt
        columns.insert(
            "id; DELETE FROM users; --".to_string(), // SQL injection in column key
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );

        let malicious_config = TargetTableConfig {
            name: "users".to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![],
        };

        // Should reject malicious config
        let result = loader.generate_create_table_ddl(&malicious_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table configuration"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious data type is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_data_type_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let mut columns = HashMap::new();

        // Malicious data type with SQL injection attempt
        columns.insert(
            "id".to_string(),
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "INTEGER; DROP TABLE users; --".to_string(), // SQL injection in type
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );

        let malicious_config = TargetTableConfig {
            name: "users".to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![],
        };

        // Should reject malicious config
        let result = loader.generate_create_table_ddl(&malicious_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table configuration"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious FK table is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_foreign_key_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let mut columns = HashMap::new();
        columns.insert(
            "id".to_string(),
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );
        columns.insert(
            "user_id".to_string(),
            TargetColumnConfig {
                name: "user_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );

        // Malicious FK with SQL injection in referenced table
        let malicious_config = TargetTableConfig {
            name: "orders".to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![ForeignKeyConfig {
                column: "user_id".to_string(),
                references_table: "users; DROP TABLE admin; --".to_string(), // SQL injection
                references_column: "id".to_string(),
                on_delete: Some("CASCADE".to_string()),
            }],
        };

        // Should reject malicious config
        let result = loader.generate_create_table_ddl(&malicious_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table configuration"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious FK action is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_fk_action_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let mut columns = HashMap::new();
        columns.insert(
            "id".to_string(),
            TargetColumnConfig {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );
        columns.insert(
            "user_id".to_string(),
            TargetColumnConfig {
                name: "user_id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: false,
                default_value: None,
            },
        );

        // Malicious FK action with SQL injection
        let malicious_config = TargetTableConfig {
            name: "orders".to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![ForeignKeyConfig {
                column: "user_id".to_string(),
                references_table: "users".to_string(),
                references_column: "id".to_string(),
                on_delete: Some("CASCADE; DROP TABLE users; --".to_string()), // SQL injection
            }],
        };

        // Should reject malicious config
        let result = loader.generate_create_table_ddl(&malicious_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table configuration"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious INSERT table name is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_insert_table_name_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();
        let columns = vec!["id".to_string(), "email".to_string()];

        // Malicious table name in INSERT statement
        let result = loader.generate_insert_statement("users; DROP TABLE admin; --", &columns, 1);

        // Should reject malicious table name
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid table name"));

        Ok(())
    }

    /// Regression test: Verify SQL injection via malicious INSERT column name is blocked
    /// Sprint 2, Day 2 - Oracle loader security fix
    #[test]
    fn test_sql_injection_insert_column_name_blocked() -> Result<()> {
        let loader = OracleLoader::with_defaults();

        // Malicious column name in INSERT statement
        let columns = vec!["id".to_string(), "email; DROP TABLE users; --".to_string()];

        let result = loader.generate_insert_statement("users", &columns, 1);

        // Should reject malicious column name
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid column name"));

        Ok(())
    }
}
