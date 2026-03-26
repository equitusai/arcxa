//! Databricks schema extractor backed by catalog connectors.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::connectors::databricks::DatabricksSqlClient;
use graphica_core::catalog::connectors::ConnectorRegistry;
use graphica_core::catalog::types::{DataSource, SourceConfig};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::super::types::*;
use super::shared::{
    parse_column_stats_from_query, parse_null_fraction_from_counts, query_result_to_sample_rows,
    schema_definition_to_metadata,
};
use super::traits::SchemaExtractor;

/// Databricks extractor using core connector abstraction.
pub struct DatabricksExtractor {
    registry: ConnectorRegistry,
}

impl DatabricksExtractor {
    pub fn new() -> Self {
        Self {
            registry: ConnectorRegistry::new(),
        }
    }

    fn get_databricks_config(source: &DataSource) -> Result<Option<&str>> {
        match &source.connection.config {
            SourceConfig::Databricks(config) => Ok(config.schema.as_deref()),
            _ => Err(anyhow!("Expected Databricks configuration")),
        }
    }

    fn validate_identifier(value: &str, field: &str) -> Result<()> {
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            return Err(anyhow!(
                "Invalid {} '{}': only [A-Za-z0-9_.] allowed",
                field,
                value
            ));
        }
        Ok(())
    }

    fn quote_table_identifier(value: &str) -> Result<String> {
        DatabricksSqlClient::quote_identifier(value).map_err(|error| anyhow!(error))
    }

    fn quote_column_identifier(value: &str) -> Result<String> {
        let segment = DatabricksSqlClient::sanitize_identifier_segment(value)
            .map_err(|error| anyhow!(error))?;
        Ok(format!("`{segment}`"))
    }
}

impl Default for DatabricksExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaExtractor for DatabricksExtractor {
    async fn extract_metadata(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata> {
        info!("Extracting Databricks schema metadata via connector registry");
        let default_schema = Self::get_databricks_config(source)?;

        if let Some(schema) = schema_filter {
            Self::validate_identifier(schema, "schema_filter")?;
        }
        if let Some(table) = table_filter {
            Self::validate_identifier(table, "table_filter")?;
        }

        let connector = self
            .registry
            .get("Databricks")
            .map_err(|e| anyhow!("Databricks connector unavailable: {}", e))?;

        let schema = connector
            .infer_schema(source, credentials.clone(), table_filter, 1000)
            .await
            .context("Databricks schema inference failed")?;

        let mut metadata = schema_definition_to_metadata(schema);
        if let Some(schema_name) = schema_filter.or(default_schema) {
            metadata.schema_name = schema_name.to_string();
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
        debug!("Extracting Databricks samples for table '{}'", table_name);
        Self::validate_identifier(table_name, "table_name")?;

        let connector = self
            .registry
            .get("Databricks")
            .map_err(|e| anyhow!("Databricks connector unavailable: {}", e))?;

        let quoted_table = Self::quote_table_identifier(table_name)?;
        let query = format!("SELECT * FROM {}", quoted_table);
        let result = connector
            .execute_query(
                source,
                credentials.clone(),
                &query,
                HashMap::new(),
                Some(sample_size),
                30,
            )
            .await
            .context("Databricks sample query failed")?;

        Ok(query_result_to_sample_rows(result))
    }

    async fn extract_statistics(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        column_name: &str,
    ) -> Result<ColumnStats> {
        debug!(
            "Extracting Databricks statistics for {}.{}",
            table_name, column_name
        );
        Self::validate_identifier(table_name, "table_name")?;
        Self::validate_identifier(column_name, "column_name")?;

        let connector = self
            .registry
            .get("Databricks")
            .map_err(|e| anyhow!("Databricks connector unavailable: {}", e))?;

        let quoted_table = Self::quote_table_identifier(table_name)?;
        let quoted_column = Self::quote_column_identifier(column_name)?;
        let query = format!(
            r#"
SELECT
    COUNT(DISTINCT {col}) AS distinct_count,
    SUM(CASE WHEN {col} IS NULL THEN 1 ELSE 0 END) AS null_count,
    COUNT(*) AS total_count
FROM {table}
"#,
            table = quoted_table,
            col = quoted_column
        );

        let result = match connector
            .execute_query(
                source,
                credentials.clone(),
                &query,
                HashMap::new(),
                Some(1),
                30,
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    "Databricks statistics query failed for {}.{}: {}. Returning defaults.",
                    table_name, column_name, e
                );
                return Ok(ColumnStats::default());
            }
        };

        let mut stats = parse_column_stats_from_query(&result);
        if stats.null_fraction == 0.0 {
            if let Some(fraction) = parse_null_fraction_from_counts(&result) {
                stats.null_fraction = fraction;
            }
        }
        Ok(stats)
    }

    fn name(&self) -> &'static str {
        "DatabricksExtractor"
    }

    fn supports_source(&self, source_type: &str) -> bool {
        let lower = source_type.to_lowercase();
        lower == "databricks" || lower == "databricks_sql" || lower == "databricks sql"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_databricks_aliases() {
        let extractor = DatabricksExtractor::new();
        assert!(extractor.supports_source("databricks"));
        assert!(extractor.supports_source("databricks_sql"));
        assert!(extractor.supports_source("databricks sql"));
        assert!(!extractor.supports_source("oracle"));
    }

    #[test]
    fn validates_identifier_rules() {
        assert!(DatabricksExtractor::validate_identifier("bronze.events", "table").is_ok());
        assert!(DatabricksExtractor::validate_identifier("bad-name", "table").is_err());
    }

    #[test]
    fn quotes_multi_part_table_identifiers() {
        assert_eq!(
            DatabricksExtractor::quote_table_identifier("main.bronze.events").unwrap(),
            "`main`.`bronze`.`events`"
        );
    }
}
