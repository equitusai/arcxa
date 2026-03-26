//! Oracle schema extractor using ODBC connectivity.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use graphica_core::catalog::types::{DataSource, SourceConfig};
use graphica_core::catalog::{connector::Credentials, resolve_oracle_odbc_resolution};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::super::types::*;
use super::odbc::execute_odbc_query;
use super::traits::SchemaExtractor;

/// Oracle schema extractor backed by ODBC.
pub struct OracleExtractor;

impl OracleExtractor {
    pub fn new() -> Self {
        Self
    }

    fn get_oracle_config(
        source: &DataSource,
    ) -> Result<(String, u16, Option<String>, Option<String>, Option<String>)> {
        match &source.connection.config {
            SourceConfig::Oracle(config) => {
                let normalized = config.normalized();
                Ok((
                    normalized.host,
                    normalized.port,
                    normalized.service_name,
                    normalized.sid,
                    normalized.schema,
                ))
            }
            _ => Err(anyhow!("Expected Oracle configuration")),
        }
    }

    fn validate_identifier(value: &str, field: &str) -> Result<()> {
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#')
        {
            return Err(anyhow!(
                "Invalid {} '{}': only [A-Za-z0-9_$#] allowed",
                field,
                value
            ));
        }
        Ok(())
    }

    pub fn build_connection_string(
        source: &DataSource,
        credentials: &Credentials,
    ) -> Result<String> {
        let config = match &source.connection.config {
            SourceConfig::Oracle(config) => config,
            _ => return Err(anyhow!("Expected Oracle configuration")),
        };
        let resolution = resolve_oracle_odbc_resolution(config, &source.metadata)
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(resolution.build_connection_string(&credentials.username, &credentials.password))
    }

    fn build_metadata_query(schema: &str, table_filter: Option<&str>) -> String {
        let upper_schema = schema.to_uppercase();
        let mut query = format!(
            r#"
SELECT
    t.OWNER as table_schema,
    t.TABLE_NAME as table_name,
    c.COLUMN_NAME as column_name,
    c.DATA_TYPE as data_type,
    CASE WHEN c.NULLABLE = 'Y' THEN 'YES' ELSE 'NO' END as is_nullable,
    c.DATA_DEFAULT as column_default,
    CASE WHEN pk.COLUMN_NAME IS NOT NULL THEN 'true' ELSE 'false' END as is_primary_key,
    t.NUM_ROWS as estimated_row_count
FROM ALL_TABLES t
INNER JOIN ALL_TAB_COLUMNS c
    ON t.OWNER = c.OWNER
    AND t.TABLE_NAME = c.TABLE_NAME
LEFT JOIN (
    SELECT acc.OWNER, acc.TABLE_NAME, acc.COLUMN_NAME
    FROM ALL_CONSTRAINTS ac
    INNER JOIN ALL_CONS_COLUMNS acc
        ON ac.OWNER = acc.OWNER
        AND ac.CONSTRAINT_NAME = acc.CONSTRAINT_NAME
    WHERE ac.CONSTRAINT_TYPE = 'P'
) pk
    ON c.OWNER = pk.OWNER
    AND c.TABLE_NAME = pk.TABLE_NAME
    AND c.COLUMN_NAME = pk.COLUMN_NAME
WHERE t.OWNER = '{}'
"#,
            upper_schema
        );

        if let Some(table) = table_filter {
            let upper_table = table.to_uppercase();
            query.push_str(&format!("  AND t.TABLE_NAME = '{}'\n", upper_table));
        }

        query.push_str("ORDER BY t.TABLE_NAME, c.COLUMN_ID");
        query
    }

    fn build_sample_query(schema: &str, table_name: &str, sample_size: usize) -> String {
        format!(
            r#"
SELECT *
FROM "{}"."{}"
FETCH FIRST {} ROWS ONLY
"#,
            schema, table_name, sample_size
        )
    }

    fn build_statistics_query(schema: &str, table_name: &str, column_name: &str) -> String {
        format!(
            r#"
SELECT
    COUNT(DISTINCT "{col}") AS distinct_count,
    SUM(CASE WHEN "{col}" IS NULL THEN 1 ELSE 0 END) AS null_count,
    COUNT(*) AS total_count
FROM "{schema}"."{table}"
"#,
            schema = schema,
            table = table_name,
            col = column_name
        )
    }

    /// Build query for foreign key relationships
    fn build_relationships_query(schema: &str, table_filter: Option<&str>) -> String {
        let upper_schema = schema.to_uppercase();
        let mut query = format!(
            r#"
SELECT
    c.constraint_name,
    c.table_name as source_table,
    cc.column_name as source_column,
    cc.position as column_position,
    r.table_name as target_table,
    rc.column_name as target_column
FROM all_constraints c
INNER JOIN all_cons_columns cc
    ON c.constraint_name = cc.constraint_name
    AND c.owner = cc.owner
INNER JOIN all_constraints r
    ON c.r_constraint_name = r.constraint_name
    AND c.r_owner = r.owner
INNER JOIN all_cons_columns rc
    ON r.constraint_name = rc.constraint_name
    AND r.owner = rc.owner
    AND cc.position = rc.position
WHERE c.constraint_type = 'R'
  AND c.owner = '{}'
"#,
            upper_schema
        );

        if let Some(table) = table_filter {
            let upper_table = table.to_uppercase();
            query.push_str(&format!("  AND c.table_name = '{}'\n", upper_table));
        }

        query.push_str("ORDER BY c.constraint_name, cc.position");

        debug!("Built Oracle FK relationships query:\n{}", query);
        query
    }

