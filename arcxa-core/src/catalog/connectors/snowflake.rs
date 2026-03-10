//! Snowflake Connector
//!
//! Full implementation using Snowflake SQL REST API

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{DataSource, SnowflakeConfig, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct SnowflakeConnector {
    client: Client,
}

impl SnowflakeConnector {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// Build Snowflake account URL
    fn build_account_url(&self, config: &SnowflakeConfig) -> String {
        // Format: https://<account>.snowflakecomputing.com
        format!("https://{}.snowflakecomputing.com", config.account)
    }

    /// Execute a SQL statement using Snowflake SQL API
    async fn execute_statement(
        &self,
        config: &SnowflakeConfig,
        credentials: &Credentials,
        sql: &str,
        timeout: u64,
    ) -> ConnectorResult<SnowflakeResponse> {
        let url = format!("{}/api/v2/statements", self.build_account_url(config));

        // Create basic auth header
        let auth_header = format!("{}:{}", credentials.username, credentials.password);
        let encoded = general_purpose::STANDARD.encode(auth_header.as_bytes());

        let request_body = SnowflakeStatementRequest {
            statement: sql.to_string(),
            timeout: timeout as i64,
            database: Some(config.database.clone()),
            schema: config.schema.clone(),
            warehouse: Some(config.warehouse.clone()),
            role: config.role.clone(),
            parameters: HashMap::new(),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Basic {}", encoded))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                GraphicaError::Internal(format!("Failed to execute Snowflake query: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(GraphicaError::Internal(format!(
                "Snowflake query failed with status {}: {}",
                status, error_text
            )));
        }

        let sf_response: SnowflakeResponse = response.json().await.map_err(|e| {
            GraphicaError::Internal(format!("Failed to parse Snowflake response: {}", e))
        })?;

        Ok(sf_response)
    }

    /// Map Snowflake data type to standard data type
    fn map_snowflake_type(sf_type: &str) -> String {
        let sf_upper = sf_type.to_uppercase();
        match sf_upper.as_str() {
            t if t.starts_with("NUMBER")
                || t.starts_with("DECIMAL")
                || t.starts_with("NUMERIC") =>
            {
                "NUMERIC".to_string()
            }
            t if t.starts_with("INT")
                || t.starts_with("BIGINT")
                || t.starts_with("SMALLINT")
                || t.starts_with("TINYINT") =>
            {
                "INTEGER".to_string()
            }
            t if t.starts_with("FLOAT") || t.starts_with("DOUBLE") || t.starts_with("REAL") => {
                "FLOAT".to_string()
            }
            t if t.starts_with("VARCHAR")
                || t.starts_with("CHAR")
                || t.starts_with("STRING")
                || t.starts_with("TEXT") =>
            {
                "VARCHAR".to_string()
            }
            "BOOLEAN" => "BOOLEAN".to_string(),
            "DATE" => "DATE".to_string(),
            t if t.starts_with("TIMESTAMP") || t.starts_with("DATETIME") => "TIMESTAMP".to_string(),
            "TIME" => "TIME".to_string(),
            t if t.starts_with("BINARY") || t.starts_with("VARBINARY") => "BINARY".to_string(),
            "VARIANT" | "OBJECT" | "ARRAY" => "JSON".to_string(),
            _ => sf_type.to_string(),
        }
    }
}

impl Default for SnowflakeConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for SnowflakeConnector {
    fn name(&self) -> &'static str {
        "Snowflake Connector"
    }

    fn source_type(&self) -> &'static str {
        "Snowflake"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::Snowflake(sf_config) => {
                let mut errors = vec![];

                if sf_config.account.is_empty() {
                    errors.push("Account identifier cannot be empty".to_string());
                }

                // Validate account format (should not include protocol or domain)
                if sf_config.account.contains("://")
                    || sf_config.account.contains(".snowflakecomputing.com")
                {
                    errors.push("Account should be just the account identifier (e.g., 'xy12345' or 'xy12345.us-east-1'), not a full URL".to_string());
                }

                if sf_config.warehouse.is_empty() {
                    errors.push("Warehouse cannot be empty".to_string());
                }

                if sf_config.database.is_empty() {
                    errors.push("Database cannot be empty".to_string());
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected Snowflake configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = match &source.connection.config {
            SourceConfig::Snowflake(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Invalid Snowflake configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();

        // Test connection with a simple query
        let result = self
            .execute_statement(
                config,
                &credentials,
                "SELECT CURRENT_VERSION(), CURRENT_DATABASE(), CURRENT_WAREHOUSE(), CURRENT_ROLE()",
                10,
            )
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let mut metadata = HashMap::new();

                // Extract version and context info from first row if available
                if let Some(first_row) = response.data.first() {
                    if first_row.len() >= 4 {
                        metadata.insert("version".to_string(), first_row[0].to_string());
                        metadata.insert("database".to_string(), first_row[1].to_string());
                        metadata.insert("warehouse".to_string(), first_row[2].to_string());
                        metadata.insert("role".to_string(), first_row[3].to_string());
                    }
                }

                Ok(ConnectionTestResult {
                    success: true,
                    duration_ms,
                    error: None,
                    metadata,
                    tested_at: Utc::now(),
                })
            }
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(format!("Connection test failed: {}", e)),
                metadata: HashMap::new(),
                tested_at: Utc::now(),
            }),
        }
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: Option<&str>,
        _sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = match &source.connection.config {
            SourceConfig::Snowflake(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Invalid Snowflake configuration".to_string(),
                ))
            }
        };

        let schema_name = config.schema.as_deref().unwrap_or("PUBLIC");

        // Query INFORMATION_SCHEMA to get table and column metadata
        let sql = if let Some(table) = table_name {
            format!(
                r#"
                SELECT
                    c.TABLE_NAME,
                    c.COLUMN_NAME,
                    c.DATA_TYPE,
                    c.IS_NULLABLE,
                    c.ORDINAL_POSITION,
                    c.COLUMN_DEFAULT,
                    c.CHARACTER_MAXIMUM_LENGTH,
                    c.NUMERIC_PRECISION,
                    c.NUMERIC_SCALE,
                    t.ROW_COUNT,
                    t.COMMENT as TABLE_COMMENT,
                    c.COMMENT as COLUMN_COMMENT
                FROM INFORMATION_SCHEMA.COLUMNS c
                LEFT JOIN INFORMATION_SCHEMA.TABLES t
                    ON c.TABLE_SCHEMA = t.TABLE_SCHEMA
                    AND c.TABLE_NAME = t.TABLE_NAME
                WHERE c.TABLE_SCHEMA = '{}'
                    AND c.TABLE_NAME = '{}'
                    AND t.TABLE_TYPE = 'BASE TABLE'
                ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION
                "#,
                schema_name, table
            )
        } else {
            format!(
                r#"
                SELECT
                    c.TABLE_NAME,
                    c.COLUMN_NAME,
                    c.DATA_TYPE,
                    c.IS_NULLABLE,
                    c.ORDINAL_POSITION,
                    c.COLUMN_DEFAULT,
                    c.CHARACTER_MAXIMUM_LENGTH,
                    c.NUMERIC_PRECISION,
                    c.NUMERIC_SCALE,
                    t.ROW_COUNT,
                    t.COMMENT as TABLE_COMMENT,
                    c.COMMENT as COLUMN_COMMENT
                FROM INFORMATION_SCHEMA.COLUMNS c
                LEFT JOIN INFORMATION_SCHEMA.TABLES t
                    ON c.TABLE_SCHEMA = t.TABLE_SCHEMA
                    AND c.TABLE_NAME = t.TABLE_NAME
                WHERE c.TABLE_SCHEMA = '{}'
                    AND t.TABLE_TYPE = 'BASE TABLE'
                ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION
                "#,
                schema_name
            )
        };

        let response = self
            .execute_statement(config, &credentials, &sql, 30)
            .await?;

        // Group columns by table
        let mut tables_map: HashMap<String, Vec<ColumnDefinition>> = HashMap::new();
        let mut table_row_counts: HashMap<String, Option<u64>> = HashMap::new();

        for row in response.data {
            if row.len() >= 12 {
                let table_name = row[0].as_str().unwrap_or("").to_string();
                let column_name = row[1].as_str().unwrap_or("").to_string();
                let data_type = row[2].as_str().unwrap_or("").to_string();
                let is_nullable = row[3].as_str().unwrap_or("YES") == "YES";
                let row_count = row[9].as_u64();

                let column = ColumnDefinition {
                    name: column_name,
                    data_type: Self::map_snowflake_type(&data_type),
                    nullable: is_nullable,
                    primary_key: false, // Would need separate query for PKs
                    default_value: row[5].as_str().map(|s| s.to_string()),
                    semantic_type: None,
                    statistics: None, // Could be populated from INFORMATION_SCHEMA stats
                };

                tables_map
                    .entry(table_name.clone())
                    .or_insert_with(Vec::new)
                    .push(column);
                table_row_counts.entry(table_name).or_insert(row_count);
            }
        }

        // Convert to TableDefinition
        let tables: Vec<TableDefinition> = tables_map
            .into_iter()
            .map(|(name, columns)| TableDefinition {
                name: name.clone(),
                columns,
                estimated_rows: table_row_counts.get(&name).and_then(|r| *r),
            })
            .collect();

        Ok(SchemaDefinition {
            name: format!("{}.{}", config.database, schema_name),
            tables,
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        credentials: Credentials,
        query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let config = match &source.connection.config {
            SourceConfig::Snowflake(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Invalid Snowflake configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();

        // Add LIMIT clause if specified
        let final_query = if let Some(lim) = limit {
            if query.trim().to_uppercase().contains("LIMIT") {
                query.to_string()
            } else {
                format!("{} LIMIT {}", query.trim().trim_end_matches(';'), lim)
            }
        } else {
            query.to_string()
        };

        let response = self
            .execute_statement(config, &credentials, &final_query, timeout_secs)
            .await?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        // Convert column metadata
        let columns: Option<Vec<ColumnDefinition>> =
            if !response.result_set_meta_data.row_type.is_empty() {
                Some(
                    response
                        .result_set_meta_data
                        .row_type
                        .iter()
                        .map(|col| ColumnDefinition {
                            name: col.name.clone(),
                            data_type: Self::map_snowflake_type(&col.sf_type),
                            nullable: col.nullable,
                            primary_key: false,
                            default_value: None,
                            semantic_type: None,
                            statistics: None,
                        })
                        .collect(),
                )
            } else {
                None
            };

        // Convert rows to JSON
        let rows: Vec<serde_json::Value> = response
            .data
            .into_iter()
            .map(|row| {
                let mut row_map = serde_json::Map::new();
                for (idx, value) in row.into_iter().enumerate() {
                    if let Some(cols) = &columns {
                        if let Some(col) = cols.get(idx) {
                            row_map.insert(col.name.clone(), value);
                        }
                    }
                }
                serde_json::Value::Object(row_map)
            })
            .collect();

        let row_count = rows.len();
        let truncated = limit.is_some() && row_count == limit.unwrap();

        Ok(QueryResult {
            rows,
            row_count,
            execution_time_ms,
            truncated,
            columns,
        })
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: true,
            query_timeout: true,
            streaming: false, // REST API doesn't support streaming in the traditional sense
            transactions: true,
            max_batch_size: Some(100000),
        }
    }
}

// Snowflake SQL API Types

#[derive(Debug, Serialize)]
struct SnowflakeStatementRequest {
    statement: String,
    timeout: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warehouse: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    parameters: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct SnowflakeResponse {
    #[serde(default)]
    data: Vec<Vec<serde_json::Value>>,
    #[serde(rename = "resultSetMetaData")]
    result_set_meta_data: ResultSetMetaData,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ResultSetMetaData {
    #[serde(rename = "rowType", default)]
    row_type: Vec<ColumnMetadata>,
}

#[derive(Debug, Deserialize)]
struct ColumnMetadata {
    name: String,
    #[serde(rename = "type")]
    sf_type: String,
    nullable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_type_mapping() {
        assert_eq!(
            SnowflakeConnector::map_snowflake_type("NUMBER(38,0)"),
            "NUMERIC"
        );
        assert_eq!(
            SnowflakeConnector::map_snowflake_type("VARCHAR(16777216)"),
            "VARCHAR"
        );
        assert_eq!(
            SnowflakeConnector::map_snowflake_type("TIMESTAMP_NTZ"),
            "TIMESTAMP"
        );
        assert_eq!(SnowflakeConnector::map_snowflake_type("BOOLEAN"), "BOOLEAN");
        assert_eq!(SnowflakeConnector::map_snowflake_type("VARIANT"), "JSON");
        assert_eq!(SnowflakeConnector::map_snowflake_type("DATE"), "DATE");
    }

    #[test]
    fn test_account_url_building() {
        let connector = SnowflakeConnector::new();
        let config = SnowflakeConfig {
            account: "xy12345.us-east-1".to_string(),
            warehouse: "COMPUTE_WH".to_string(),
            database: "PROD_DB".to_string(),
            schema: Some("PUBLIC".to_string()),
            role: Some("ANALYST".to_string()),
        };

        let url = connector.build_account_url(&config);
        assert_eq!(url, "https://xy12345.us-east-1.snowflakecomputing.com");
    }

    #[test]
    fn test_config_validation() {
        let connector = SnowflakeConnector::new();

        // Valid config
        let valid_config = SourceConfig::Snowflake(SnowflakeConfig {
            account: "xy12345".to_string(),
            warehouse: "COMPUTE_WH".to_string(),
            database: "DB".to_string(),
            schema: None,
            role: None,
        });
        assert!(connector.validate_config(&valid_config).unwrap().valid);

        // Invalid: empty account
        let invalid_config = SourceConfig::Snowflake(SnowflakeConfig {
            account: "".to_string(),
            warehouse: "COMPUTE_WH".to_string(),
            database: "DB".to_string(),
            schema: None,
            role: None,
        });
        assert!(!connector.validate_config(&invalid_config).unwrap().valid);

        // Invalid: account includes protocol
        let invalid_url_config = SourceConfig::Snowflake(SnowflakeConfig {
            account: "https://xy12345.snowflakecomputing.com".to_string(),
            warehouse: "COMPUTE_WH".to_string(),
            database: "DB".to_string(),
            schema: None,
            role: None,
        });
        let result = connector.validate_config(&invalid_url_config).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("not a full URL")));
    }
}
