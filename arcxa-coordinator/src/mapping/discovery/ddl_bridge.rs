//! # Schema to DDL Bridge Module (Source-Agnostic)
//!
//! Generates DDL from discovered schemas regardless of source type.
//!
//! This module connects:
//! - `discovery::DiscoveredSchema` (abstract schema from any source)
//! - `mapping::ddl` (SQL DDL generation)
//!
//! ## Supported Sources
//!
//! - CSV files (via CsvExtractor)
//! - Parquet files (via future ParquetExtractor)
//! - Databases (via PostgreSQLExtractor, DB2Extractor)
//! - Snowflake (via future SnowflakeExtractor)
//! - S3 (via future S3Extractor)

use anyhow::{Context, Result};

use crate::mapping::ddl::{ColumnDefinition, SqlDialect, TableDefinition};
use crate::mapping::discovery::types::DiscoveredTable;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for schema to DDL conversion
#[derive(Debug, Clone)]
pub struct SchemaToDdlConfig {
    /// Make all columns nullable (safe default)
    pub all_columns_nullable: bool,

    /// Add VARCHAR padding for safety (percentage, 0.0 - 1.0)
    pub varchar_padding_percent: f64,

    /// Minimum VARCHAR length
    pub min_varchar_length: usize,

    /// Maximum VARCHAR length
    pub max_varchar_length: usize,

    /// Default VARCHAR length when cannot determine
    pub default_varchar_length: usize,
}

impl Default for SchemaToDdlConfig {
    fn default() -> Self {
        Self {
            all_columns_nullable: true,   // Safe default
            varchar_padding_percent: 0.5, // 50% padding for growth
            min_varchar_length: 50,       // Minimum 50 chars
            max_varchar_length: 4000,     // DB2/Oracle limit
            default_varchar_length: 255,  // Standard default
        }
    }
}

// ============================================================================
// Core Conversion Functions
// ============================================================================

/// Generate table definition from discovered schema
///
/// This is the main entry point for source-agnostic DDL generation.
///
/// # Arguments
///
/// * `discovered_table` - Table schema from any source (CSV, DB, Parquet, etc.)
/// * `dialect` - SQL dialect for type mapping
/// * `config` - Configuration for conversion
///
/// # Returns
///
/// A `TableDefinition` ready for DDL generation
pub fn table_from_discovered_schema(
    discovered_table: &DiscoveredTable,
    dialect: &dyn SqlDialect,
    config: &SchemaToDdlConfig,
) -> Result<TableDefinition> {
    // Convert discovered columns to DDL columns
    let columns = discovered_table
        .columns
        .iter()
        .map(|col| {
            let sql_type = sql_type_from_discovered_column(col, config);

            let nullable = if config.all_columns_nullable {
                true
            } else {
                col.nullable
            };

            ColumnDefinition {
                name: col.name.clone(),
                sql_type,
                nullable,
                default_value: None,
                primary_key: col.primary_key,
                unique: false,
                check_constraint: None,
                comment: Some(format!(
                    "Type: {}, Confidence: {:.2}, Semantic: {}",
                    col.data_type,
                    col.confidence,
                    col.semantic_type.as_deref().unwrap_or("none")
                )),
            }
        })
        .collect();

    // Extract primary keys
    let primary_key: Vec<String> = discovered_table
        .columns
        .iter()
        .filter(|col| col.primary_key)
        .map(|col| col.name.clone())
        .collect();

    Ok(TableDefinition {
        name: discovered_table.name.clone(),
        columns,
        primary_key,
        foreign_keys: vec![],
        indexes: vec![],
        comment: Some(format!(
            "Auto-generated from discovered schema (row count: {})",
            discovered_table.row_count.unwrap_or(0)
        )),
    })
}

