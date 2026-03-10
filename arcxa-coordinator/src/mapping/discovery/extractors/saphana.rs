//! SAP HANA schema extractor using ODBC connectivity.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::{DataSource, SourceConfig};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::super::types::*;
use super::odbc::execute_odbc_query;
use super::traits::SchemaExtractor;

/// SAP HANA extractor backed by ODBC.
pub struct SAPHANAExtractor;

impl SAPHANAExtractor {
    pub fn new() -> Self {
        Self
    }

    fn get_hana_config(source: &DataSource) -> Result<(&str, u16, &str, Option<&str>)> {
        match &source.connection.config {
            SourceConfig::SAPHANA(config) => Ok((
                config.host.as_str(),
                config.port,
                config.database.as_str(),
                config.schema.as_deref(),
            )),
            _ => Err(anyhow!("Expected SAP HANA configuration")),
        }
    }

    fn validate_identifier(value: &str, field: &str) -> Result<()> {
        if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(anyhow!(
                "Invalid {} '{}': only [A-Za-z0-9_] allowed",
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
        let (host, port, database, _schema) = Self::get_hana_config(source)?;

        if let Some(raw) = source.metadata.get("odbc_connection_string") {
            return Ok(Self::apply_credentials_to_connection_string(
                raw,
                credentials,
            ));
        }

        let driver = source
            .metadata
            .get("odbc_driver")
            .cloned()
            .or_else(|| std::env::var("GRAPHICA_HANA_ODBC_DRIVER").ok())
            .unwrap_or_else(|| "HDBODBC".to_string());

        let dsn = source
            .metadata
            .get("odbc_dsn")
            .cloned()
            .or_else(|| std::env::var("GRAPHICA_HANA_ODBC_DSN").ok());

        let mut conn = if let Some(dsn) = dsn {
            format!(
                "DSN={};UID={};PWD={}",
                dsn, credentials.username, credentials.password
            )
        } else {
            format!(
                "DRIVER={{{}}};SERVERNODE={}:{};DATABASE={};UID={};PWD={};",
                driver, host, port, database, credentials.username, credentials.password
            )
        };

        if let Some(options) = source.metadata.get("odbc_options") {
            if !options.is_empty() {
                if !conn.ends_with(';') {
                    conn.push(';');
                }
                conn.push_str(options);
            }
        }

        Ok(conn)
    }

    fn apply_credentials_to_connection_string(
        connection_string: &str,
        credentials: &Credentials,
    ) -> String {
        let mut conn = connection_string.to_string();
        let upper = conn.to_uppercase();
        if !upper.contains("UID=") {
            if !conn.ends_with(';') {
                conn.push(';');
            }
            conn.push_str(&format!("UID={}", credentials.username));
        }
        if !upper.contains("PWD=") {
            if !conn.ends_with(';') {
                conn.push(';');
            }
            conn.push_str(&format!("PWD={}", credentials.password));
        }
        conn
    }

    fn build_metadata_query(schema: &str, table_filter: Option<&str>) -> String {
        let upper_schema = schema.to_uppercase();
        let mut query = format!(
            r#"
SELECT
    t.SCHEMA_NAME as table_schema,
    t.TABLE_NAME as table_name,
    c.COLUMN_NAME as column_name,
    c.DATA_TYPE_NAME as data_type,
    CASE WHEN c.IS_NULLABLE = 'TRUE' THEN 'YES' ELSE 'NO' END as is_nullable,
    c.DEFAULT_VALUE as column_default,
    CASE WHEN pk.COLUMN_NAME IS NOT NULL THEN 'true' ELSE 'false' END as is_primary_key,
    t.RECORD_COUNT as estimated_row_count
FROM SYS.TABLES t
INNER JOIN SYS.COLUMNS c
    ON t.SCHEMA_NAME = c.SCHEMA_NAME
    AND t.TABLE_NAME = c.TABLE_NAME
LEFT JOIN (
    SELECT cc.SCHEMA_NAME, cc.TABLE_NAME, cc.COLUMN_NAME
    FROM SYS.CONSTRAINTS con
    INNER JOIN SYS.CONSTRAINT_COLUMNS cc
        ON con.CONSTRAINT_NAME = cc.CONSTRAINT_NAME
        AND con.SCHEMA_NAME = cc.SCHEMA_NAME
    WHERE con.CONSTRAINT_TYPE = 'PRIMARY KEY'
) pk
    ON c.SCHEMA_NAME = pk.SCHEMA_NAME
    AND c.TABLE_NAME = pk.TABLE_NAME
    AND c.COLUMN_NAME = pk.COLUMN_NAME
WHERE t.SCHEMA_NAME = '{}'
"#,
            upper_schema
        );

        if let Some(table) = table_filter {
            let upper_table = table.to_uppercase();
            query.push_str(&format!("  AND t.TABLE_NAME = '{}'\n", upper_table));
        }

        query.push_str("ORDER BY t.TABLE_NAME, c.POSITION");
        query
    }

    fn build_sample_query(schema: &str, table_name: &str, sample_size: usize) -> String {
        format!(
            r#"
SELECT *
FROM "{}"."{}"
LIMIT {}
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
    rc.CONSTRAINT_NAME as constraint_name,
    rc.TABLE_NAME as source_table,
    rc.COLUMN_NAME as source_column,
    rc.POSITION as column_position,
    rc.REFERENCED_TABLE_NAME as target_table,
    rc.REFERENCED_COLUMN_NAME as target_column
FROM SYS.REFERENTIAL_CONSTRAINTS rc
WHERE rc.SCHEMA_NAME = '{}'
"#,
            upper_schema
        );

        if let Some(table) = table_filter {
            let upper_table = table.to_uppercase();
            query.push_str(&format!("  AND rc.TABLE_NAME = '{}'\n", upper_table));
        }

        query.push_str("ORDER BY rc.CONSTRAINT_NAME, rc.POSITION");

        debug!("Built SAP HANA FK relationships query:\n{}", query);
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

impl Default for SAPHANAExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaExtractor for SAPHANAExtractor {
    async fn extract_metadata(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata> {
        info!("Extracting SAP HANA schema metadata via ODBC");
        let (_host, _port, _database, default_schema) = Self::get_hana_config(source)?;

        if let Some(schema) = schema_filter {
            Self::validate_identifier(schema, "schema_filter")?;
        }
        if let Some(table) = table_filter {
            Self::validate_identifier(table, "table_filter")?;
        }

        let schema_name = schema_filter
            .or(default_schema)
            .unwrap_or("PUBLIC")
            .to_uppercase();

        let query = Self::build_metadata_query(&schema_name, table_filter);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = execute_odbc_query(&connection_string, &query, true)
            .await
            .context("Failed to execute SAP HANA metadata query")?;

        let mut metadata = Self::map_metadata_results(rows, &schema_name)?;

        // Extract foreign key relationships
        let relationship_query = Self::build_relationships_query(&schema_name, table_filter);
        match execute_odbc_query(&connection_string, &relationship_query, true).await {
            Ok(relationship_rows) => {
                metadata.relationships = Self::map_relationship_results(relationship_rows)
                    .context("Failed to map SAP HANA relationship metadata")?;
                debug!(
                    "Extracted {} foreign key relationships from SAP HANA schema '{}'",
                    metadata.relationships.len(),
                    schema_name
                );
            }
            Err(e) => {
                warn!("Failed to extract SAP HANA relationships: {}", e);
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
        debug!("Extracting SAP HANA samples for table '{}'", table_name);
        Self::validate_identifier(table_name, "table_name")?;

        let (_host, _port, _database, default_schema) = Self::get_hana_config(source)?;
        let schema_name = default_schema.unwrap_or("PUBLIC").to_uppercase();

        let query = Self::build_sample_query(&schema_name, table_name, sample_size);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = execute_odbc_query(&connection_string, &query, false)
            .await
            .context("Failed to execute SAP HANA sample query")?;

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
            "Extracting SAP HANA statistics for {}.{}",
            table_name, column_name
        );
        Self::validate_identifier(table_name, "table_name")?;
        Self::validate_identifier(column_name, "column_name")?;

        let (_host, _port, _database, default_schema) = Self::get_hana_config(source)?;
        let schema_name = default_schema.unwrap_or("PUBLIC").to_uppercase();

        let query = Self::build_statistics_query(&schema_name, table_name, column_name);
        let connection_string = Self::build_connection_string(source, credentials)?;

        let rows = match execute_odbc_query(&connection_string, &query, true).await {
            Ok(rows) => rows,
            Err(e) => {
                warn!(
                    "SAP HANA statistics query failed for {}.{}: {}. Returning defaults.",
                    table_name, column_name, e
                );
                return Ok(ColumnStats::default());
            }
        };

        Self::map_statistics_results(rows)
    }

    fn name(&self) -> &'static str {
        "SAPHANAExtractor"
    }

    fn supports_source(&self, source_type: &str) -> bool {
        let lower = source_type.to_lowercase();
        lower == "saphana" || lower == "sap_hana" || lower == "sap hana"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_hana_aliases() {
        let extractor = SAPHANAExtractor::new();
        assert!(extractor.supports_source("saphana"));
        assert!(extractor.supports_source("sap_hana"));
        assert!(extractor.supports_source("sap hana"));
        assert!(!extractor.supports_source("oracle"));
    }

    #[test]
    fn validates_identifier_rules() {
        assert!(SAPHANAExtractor::validate_identifier("CUSTOMER_ID", "column").is_ok());
        assert!(SAPHANAExtractor::validate_identifier("bad-name", "column").is_err());
    }
}
