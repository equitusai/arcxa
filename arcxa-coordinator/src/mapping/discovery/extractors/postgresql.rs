//! # PostgreSQL Schema Extractor
//!
//! Reference implementation of SchemaExtractor for PostgreSQL.
//! This serves as a modular template for other data sources.
//!
//! ## SQL Queries Used
//!
//! - **Metadata**: INFORMATION_SCHEMA.COLUMNS + INFORMATION_SCHEMA.TABLES
//! - **Sampling**: TABLESAMPLE BERNOULLI for efficient random sampling
//! - **Statistics**: pg_stats for pre-computed column statistics
//!
//! ## Performance
//!
//! - Metadata extraction: <100ms for typical schemas
//! - Sample extraction: <500ms using TABLESAMPLE
//! - Statistics: <50ms per column from pg_stats
//!
//! ## Modularity
//!
//! This implementation follows a clear pattern:
//! 1. Connection management (reusable across extractors)
//! 2. SQL query construction (database-specific)
//! 3. Result mapping (reusable pattern)
//!
//! Other data sources can follow this same pattern:
//! - Snowflake: Replace queries with Snowflake SQL
//! - Oracle: Replace queries with Oracle SQL
//! - S3/Parquet: Replace queries with file reading

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::postgres_tls::connect_postgres_client;
use graphica_core::catalog::types::{DataSource, SourceConfig};

use super::super::types::*;
use super::traits::SchemaExtractor;

/// PostgreSQL schema extractor
///
/// Implements schema discovery for PostgreSQL databases using:
/// - INFORMATION_SCHEMA for metadata
/// - TABLESAMPLE for efficient sampling
/// - pg_stats for column statistics
pub struct PostgreSQLExtractor {
    /// Placeholder for future pool configuration.
    _connection_config: Option<String>,
}

impl PostgreSQLExtractor {
    /// Create a new PostgreSQL extractor
    pub fn new() -> Self {
        Self {
            _connection_config: None,
        }
    }

    // ========================================================================
    // SQL Query Builders
    // ========================================================================

    /// Build INFORMATION_SCHEMA query for metadata extraction
    ///
    /// Returns table and column metadata from INFORMATION_SCHEMA.
    /// This is standard SQL and works across PostgreSQL versions.
    fn build_metadata_query(schema_filter: Option<&str>, table_filter: Option<&str>) -> String {
        let mut query = String::from(
            r#"
SELECT
    t.table_schema,
    t.table_name,
    t.table_type,
    c.column_name,
    c.data_type,
    c.is_nullable,
    c.column_default,
    CASE
        WHEN tc.constraint_type = 'PRIMARY KEY' THEN true
        ELSE false
    END as is_primary_key,
    pg_class.reltuples::bigint as estimated_row_count
FROM information_schema.tables t
INNER JOIN information_schema.columns c
    ON t.table_schema = c.table_schema
    AND t.table_name = c.table_name
LEFT JOIN information_schema.table_constraints tc
    ON t.table_schema = tc.table_schema
    AND t.table_name = tc.table_name
    AND tc.constraint_type = 'PRIMARY KEY'
LEFT JOIN information_schema.key_column_usage kcu
    ON tc.constraint_name = kcu.constraint_name
    AND c.column_name = kcu.column_name
LEFT JOIN pg_class
    ON pg_class.relname = t.table_name
WHERE t.table_type = 'BASE TABLE'
"#,
        );

        if let Some(schema) = schema_filter {
            query.push_str(&format!("  AND t.table_schema = '{}'\n", schema));
        }

        if let Some(table) = table_filter {
            query.push_str(&format!("  AND t.table_name = '{}'\n", table));
        }

        query.push_str("ORDER BY t.table_schema, t.table_name, c.ordinal_position");

        debug!("Built metadata query:\n{}", query);
        query
    }

    /// Build TABLESAMPLE query for efficient sampling
    ///
    /// Uses TABLESAMPLE BERNOULLI for random sampling.
    /// This is much faster than ORDER BY RANDOM() LIMIT N.
    fn build_sample_query(schema: &str, table_name: &str, sample_size: usize) -> String {
        // Calculate sample percentage (aim for ~1000 rows)
        // BERNOULLI samples rows with given percentage probability
        let sample_percent = 10.0; // 10% sample, adjust based on table size

        let query = format!(
            r#"
SELECT row_to_json(sampled)::text AS row_json
FROM (
    SELECT *
    FROM "{}"."{}" TABLESAMPLE BERNOULLI ({})
    LIMIT {}
) AS sampled
"#,
            schema, table_name, sample_percent, sample_size
        );

        debug!("Built sample query:\n{}", query);
        query
    }

