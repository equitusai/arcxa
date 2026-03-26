//! Database Extract executor - Extract data from registered datasources
//!
//! Supports incremental extraction with watermark tracking.
//! Integrates with the DataSourceCatalog to execute queries against registered datasources.

use anyhow::{Context, Result};
use graphica_core::catalog::api_types::SchemaDefinition;
use graphica_core::catalog::client::DataSourceCatalog;
use graphica_core::orchestration::workflow::DbExtractConfig;
use graphica_core::security::validate_identifier;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::common::datasource_readiness::{evaluate_datasource_readiness, DatasourceOperation};

/// Database Extract executor
pub struct DbExtractExecutor {
    config: DbExtractConfig,
    catalog: Option<Arc<dyn DataSourceCatalog + Send + Sync>>,
}

impl DbExtractExecutor {
    pub fn new(config: DbExtractConfig) -> Self {
        Self {
            config,
            catalog: None,
        }
    }

    /// Create with datasource catalog for actual extraction
    pub fn with_catalog(
        config: DbExtractConfig,
        catalog: Arc<dyn DataSourceCatalog + Send + Sync>,
    ) -> Self {
        Self {
            config,
            catalog: Some(catalog),
        }
    }

    /// Extract data from datasource
    pub async fn extract(&self) -> Result<Vec<Value>> {
        let catalog = self
            .catalog
            .as_ref()
            .context("DataSourceCatalog not configured - use with_catalog() to provide catalog")?;

        // Build query based on config and source type
        let source = catalog
            .get_source(&self.config.datasource_id)
            .await
            .context("Failed to load datasource for query generation")?;
        evaluate_datasource_readiness(&source, DatasourceOperation::WorkflowRead)
            .map_err(|failure| anyhow::anyhow!(failure.message))?;
        let source = source.source;
        let query = self.build_query_for_source(Some(&source.source_type))?;

        // Execute query against datasource
        let parameters = HashMap::new(); // TODO: Support parameterized queries
        let limit = Some(self.config.batch_size);

        let result = catalog
            .execute_query(&self.config.datasource_id, &query, parameters, limit)
            .await
            .context(format!(
                "Failed to execute query against datasource {}",
                self.config.datasource_id
            ))?;

        // QueryResult.rows is already Vec<Value>, so just use it directly
        let records = result.rows;

        tracing::info!(
            "Extracted {} records from datasource {} (table: {:?}, query: {:?})",
            records.len(),
            self.config.datasource_id,
            self.config.table_name,
            self.config.query.as_deref().map(|q| &q[..50.min(q.len())])
        );

        Ok(records)
    }

    /// Build SQL query from config
    fn build_query(&self) -> Result<String> {
        self.build_query_for_source(None)
    }

    fn build_query_for_source(&self, source_type: Option<&str>) -> Result<String> {
        // If explicit query provided, use it
        if let Some(ref query) = self.config.query {
            return Ok(query.clone());
        }

        // Otherwise, build query from table_name
        let table_name = self
            .config
            .table_name
            .as_ref()
            .context("Either query or table_name must be provided")?;

        // Validate table name to prevent SQL injection
        let validated_table = validate_identifier(table_name).context(format!(
            "Invalid table name '{}': Table names must be alphanumeric with underscores only",
            table_name
        ))?;

        // Build column list with validation
        let columns = if let Some(ref cols) = self.config.columns {
            if cols.is_empty() {
                "*".to_string()
            } else {
                // Validate each column name to prevent SQL injection
                let validated_cols: Result<Vec<&str>, _> =
                    cols.iter().map(|c| validate_identifier(c)).collect();

                validated_cols
                    .context("Invalid column name in config")?
                    .join(", ")
            }
        } else {
            "*".to_string()
        };

        // Build base query with validated identifier
        let mut query = format!("SELECT {} FROM {}", columns, validated_table);

        // Add incremental WHERE clause if configured
        if self.config.incremental.unwrap_or(false) {
            if let Some(ref incremental_column) = self.config.incremental_column {
                // Validate incremental column name to prevent SQL injection
                let validated_incremental_col = validate_identifier(incremental_column)
                    .context(format!(
                        "Invalid incremental column name '{}': Column names must be alphanumeric with underscores only",
                        incremental_column
                    ))?;

                if let Some(ref last_value) = self.config.last_value {
                    // Add WHERE clause for incremental extraction
                    let watermark_condition = match last_value {
                        Value::String(s) => format!("{} > '{}'", validated_incremental_col, s),
                        Value::Number(n) => format!("{} > {}", validated_incremental_col, n),
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Unsupported incremental value type: {}",
                                last_value
                            ))
                        }
                    };
                    query.push_str(&format!(" WHERE {}", watermark_condition));
                }

                // Add ORDER BY for incremental tracking
                query.push_str(&format!(" ORDER BY {} ASC", validated_incremental_col));
            }
        }

        // Add LIMIT / FETCH FIRST depending on source type
        match Self::limit_syntax(source_type) {
            LimitSyntax::FetchFirst => {
                query.push_str(&format!(
                    " FETCH FIRST {} ROWS ONLY",
                    self.config.batch_size
                ));
            }
            LimitSyntax::Limit => {
                query.push_str(&format!(" LIMIT {}", self.config.batch_size));
            }
        }

        Ok(query)
    }

    fn limit_syntax(source_type: Option<&str>) -> LimitSyntax {
        let lower = source_type.unwrap_or_default().to_lowercase();
        if lower.contains("oracle") || lower.contains("db2") {
            LimitSyntax::FetchFirst
        } else {
            LimitSyntax::Limit
        }
    }
}