    fn map_metadata_results(
        rows: Vec<HashMap<String, String>>,
        schema_name: &str,
    ) -> Result<SchemaMetadata> {
        if rows.is_empty() {
            return Ok(SchemaMetadata {
                schema_name: schema_name.to_string(),
                tables: vec![],
                relationships: vec![],
            });
        }

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
                nullable: row
                    .get("is_nullable")
                    .map(|s| matches!(s.as_str(), "YES" | "Y" | "TRUE" | "1"))
                    .unwrap_or(true),
                default_value: row.get("column_default").cloned(),
                primary_key: row
                    .get("is_primary_key")
                    .map(|s| matches!(s.as_str(), "true" | "t" | "1" | "YES" | "Y"))
                    .unwrap_or(false),
            };

            table.columns.push(column);
        }

        Ok(SchemaMetadata {
            schema_name: schema_name.to_string(),
            tables: tables_map.into_values().collect(),
            relationships: vec![],
        })
    }

    fn map_sample_results(rows: Vec<HashMap<String, String>>) -> Result<Vec<SampleRow>> {
        Ok(rows
            .into_iter()
            .map(|row| SampleRow { values: row })
            .collect())
    }

    /// Map foreign key relationship query results
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

    fn map_statistics_results(rows: Vec<HashMap<String, String>>) -> Result<ColumnStats> {
        if let Some(row) = rows.first() {
            let distinct_count = row
                .get("distinct_count")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let null_fraction = if let (Some(null_count), Some(total_count)) = (
                row.get("null_count").and_then(|s| s.parse::<f64>().ok()),
                row.get("total_count").and_then(|s| s.parse::<f64>().ok()),
            ) {
                if total_count > 0.0 {
                    null_count / total_count
                } else {
                    0.0
                }
            } else {
                0.0
            };

            Ok(ColumnStats {
                distinct_count,
                null_fraction,
                most_common_values: None,
            })
        } else {
            Ok(ColumnStats::default())
        }
    }
}

impl Default for OracleExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaExtractor for OracleExtractor {
    async fn extract_metadata(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata> {
        info!("Extracting Oracle schema metadata via ODBC");
        let (_host, _port, _service, _sid, default_schema) = Self::get_oracle_config(source)?;

        if let Some(schema) = schema_filter {
            Self::validate_identifier(schema, "schema_filter")?;
        }
        if let Some(table) = table_filter {
            Self::validate_identifier(table, "table_filter")?;
        }

        let schema_name = schema_filter
            .or(default_schema.as_deref())
            .unwrap_or(&credentials.username)
            .to_uppercase();

        let query = Self::build_metadata_query(&schema_name, table_filter);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = execute_odbc_query(&connection_string, &query, true)
            .await
            .context("Failed to execute Oracle metadata query")?;

        let mut metadata = Self::map_metadata_results(rows, &schema_name)?;

        // Extract foreign key relationships
        let relationship_query = Self::build_relationships_query(&schema_name, table_filter);
        match execute_odbc_query(&connection_string, &relationship_query, true).await {
            Ok(relationship_rows) => {
                metadata.relationships = Self::map_relationship_results(relationship_rows)
                    .context("Failed to map Oracle relationship metadata")?;
                debug!(
                    "Extracted {} foreign key relationships from Oracle schema '{}'",
                    metadata.relationships.len(),
                    schema_name
                );
            }
            Err(e) => {
                warn!("Failed to extract Oracle relationships: {}", e);
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
        debug!("Extracting Oracle samples for table '{}'", table_name);
        Self::validate_identifier(table_name, "table_name")?;

        let (_host, _port, _service, _sid, default_schema) = Self::get_oracle_config(source)?;
        let schema_name = default_schema
            .as_deref()
            .unwrap_or(&credentials.username)
            .to_uppercase();

        let query = Self::build_sample_query(&schema_name, table_name, sample_size);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = execute_odbc_query(&connection_string, &query, false)
            .await
            .context("Failed to execute Oracle sample query")?;

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
            "Extracting Oracle statistics for {}.{}",
            table_name, column_name
        );
        Self::validate_identifier(table_name, "table_name")?;
        Self::validate_identifier(column_name, "column_name")?;

        let (_host, _port, _service, _sid, default_schema) = Self::get_oracle_config(source)?;
        let schema_name = default_schema
            .as_deref()
            .unwrap_or(&credentials.username)
            .to_uppercase();

        let query = Self::build_statistics_query(&schema_name, table_name, column_name);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = match execute_odbc_query(&connection_string, &query, true).await {
            Ok(rows) => rows,
            Err(e) => {
                warn!(
                    "Oracle statistics query failed for {}.{}: {}. Returning defaults.",
                    table_name, column_name, e
                );
                return Ok(ColumnStats::default());
            }
        };

        let mut stats = Self::map_statistics_results(rows)?;
        if stats.null_fraction == 0.0 {
            stats.null_fraction = 0.0;
        }
        Ok(stats)
    }

    fn name(&self) -> &'static str {
        "OracleExtractor"
    }

    fn supports_source(&self, source_type: &str) -> bool {
        let lower = source_type.to_lowercase();
        lower == "oracle" || lower == "oracle19c" || lower == "oracle19i"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_oracle_aliases() {
        let extractor = OracleExtractor::new();
        assert!(extractor.supports_source("oracle"));
        assert!(extractor.supports_source("oracle19c"));
        assert!(extractor.supports_source("oracle19i"));
        assert!(!extractor.supports_source("postgresql"));
    }

    #[test]
    fn validates_identifier_rules() {
        assert!(OracleExtractor::validate_identifier("EMPLOYEE_ID", "column").is_ok());
        assert!(OracleExtractor::validate_identifier("bad-name", "column").is_err());
    }
}