    /// Build pg_stats query for column statistics
    ///
    /// PostgreSQL maintains pre-computed statistics in pg_stats.
    /// This is much faster than computing statistics on-the-fly.
    fn build_statistics_query(schema: &str, table_name: &str, column_name: &str) -> String {
        let query = format!(
            r#"
SELECT
    schemaname,
    tablename,
    attname as column_name,
    n_distinct::text as distinct_count,
    null_frac::text as null_fraction,
    array_to_string(most_common_vals, ',') as most_common_values
FROM pg_stats
WHERE schemaname = '{}'
  AND tablename = '{}'
  AND attname = '{}'
"#,
            schema, table_name, column_name
        );

        debug!("Built statistics query:\n{}", query);
        query
    }

    /// Build query for foreign key relationships.
    fn build_relationships_query(
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> String {
        let mut query = String::from(
            r#"
SELECT
    tc.constraint_name,
    kcu.table_name AS source_table,
    kcu.column_name AS source_column,
    ccu.table_name AS target_table,
    ccu.column_name AS target_column
FROM information_schema.table_constraints tc
INNER JOIN information_schema.key_column_usage kcu
    ON tc.constraint_name = kcu.constraint_name
    AND tc.table_schema = kcu.table_schema
INNER JOIN information_schema.constraint_column_usage ccu
    ON ccu.constraint_name = tc.constraint_name
    AND ccu.table_schema = tc.table_schema
WHERE tc.constraint_type = 'FOREIGN KEY'
"#,
        );

        if let Some(schema) = schema_filter {
            query.push_str(&format!("  AND tc.table_schema = '{}'\n", schema));
        }

        if let Some(table) = table_filter {
            query.push_str(&format!("  AND kcu.table_name = '{}'\n", table));
        }

        query.push_str("ORDER BY tc.constraint_name, kcu.ordinal_position");
        query
    }

    // ========================================================================
    // Connection Helpers
    // ========================================================================

    fn is_safe_identifier(value: &str) -> bool {
        !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn validate_identifier(value: &str, field: &str) -> Result<()> {
        if Self::is_safe_identifier(value) {
            Ok(())
        } else {
            Err(anyhow!(
                "Invalid {} '{}': only [A-Za-z0-9_] are allowed",
                field,
                value
            ))
        }
    }

    /// Execute a query and return rows as column-name -> string value.
    async fn execute_query(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        query: &str,
    ) -> Result<Vec<HashMap<String, String>>> {
        let (host, port, database, _schema, ssl_mode) = Self::get_pg_config(source)?;

        let mut connection_string = format!(
            "host={} port={} user={} password={} dbname={}",
            host, port, credentials.username, credentials.password, database
        );
        if let Some(mode) = ssl_mode {
            connection_string.push_str(&format!(" sslmode={}", mode));
        }

        let client = connect_postgres_client(&connection_string, ssl_mode)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to PostgreSQL {}:{}/{}",
                    host, port, database
                )
            })?;

        let rows = client
            .query(query, &[])
            .await
            .with_context(|| "Failed to execute PostgreSQL discovery query".to_string())?;

        let mut mapped_rows = Vec::with_capacity(rows.len());
        for row in rows {
            let mut mapped = HashMap::new();
            for (idx, column) in row.columns().iter().enumerate() {
                mapped.insert(
                    column.name().to_string(),
                    Self::row_value_to_string(&row, idx),
                );
            }
            mapped_rows.push(mapped);
        }