/// Map discovered column to SQL type with intelligent sizing
fn sql_type_from_discovered_column(
    col: &crate::mapping::discovery::types::DiscoveredColumn,
    config: &SchemaToDdlConfig,
) -> String {
    // If data_type is already specific (e.g., "VARCHAR(100)"), use it
    if col.data_type.contains('(') {
        return col.data_type.clone();
    }

    match col.data_type.to_uppercase().as_str() {
        "INTEGER" | "INT" | "BIGINT" | "SMALLINT" => col.data_type.clone(),

        "DECIMAL" | "NUMERIC" => {
            // Use default precision if not specified
            "DECIMAL(18, 4)".to_string()
        }

        "FLOAT" | "DOUBLE" | "REAL" => col.data_type.clone(),

        "BOOLEAN" | "BOOL" => "BOOLEAN".to_string(),

        "DATE" => "DATE".to_string(),

        "TIMESTAMP" | "DATETIME" => "TIMESTAMP".to_string(),

        "VARCHAR" | "TEXT" | "STRING" => {
            // Calculate VARCHAR length from statistics
            let base_length = if let Some(avg_len) = col.statistics.avg_length {
                avg_len.ceil() as usize
            } else {
                config.default_varchar_length
            };

            // Add padding for growth
            let with_padding =
                (base_length as f64 * (1.0 + config.varchar_padding_percent)).ceil() as usize;

            // Round up to nearest 50 for cleaner DDL
            let rounded = ((with_padding + 49) / 50) * 50;

            // Apply bounds
            let varchar_len = rounded
                .max(config.min_varchar_length)
                .min(config.max_varchar_length);

            format!("VARCHAR({})", varchar_len)
        }

        "JSON" | "JSONB" => {
            // JSON as VARCHAR for databases that don't have native JSON
            "VARCHAR(4000)".to_string()
        }

        "XML" => {
            // XML as VARCHAR
            "VARCHAR(4000)".to_string()
        }

        _ => {
            // Unknown type - default to VARCHAR
            format!("VARCHAR({})", config.default_varchar_length)
        }
    }
}

/// Generate DDL statements from discovered table
///
/// # Arguments
///
/// * `discovered_table` - Table schema from any source
/// * `dialect_name` - SQL dialect ("db2", "postgresql", "oracle", etc.)
/// * `config` - Optional configuration (uses defaults if None)
///
/// # Returns
///
/// A vector of DDL statements (CREATE TABLE, indexes, etc.)
///
/// # Example
///
/// ```ignore
/// use graphica_coordinator::mapping::schema_to_ddl::*;
/// use graphica_coordinator::mapping::discovery::extractors::{CsvExtractor, SchemaExtractor};
///
/// // Extract schema from CSV
/// let extractor = CsvExtractor::new();
/// let metadata = extractor.extract_metadata(&source, &creds, None, None).await?;
/// let table = &metadata.tables[0];
///
/// // Generate DDL (works for any source!)
/// let ddl_statements = generate_ddl_from_discovered_table(
///     table,
///     "db2",
///     None,
/// )?;
///
/// for ddl in ddl_statements {
///     println!("{}", ddl);
/// }
/// ```
pub fn generate_ddl_from_discovered_table(
    discovered_table: &DiscoveredTable,
    dialect_name: &str,
    config: Option<SchemaToDdlConfig>,
) -> Result<Vec<String>> {
    let config = config.unwrap_or_default();

    // Get SQL dialect
    let dialect = crate::mapping::ddl::get_dialect(dialect_name)
        .with_context(|| format!("Unsupported SQL dialect: {}", dialect_name))?;

    // Convert to table definition
    let table_def = table_from_discovered_schema(discovered_table, &*dialect, &config)?;

    // Generate DDL
    let create_table_sql = dialect.create_table(&table_def);

    let mut ddl_statements = vec![create_table_sql];

    // Add indexes if any
    for index in &table_def.indexes {
        ddl_statements.push(dialect.create_index(index));
    }

    Ok(ddl_statements)
}

// ============================================================================
// Backward Compatibility - CSV-specific wrapper
// ============================================================================

