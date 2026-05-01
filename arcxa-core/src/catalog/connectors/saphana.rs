//! SAP HANA Connector

use async_trait::async_trait;
use chrono::Utc;
use odbc_api::{ConnectionOptions, Cursor, Environment, ResultSetMetadata};
use serde_json::Value;
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
        source: &DataSource,
        credentials: Credentials,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        #[cfg(feature = "odbc")]
        {
            if !parameters.is_empty() {
                return Err(GraphicaError::Configuration(
                    "SAP HANA connector does not yet support parameterized queries".to_string(),
                ));
            }

            let config = match &source.connection.config {
                SourceConfig::SAPHANA(c) => c,
                _ => {
                    return Err(GraphicaError::Configuration(
                        "Expected SAP HANA configuration".to_string(),
                    ))
                }
            };

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

            let effective_query = match limit {
                Some(limit) => format!(
                    "SELECT * FROM ({}) AS GRAPHICA_HANA_QUERY LIMIT {}",
                    query, limit
                ),
                None => query.to_string(),
            };

            let started = std::time::Instant::now();
            let task = tokio::task::spawn_blocking(move || -> Result<QueryResult, GraphicaError> {
                let env = Environment::new().map_err(|error| {
                    GraphicaError::Internal(format!("ODBC environment error: {:?}", error))
                })?;

                let conn = env
                    .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                    .map_err(|error| {
                        GraphicaError::Internal(format!("HANA connection failed: {:?}", error))
                    })?;

                let mut cursor = conn
                    .execute(&effective_query, (), None)
                    .map_err(|error| {
                        GraphicaError::Internal(format!("HANA query failed: {:?}", error))
                    })?
                    .ok_or_else(|| {
                        GraphicaError::Internal(
                            "SAP HANA query returned no result set".to_string(),
                        )
                    })?;

                let num_cols = cursor.num_result_cols().map_err(|error| {
                    GraphicaError::Internal(format!("Failed to inspect HANA result columns: {:?}", error))
                })? as usize;

                let mut column_names = Vec::with_capacity(num_cols);
                let mut description = odbc_api::ColumnDescription::default();
                for i in 1..=num_cols {
                    cursor.describe_col(i as u16, &mut description).map_err(|error| {
                        GraphicaError::Internal(format!("Failed to describe HANA column {}: {:?}", i, error))
                    })?;
                    column_names.push(
                        description
                            .name_to_string()
                            .unwrap_or_else(|_| format!("col{}", i)),
                    );
                }

                let mut rows = Vec::new();
                while let Some(mut row) = cursor.next_row().map_err(|error| {
                    GraphicaError::Internal(format!("Failed to fetch HANA row: {:?}", error))
                })? {
                    let mut object = serde_json::Map::with_capacity(num_cols);
                    for (index, name) in column_names.iter().enumerate() {
                        let mut buffer = Vec::new();
                        let not_null = row.get_text((index + 1) as u16, &mut buffer).map_err(|error| {
                            GraphicaError::Internal(format!("Failed to read HANA column {}: {:?}", index + 1, error))
                        })?;
                        let value = if not_null {
                            Value::String(String::from_utf8_lossy(&buffer).to_string())
                        } else {
                            Value::Null
                        };
                        object.insert(name.clone(), value);
                    }
                    rows.push(Value::Object(object));
                }

                let row_count = rows.len();
                Ok(QueryResult {
                    rows,
                    row_count,
                    execution_time_ms: started.elapsed().as_millis() as u64,
                    truncated: false,
                    columns: None,
                })
            });

            match timeout(Duration::from_secs(timeout_secs.max(1)), task).await {
                Ok(result) => match result {
                    Ok(inner) => inner,
                    Err(error) => Err(GraphicaError::Internal(format!(
                        "HANA query task failed: {}",
                        error
                    ))),
                },
                Err(_) => Err(GraphicaError::Internal(
                    "SAP HANA query timed out".to_string(),
                )),
            }
        }

        #[cfg(not(feature = "odbc"))]
        {
            let _ = (source, credentials, query, parameters, limit, timeout_secs);
            Err(GraphicaError::Configuration(
                "SAP HANA query execution requires the 'odbc' feature".to_string(),
            ))
        }
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: false,
            query_timeout: true,
            streaming: false,
            transactions: false,
            max_batch_size: Some(10000),
        }
    }
}