        Ok(mapped_rows)
    }

    /// Get PostgreSQL configuration from DataSource
    fn get_pg_config(source: &DataSource) -> Result<(&str, u16, &str, &str, Option<&str>)> {
        match &source.connection.config {
            SourceConfig::PostgreSQL(config) => {
                let schema = config.schema.as_deref().unwrap_or("public");
                Ok((
                    &config.host,
                    config.port,
                    &config.database,
                    schema,
                    config.ssl_mode.as_deref(),
                ))
            }
            _ => Err(anyhow!("Expected PostgreSQL configuration")),
        }
    }

    fn row_value_to_string(row: &tokio_postgres::Row, idx: usize) -> String {
        if let Ok(value) = row.try_get::<usize, Option<String>>(idx) {
            return value.unwrap_or_default();
        }
        if let Ok(value) = row.try_get::<usize, String>(idx) {
            return value;
        }
        if let Ok(value) = row.try_get::<usize, Option<bool>>(idx) {
            return value.map(|v| v.to_string()).unwrap_or_default();
        }
        if let Ok(value) = row.try_get::<usize, bool>(idx) {
            return value.to_string();
        }
        if let Ok(value) = row.try_get::<usize, Option<i64>>(idx) {
            return value.map(|v| v.to_string()).unwrap_or_default();
        }
        if let Ok(value) = row.try_get::<usize, i64>(idx) {
            return value.to_string();
        }
        if let Ok(value) = row.try_get::<usize, Option<i32>>(idx) {
            return value.map(|v| v.to_string()).unwrap_or_default();
        }
        if let Ok(value) = row.try_get::<usize, i32>(idx) {
            return value.to_string();
        }
        if let Ok(value) = row.try_get::<usize, Option<f64>>(idx) {
            return value.map(|v| v.to_string()).unwrap_or_default();
        }
        if let Ok(value) = row.try_get::<usize, f64>(idx) {
            return value.to_string();
        }
        String::new()
    }

    // ========================================================================
    // Result Mapping (Modular pattern for all extractors)
    // ========================================================================

    /// Map INFORMATION_SCHEMA results to SchemaMetadata
    ///
    /// This mapping pattern can be reused for other databases.
    fn map_metadata_results(rows: Vec<HashMap<String, String>>) -> Result<SchemaMetadata> {
        if rows.is_empty() {
            return Ok(SchemaMetadata {
                schema_name: "public".to_string(),
                tables: vec![],
                relationships: vec![],
            });
        }

        let schema_name = rows[0]
            .get("table_schema")
            .cloned()
            .unwrap_or_else(|| "public".to_string());

        let mut tables_map: HashMap<String, TableMetadata> = HashMap::new();

        for row in rows {
            let table_name = row
                .get("table_name")
                .ok_or_else(|| anyhow!("Missing table_name"))?
                .clone();

            let table = tables_map.entry(table_name.clone()).or_insert_with(|| {
                let estimated_rows = row
                    .get("estimated_row_count")
                    .and_then(|s| s.parse::<u64>().ok());

                TableMetadata {
                    name: table_name.clone(),
                    columns: vec![],
                    estimated_rows,
                }
            });

            let column = ColumnMetadata {
                name: row
                    .get("column_name")
                    .ok_or_else(|| anyhow!("Missing column_name"))?
                    .clone(),
                data_type: row
                    .get("data_type")
                    .ok_or_else(|| anyhow!("Missing data_type"))?
                    .clone(),
                nullable: row.get("is_nullable").map(|s| s == "YES").unwrap_or(true),
                default_value: row.get("column_default").cloned(),
                primary_key: row
                    .get("is_primary_key")
                    .map(|s| s == "true" || s == "t")
                    .unwrap_or(false),
            };

            table.columns.push(column);
        }

        Ok(SchemaMetadata {
            schema_name,
            tables: tables_map.into_values().collect(),
            relationships: vec![],
        })
    }

    /// Map sample query results to SampleRow
    fn map_sample_results(rows: Vec<HashMap<String, String>>) -> Result<Vec<SampleRow>> {
        let mut samples = Vec::new();

        for row in rows {
            if let Some(raw_json) = row.get("row_json") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_json) {
                    if let Some(object) = parsed.as_object() {
                        let mut values = HashMap::new();
                        for (key, value) in object {
                            let value_str = match value {
                                serde_json::Value::Null => String::new(),
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            values.insert(key.clone(), value_str);
                        }
                        samples.push(SampleRow { values });
                        continue;
                    }
                }
            }

            samples.push(SampleRow { values: row });
        }

        Ok(samples)
    }

    fn map_relationship_results(
        rows: Vec<HashMap<String, String>>,
    ) -> Result<Vec<TableRelationshipMetadata>> {
        let mut relationships_by_key: HashMap<String, TableRelationshipMetadata> = HashMap::new();

        for row in rows {
            let constraint_name = row.get("constraint_name").cloned().unwrap_or_default();
            let source_table = row
                .get("source_table")
                .ok_or_else(|| anyhow!("Missing source_table"))?
                .clone();
            let source_column = row
                .get("source_column")
                .ok_or_else(|| anyhow!("Missing source_column"))?
                .clone();
            let target_table = row
                .get("target_table")
                .ok_or_else(|| anyhow!("Missing target_table"))?
                .clone();
            let target_column = row
                .get("target_column")
                .ok_or_else(|| anyhow!("Missing target_column"))?
                .clone();

            let key = format!("{}:{}->{}", constraint_name, source_table, target_table);
            let rel =
                relationships_by_key
                    .entry(key)
                    .or_insert_with(|| TableRelationshipMetadata {
                        name: if constraint_name.is_empty() {
                            None
                        } else {
                            Some(constraint_name.clone())
                        },
                        source_table: source_table.clone(),
                        source_columns: vec![],
                        target_table: target_table.clone(),
                        target_columns: vec![],
                    });

            rel.source_columns.push(source_column);
            rel.target_columns.push(target_column);
        }

        Ok(relationships_by_key.into_values().collect())
    }

    /// Map pg_stats results to ColumnStats
    fn map_statistics_results(rows: Vec<HashMap<String, String>>) -> Result<ColumnStats> {
        if let Some(row) = rows.first() {
            Ok(ColumnStats {
                distinct_count: row
                    .get("distinct_count")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0),
                null_fraction: row
                    .get("null_fraction")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0),
                most_common_values: row.get("most_common_values").cloned(),
            })
        } else {
            Ok(ColumnStats::default())
        }
    }
}