// TODO: Phase 2 - Add ontology-aware variant (see docs/ONTOLOGY_DDL_DESIGN.md)
//
// pub fn table_from_discovered_schema_with_ontology(
//     discovered_table: &DiscoveredTable,
//     ontology_mappings: &[FieldOntologyMapping],
//     dialect: &dyn SqlDialect,
//     config: &SchemaToDdlConfig,
// ) -> Result<TableDefinition> {
//     // Use ontology_mappings to override DDL constraints
//     // Apply OntologyConstraintRegistry rules
//     // Generate columns with ontology-derived types and CHECK constraints
// }
//
// This will enable:
// - Consistent DDL across sources (schema:email → VARCHAR(255)+CHECK everywhere)
// - RDF lineage for field→ontology→DDL constraint chain
// - Semantic queries for fields by ontology type

/// Generate DDL and TableDefinition from CSV file
///
/// Returns both the DDL statements and the TableDefinition for schema versioning.
pub fn generate_ddl_and_table_def_from_csv(
    csv_path: &std::path::Path,
    table_name: &str,
    dialect_name: &str,
    config: Option<SchemaToDdlConfig>,
) -> Result<(Vec<String>, TableDefinition)> {
    use crate::mapping::discovery::extractors::{CsvExtractor, SchemaExtractor};
    use graphica_core::catalog::connector::Credentials;
    use graphica_core::catalog::types::{
        ConnectionDetails, CsvFileConfig, DataSource, SourceConfig,
    };
    use std::collections::HashMap;

    let config = config.unwrap_or_default();

    // Create a data source for CSV file
    let source = DataSource {
        id: "csv_temp".to_string(),
        title: table_name.to_string(),
        description: Some(format!("Temporary CSV source for {}", csv_path.display())),
        source_type: "csv".to_string(),
        connection: ConnectionDetails {
            secret_ref: "none".to_string(),
            config: SourceConfig::CsvFile(CsvFileConfig {
                path: csv_path.to_string_lossy().to_string(),
                delimiter: ',',
                has_header: true,
            }),
            encryption_enabled: false,
            credentials: Default::default(),
        },
        schema_ref: None,
        tags: vec![],
        metadata: HashMap::new(),
        created_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        last_synced_at: None,
    };

    let credentials = Credentials {
        username: String::new(),
        password: String::new(),
        additional: HashMap::new(),
    };

    let extractor = CsvExtractor::new();
    let runtime = tokio::runtime::Runtime::new()?;
    let discovered_table =
        runtime.block_on(extractor.extract_from_path(csv_path, table_name, 1000))?;

    // Get SQL dialect
    let dialect = crate::mapping::ddl::get_dialect(dialect_name)
        .with_context(|| format!("Unsupported SQL dialect: {}", dialect_name))?;

    // Convert to table definition
    let table_def = table_from_discovered_schema(&discovered_table, &*dialect, &config)?;

    // Generate DDL
    let ddl_statements =
        generate_ddl_from_discovered_table(&discovered_table, dialect_name, Some(config))?;

    Ok((ddl_statements, table_def))
}

