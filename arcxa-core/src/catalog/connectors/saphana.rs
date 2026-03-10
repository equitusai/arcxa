//! SAP HANA Connector

use async_trait::async_trait;
use chrono::Utc;
use odbc_api::{ConnectionOptions, Environment};
use std::collections::HashMap;
use tokio::time::{timeout, Duration};

use crate::catalog::{
    api_types::{ConnectionTestResult, QueryResult, SchemaDefinition},
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{DataSource, SAPHANAConfig, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct SAPHANAConnector;

impl SAPHANAConnector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SAPHANAConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for SAPHANAConnector {
    fn name(&self) -> &'static str {
        "SAP HANA Connector"
    }

    fn source_type(&self) -> &'static str {
        "SAPHANA"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::SAPHANA(hana_config) => {
                let mut errors = vec![];
                if hana_config.host.is_empty() {
                    errors.push("Host cannot be empty".to_string());
                }
                if hana_config.database.is_empty() {
                    errors.push("Database cannot be empty".to_string());
                }
                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected SAP HANA configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = match &source.connection.config {
            SourceConfig::SAPHANA(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected SAP HANA configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();

        let driver = credentials
            .additional
            .get("odbc_driver")
            .cloned()
            .unwrap_or_else(|| "HDBODBC".to_string());

        let mut connection_string = format!(
            "DRIVER={{{}}};SERVERNODE={}:{};UID={};PWD={};",
            driver, config.host, config.port, credentials.username, credentials.password
        );

        if !config.database.is_empty() {
            connection_string.push_str(&format!("DATABASENAME={};", config.database));
        }

        let connect = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let _conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("HANA connection failed: {:?}", e)))?;

            Ok::<(), GraphicaError>(())
        });

        let result = match timeout(Duration::from_secs(5), connect).await {
            Ok(res) => match res {
                Ok(inner) => Ok(inner),
                Err(e) => Err(GraphicaError::Internal(format!(
                    "HANA connection task failed: {}",
                    e
                ))),
            },
            Err(_) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                return Ok(ConnectionTestResult {
                    success: false,
                    duration_ms,
                    error: Some("HANA connection test timed out".to_string()),
                    metadata: HashMap::from([
                        ("host".to_string(), config.host.clone()),
                        ("port".to_string(), config.port.to_string()),
                        ("database".to_string(), config.database.clone()),
                        (
                            "schema".to_string(),
                            config
                                .schema
                                .clone()
                                .unwrap_or_else(|| "PUBLIC".to_string()),
                        ),
                        ("driver".to_string(), driver),
                    ]),
                    tested_at: Utc::now(),
                });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => Ok(ConnectionTestResult {
                success: true,
                duration_ms,
                error: None,
                metadata: HashMap::from([
                    ("host".to_string(), config.host.clone()),
                    ("port".to_string(), config.port.to_string()),
                    ("database".to_string(), config.database.clone()),
                    (
                        "schema".to_string(),
                        config
                            .schema
                            .clone()
                            .unwrap_or_else(|| "PUBLIC".to_string()),
                    ),
                    ("driver".to_string(), driver),
                ]),
                tested_at: Utc::now(),
            }),
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(e.to_string()),
                metadata: HashMap::from([
                    ("host".to_string(), config.host.clone()),
                    ("port".to_string(), config.port.to_string()),
                    ("database".to_string(), config.database.clone()),
                    (
                        "schema".to_string(),
                        config
                            .schema
                            .clone()
                            .unwrap_or_else(|| "PUBLIC".to_string()),
                    ),
                    ("driver".to_string(), driver),
                ]),
                tested_at: Utc::now(),
            }),
        }
    }

    async fn infer_schema(
        &self,
        _source: &DataSource,
        _credentials: Credentials,
        _table_name: Option<&str>,
        _sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        Err(GraphicaError::Configuration(
            "SAP HANA schema inference is not yet implemented for catalog connectors".to_string(),
        ))
    }

    async fn execute_query(
        &self,
        _source: &DataSource,
        _credentials: Credentials,
        _query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        _limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        Err(GraphicaError::Configuration(
            "SAP HANA query execution is not yet implemented for catalog connectors".to_string(),
        ))
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: false,
            query_timeout: false,
            streaming: false,
            transactions: false,
            max_batch_size: Some(10000),
        }
    }
}