impl Default for PostgreSQLExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaExtractor for PostgreSQLExtractor {
    async fn extract_metadata(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata> {
        info!("Extracting PostgreSQL schema metadata");

        let (_host, _port, _database, default_schema, _ssl_mode) = Self::get_pg_config(source)?;
        let schema = schema_filter.unwrap_or(default_schema);
        Self::validate_identifier(schema, "schema_filter")?;
        if let Some(table) = table_filter {
            Self::validate_identifier(table, "table_filter")?;
        }

        let query = Self::build_metadata_query(Some(schema), table_filter);

        let rows = self
            .execute_query(source, credentials, &query)
            .await
            .context("Failed to execute metadata query")?;

        let mut metadata = Self::map_metadata_results(rows)?;

        let relationship_query = Self::build_relationships_query(Some(schema), table_filter);
        match self
            .execute_query(source, credentials, &relationship_query)
            .await
        {
            Ok(relationship_rows) => {
                metadata.relationships = Self::map_relationship_results(relationship_rows)
                    .context("Failed to map relationship metadata")?;
            }
            Err(e) => {
                warn!("Failed to extract PostgreSQL relationships: {}", e);
            }
        }

        Ok(metadata)
    }

    async fn extract_samples(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        sample_size: usize,
    ) -> Result<Vec<SampleRow>> {
        info!("Extracting samples from table: {}", table_name);

        let (_host, _port, _database, schema, _ssl_mode) = Self::get_pg_config(source)?;
        Self::validate_identifier(schema, "schema")?;
        Self::validate_identifier(table_name, "table_name")?;

        let query = Self::build_sample_query(schema, table_name, sample_size);

        let rows = self
            .execute_query(source, credentials, &query)
            .await
            .context("Failed to execute sample query")?;

        Self::map_sample_results(rows)
    }

    async fn extract_statistics(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        column_name: &str,
    ) -> Result<ColumnStats> {
        debug!(
            "Extracting statistics for column: {}.{}",
            table_name, column_name
        );

        let (_host, _port, _database, schema, _ssl_mode) = Self::get_pg_config(source)?;
        Self::validate_identifier(schema, "schema")?;
        Self::validate_identifier(table_name, "table_name")?;
        Self::validate_identifier(column_name, "column_name")?;

        let query = Self::build_statistics_query(schema, table_name, column_name);

        let rows = self
            .execute_query(source, credentials, &query)
            .await
            .context("Failed to execute statistics query")?;

        Self::map_statistics_results(rows)
    }

    fn name(&self) -> &'static str {
        "PostgreSQLExtractor"
    }

    fn supports_source(&self, source_type: &str) -> bool {
        let lower = source_type.to_lowercase();
        lower == "postgresql" || lower == "postgres" || lower == "edb" || lower == "enterprisedb"
    }
}

// ============================================================================
// Module Pattern Documentation
// ============================================================================