/// Generate DDL from CSV file (backward compatibility wrapper)
///
/// **DEPRECATED:** Use discovery layer + generate_ddl_from_discovered_table instead.
///
/// This function exists for backward compatibility. New code should:
/// 1. Use CsvExtractor to get DiscoveredSchema
/// 2. Use generate_ddl_from_discovered_table for DDL generation
///
/// This allows the same code to work for CSV, Parquet, databases, etc.
pub fn generate_ddl_from_csv(
    csv_path: &std::path::Path,
    table_name: &str,
    dialect_name: &str,
    config: Option<SchemaToDdlConfig>,
) -> Result<Vec<String>> {
    use crate::mapping::discovery::extractors::{CsvExtractor, SchemaExtractor};
    use graphica_core::catalog::connector::Credentials;
    use graphica_core::catalog::types::{
        ConnectionDetails, CsvFileConfig, DataSource, SourceConfig,
    };
    use std::collections::HashMap;

    // Create a data source for CSV file
    let source = DataSource {
        id: "csv_temp".to_string(),
        title: table_name.to_string(),
        description: Some(format!("Temporary CSV source for {}", csv_path.display())),
        source_type: "csv".to_string(),
        connection: ConnectionDetails {
            secret_ref: "none".to_string(), // No secrets for local files
            config: SourceConfig::CsvFile(CsvFileConfig {
                path: csv_path.to_string_lossy().to_string(),
                delimiter: ',',
                has_header: true,
            }),
            encryption_enabled: false,
            credentials: Default::default(),
        },
        schema_ref: None,
        tags: vec![],
        metadata: HashMap::new(),
        created_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        last_synced_at: None,
    };

    // Empty credentials for file-based sources
    let credentials = Credentials {
        username: String::new(),
        password: String::new(),
        additional: HashMap::new(),
    };

    // Use CSV extractor
    let extractor = CsvExtractor::new();

    // Extract metadata using async runtime
    let runtime = tokio::runtime::Runtime::new()?;
    let metadata = runtime.block_on(extractor.extract_metadata(
        &source,
        &credentials,
        None,
        Some(table_name),
    ))?;

    // Get the first table metadata and convert to DiscoveredTable
    let table_metadata = metadata
        .tables
        .first()
        .ok_or_else(|| anyhow::anyhow!("No tables found in CSV"))?;

    // Convert TableMetadata to DiscoveredTable
    // We need to get the discovered table from the extractor directly
    let discovered_table =
        runtime.block_on(extractor.extract_from_path(csv_path, table_name, 1000))?;

    // Generate DDL using the source-agnostic function
    generate_ddl_from_discovered_table(&discovered_table, dialect_name, config)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::types::*;

    #[test]
    fn test_varchar_sizing() {
        let config = SchemaToDdlConfig::default();

        // Test with average length
        let col = DiscoveredColumn {
            name: "TEST_COL".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            primary_key: false,
            semantic_type: None,
            confidence: 1.0,
            patterns: vec![],
            statistics: ColumnStatistics {
                distinct_count: 100,
                null_fraction: 0.1,
                sample_count: 1000,
                most_common_values: None,
                avg_length: Some(50.0),
                min_value: None,
                max_value: None,
            },
            sample_values: vec![],
        };

        let sql_type = sql_type_from_discovered_column(&col, &config);
        assert!(sql_type.starts_with("VARCHAR("));
        // 50 * 1.5 = 75, rounded to 100
        assert_eq!(sql_type, "VARCHAR(100)");
    }

    #[test]
    fn test_preserves_specific_types() {
        let config = SchemaToDdlConfig::default();

        // Test that specific VARCHAR(N) is preserved
        let col = DiscoveredColumn {
            name: "TEST_COL".to_string(),
            data_type: "VARCHAR(200)".to_string(),
            nullable: true,
            primary_key: false,
            semantic_type: None,
            confidence: 1.0,
            patterns: vec![],
            statistics: ColumnStatistics::default(),
            sample_values: vec![],
        };

        let sql_type = sql_type_from_discovered_column(&col, &config);
        assert_eq!(sql_type, "VARCHAR(200)");
    }

    #[test]
    fn test_numeric_types_passthrough() {
        let config = SchemaToDdlConfig::default();

        for type_name in &["INTEGER", "BIGINT", "BOOLEAN", "DATE", "TIMESTAMP"] {
            let col = DiscoveredColumn {
                name: "TEST_COL".to_string(),
                data_type: type_name.to_string(),
                nullable: true,
                primary_key: false,
                semantic_type: None,
                confidence: 1.0,
                patterns: vec![],
                statistics: ColumnStatistics::default(),
                sample_values: vec![],
            };

            let sql_type = sql_type_from_discovered_column(&col, &config);
            assert_eq!(sql_type.to_uppercase(), type_name.to_uppercase());
        }
    }
}