enum LimitSyntax {
    Limit,
    FetchFirst,
}

#[async_trait::async_trait]
impl crate::etl::EtlExecutor for DbExtractExecutor {
    async fn execute(&self, _input: Value) -> Result<Value> {
        let records = self.extract().await?;
        let record_count = records.len();

        // Extract new watermark if incremental mode is enabled
        let mut response = json!({
            "records": records,
            "count": record_count,
            "datasource_id": self.config.datasource_id,
            "table_name": self.config.table_name,
            "incremental": self.config.incremental,
        });

        // Optionally include schema metadata for ontology mapping
        if self.config.include_schema.unwrap_or(false) {
            let catalog = self.catalog.as_ref().context(
                "DataSourceCatalog not configured - use with_catalog() to provide catalog",
            )?;

            let schema_table = self
                .config
                .schema_table
                .as_ref()
                .or(self.config.table_name.as_ref())
                .context("schema_table or table_name required for schema inference")?;

            let sample_size = self.config.schema_sample_size.unwrap_or(1000);

            let schema = catalog
                .infer_schema(&self.config.datasource_id, Some(schema_table), sample_size)
                .await
                .context("Failed to infer schema for db_extract")?;

            response["schema"] = schema_definition_to_payload(&schema, Some(schema_table));
        }

        // If incremental mode and we have records, extract the new incremental value
        if self.config.incremental.unwrap_or(false) && record_count > 0 {
            if let Some(ref incremental_column) = self.config.incremental_column {
                if let Some(last_record) = records.last() {
                    if let Some(incremental_value) = last_record.get(incremental_column) {
                        response["new_incremental_value"] = incremental_value.clone();
                    }
                }
            }
        }

        Ok(response)
    }

    fn step_type(&self) -> &'static str {
        "db_extract"
    }
}

fn schema_definition_to_payload(
    schema: &SchemaDefinition,
    table_name: Option<&str>,
) -> serde_json::Value {
    let selected_table = table_name.and_then(|name| {
        schema
            .tables
            .iter()
            .find(|table| table.name.eq_ignore_ascii_case(name))
    });

    let (table_name, fields) = if let Some(table) = selected_table {
        (Some(table.name.clone()), table_to_fields(table))
    } else if schema.tables.len() == 1 {
        let table = &schema.tables[0];
        (Some(table.name.clone()), table_to_fields(table))
    } else {
        (table_name.map(|t| t.to_string()), Vec::new())
    };

    json!({
        "name": schema.name,
        "table": table_name,
        "fields": fields,
    })
}

fn table_to_fields(
    table: &graphica_core::catalog::api_types::TableDefinition,
) -> Vec<serde_json::Value> {
    table
        .columns
        .iter()
        .map(|col| {
            json!({
                "name": col.name,
                "type": col.data_type,
                "nullable": col.nullable,
                "primary_key": col.primary_key,
                "default": col.default_value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_query_from_table() {
        let config = DbExtractConfig {
            datasource_id: "ds1".to_string(),
            table_name: Some("users".to_string()),
            schema_table: None,
            query: None,
            columns: Some(vec!["id".to_string(), "name".to_string()]),
            incremental: None,
            incremental_column: None,
            last_value: None,
            batch_size: 1000,
            include_schema: None,
            schema_sample_size: None,
        };

        let executor = DbExtractExecutor::new(config);
        let query = executor.build_query().unwrap();

        assert_eq!(query, "SELECT id, name FROM users LIMIT 1000");
    }

    #[test]
    fn test_build_query_with_all_columns() {
        let config = DbExtractConfig {
            datasource_id: "ds1".to_string(),
            table_name: Some("users".to_string()),
            schema_table: None,
            query: None,
            columns: Some(vec![]),
            incremental: None,
            incremental_column: None,
            last_value: None,
            batch_size: 500,
            include_schema: None,
            schema_sample_size: None,
        };

        let executor = DbExtractExecutor::new(config);
        let query = executor.build_query().unwrap();

        assert_eq!(query, "SELECT * FROM users LIMIT 500");
    }

    #[test]
    fn test_build_query_incremental() {
        let config = DbExtractConfig {
            datasource_id: "ds1".to_string(),
            table_name: Some("users".to_string()),
            schema_table: None,
            query: None,
            columns: None,
            incremental: Some(true),
            incremental_column: Some("updated_at".to_string()),
            last_value: Some(json!("2024-01-01T00:00:00Z")),
            batch_size: 1000,
            include_schema: None,
            schema_sample_size: None,
        };

        let executor = DbExtractExecutor::new(config);
        let query = executor.build_query().unwrap();

        assert_eq!(
            query,
            "SELECT * FROM users WHERE updated_at > '2024-01-01T00:00:00Z' ORDER BY updated_at ASC LIMIT 1000"
        );
    }

    #[test]
    fn test_build_query_custom() {
        let config = DbExtractConfig {
            datasource_id: "ds1".to_string(),
            table_name: None,
            schema_table: None,
            query: Some("SELECT * FROM users WHERE active = true".to_string()),
            columns: None,
            incremental: None,
            incremental_column: None,
            last_value: None,
            batch_size: 1000,
            include_schema: None,
            schema_sample_size: None,
        };

        let executor = DbExtractExecutor::new(config);
        let query = executor.build_query().unwrap();

        assert_eq!(query, "SELECT * FROM users WHERE active = true");
    }
}