/// ## How to Create Extractors for Other Data Sources
///
/// Follow this modular pattern:
///
/// ### 1. Query Builders
/// ```rust,ignore
/// fn build_metadata_query(...) -> String {
///     // Database-specific SQL
///     // Snowflake: Use INFORMATION_SCHEMA or SHOW commands
///     // Oracle: Use ALL_TAB_COLUMNS, USER_TAB_COLUMNS
///     // S3/Parquet: Use file listing + schema reading
/// }
/// ```
///
/// ### 2. Connection Management
/// ```rust,ignore
/// async fn execute_query(...) -> Result<Vec<HashMap<String, String>>> {
///     // Database-specific client
///     // Snowflake: snowflake-connector-rs
///     // Oracle: oracle crate
///     // S3/Parquet: aws-sdk-s3 + parquet crate
/// }
/// ```
///
/// ### 3. Result Mapping
/// ```rust,ignore
/// fn map_metadata_results(...) -> Result<SchemaMetadata> {
///     // Same pattern for all sources
///     // Convert database rows → SchemaMetadata types
/// }
/// ```
///
/// ### 4. SchemaExtractor Implementation
/// ```rust,ignore
/// #[async_trait]
/// impl SchemaExtractor for SnowflakeExtractor {
///     async fn extract_metadata(...) {
///         let query = Self::build_metadata_query(...);
///         let rows = self.execute_query(...).await?;
///         Self::map_metadata_results(rows)
///     }
///     // ... same pattern for extract_samples, extract_statistics
/// }
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_metadata_query() {
        let query = PostgreSQLExtractor::build_metadata_query(Some("public"), None);
        assert!(query.contains("information_schema.tables"));
        assert!(query.contains("table_schema = 'public'"));
    }

    #[test]
    fn test_build_sample_query() {
        let query = PostgreSQLExtractor::build_sample_query("public", "users", 1000);
        assert!(query.contains("TABLESAMPLE BERNOULLI"));
        assert!(query.contains("LIMIT 1000"));
    }

    #[test]
    fn test_supports_postgres_aliases() {
        let extractor = PostgreSQLExtractor::new();
        assert!(extractor.supports_source("postgresql"));
        assert!(extractor.supports_source("postgres"));
        assert!(extractor.supports_source("edb"));
        assert!(extractor.supports_source("enterprisedb"));
        assert!(!extractor.supports_source("oracle"));
    }

    #[test]
    fn test_build_statistics_query() {
        let query = PostgreSQLExtractor::build_statistics_query("public", "users", "email");
        assert!(query.contains("pg_stats"));
        assert!(query.contains("attname = 'email'"));
    }

    #[test]
    fn test_build_relationships_query() {
        let query = PostgreSQLExtractor::build_relationships_query(Some("public"), Some("orders"));
        assert!(query.contains("constraint_type = 'FOREIGN KEY'"));
        assert!(query.contains("tc.table_schema = 'public'"));
        assert!(query.contains("kcu.table_name = 'orders'"));
    }

    #[test]
    fn test_map_metadata_results() {
        let rows = vec![
            HashMap::from([
                ("table_schema".to_string(), "public".to_string()),
                ("table_name".to_string(), "users".to_string()),
                ("column_name".to_string(), "id".to_string()),
                ("data_type".to_string(), "integer".to_string()),
                ("is_nullable".to_string(), "NO".to_string()),
                ("is_primary_key".to_string(), "true".to_string()),
                ("estimated_row_count".to_string(), "1000".to_string()),
            ]),
            HashMap::from([
                ("table_schema".to_string(), "public".to_string()),
                ("table_name".to_string(), "users".to_string()),
                ("column_name".to_string(), "email".to_string()),
                ("data_type".to_string(), "character varying".to_string()),
                ("is_nullable".to_string(), "YES".to_string()),
                ("is_primary_key".to_string(), "false".to_string()),
                ("estimated_row_count".to_string(), "1000".to_string()),
            ]),
        ];

        let metadata = PostgreSQLExtractor::map_metadata_results(rows).unwrap();

        assert_eq!(metadata.schema_name, "public");
        assert_eq!(metadata.tables.len(), 1);
        assert_eq!(metadata.tables[0].name, "users");
        assert_eq!(metadata.tables[0].columns.len(), 2);
        assert_eq!(metadata.tables[0].columns[0].name, "id");
        assert!(metadata.tables[0].columns[0].primary_key);
        assert!(metadata.relationships.is_empty());
    }

    #[test]
    fn test_map_relationship_results() {
        let rows = vec![HashMap::from([
            (
                "constraint_name".to_string(),
                "fk_orders_customer_id".to_string(),
            ),
            ("source_table".to_string(), "orders".to_string()),
            ("source_column".to_string(), "customer_id".to_string()),
            ("target_table".to_string(), "users".to_string()),
            ("target_column".to_string(), "id".to_string()),
        ])];

        let relationships = PostgreSQLExtractor::map_relationship_results(rows).unwrap();
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].source_table, "orders");
        assert_eq!(relationships[0].target_table, "users");
        assert_eq!(
            relationships[0].source_columns,
            vec!["customer_id".to_string()]
        );
    }
}
