//! Databricks Connector
//!
//! Scaffold connector for Databricks SQL Warehouse / Lakehouse endpoints.
//! This connector currently provides validated configuration, test metadata,
//! and mock query/schema responses as an integration seam.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{DataSource, DatabricksConfig, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct DatabricksConnector;

impl DatabricksConnector {
    pub fn new() -> Self {
        Self
    }

    fn extract_config(source: &DataSource) -> ConnectorResult<&DatabricksConfig> {
        match &source.connection.config {
            SourceConfig::Databricks(config) => Ok(config),
            _ => Err(GraphicaError::Configuration(
                "Expected Databricks configuration".to_string(),
            )),
        }
    }
}

impl Default for DatabricksConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for DatabricksConnector {
    fn name(&self) -> &'static str {
        "Databricks Connector"
    }

    fn source_type(&self) -> &'static str {
        "Databricks"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::Databricks(dbx) => {
                let mut errors = vec![];
                if dbx.workspace_url.is_empty() {
                    errors.push("workspaceUrl cannot be empty".to_string());
                } else if !dbx.workspace_url.starts_with("https://") {
                    errors.push("workspaceUrl must start with https://".to_string());
                }
                if dbx.http_path.is_empty() {
                    errors.push("httpPath cannot be empty".to_string());
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected Databricks configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = Self::extract_config(source)?;
        let started = std::time::Instant::now();

        let token = credentials
            .additional
            .get("token")
            .or_else(|| credentials.additional.get("access_token"))
            .cloned()
            .or_else(|| {
                if credentials.password.is_empty() {
                    None
                } else {
                    Some(credentials.password.clone())
                }
            });

        let duration_ms = started.elapsed().as_millis() as u64;

        let Some(token) = token else {
            return Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(
                    "Databricks token missing: provide credentials.additional['token'] or password"
                        .to_string(),
                ),
                metadata: HashMap::from([
                    ("workspaceUrl".to_string(), config.workspace_url.clone()),
                    ("httpPath".to_string(), config.http_path.clone()),
                ]),
                tested_at: Utc::now(),
            });
        };

        let base_url = config.workspace_url.trim_end_matches('/');
        let url = format!("{}/api/2.0/workspace/get-status?path=/", base_url);
        let client = Client::new();

        let response = client.get(url).bearer_auth(token).send().await;

        let (success, error) = match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    (true, None)
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    (
                        false,
                        Some(format!(
                            "Databricks connection failed: status {} {}",
                            status, body
                        )),
                    )
                }
            }
            Err(e) => (false, Some(format!("Databricks request failed: {}", e))),
        };

        Ok(ConnectionTestResult {
            success,
            duration_ms,
            error,
            metadata: HashMap::from([
                ("workspaceUrl".to_string(), config.workspace_url.clone()),
                ("httpPath".to_string(), config.http_path.clone()),
                (
                    "catalog".to_string(),
                    config.catalog.clone().unwrap_or_else(|| "main".to_string()),
                ),
            ]),
            tested_at: Utc::now(),
        })
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        table_name: Option<&str>,
        _sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = Self::extract_config(source)?;

        tracing::warn!(
            "Databricks schema inference requested before implementation: schema={} table={:?}",
            config
                .schema
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            table_name
        );

        Err(GraphicaError::Configuration(
            "Databricks schema inference is not yet implemented for catalog connectors".to_string(),
        ))
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let _config = Self::extract_config(source)?;
        tracing::info!(
            "Databricks query execution requested before implementation: query='{}' limit={:?}",
            query,
            limit
        );

        Err(GraphicaError::Configuration(
            "Databricks query execution is not yet implemented for catalog connectors".to_string(),
        ))
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: false,
            query_timeout: false,
            streaming: false,
            transactions: false,
            max_batch_size: Some(200_000),
        }
    }
}
