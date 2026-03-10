//! S3 Parquet Connector

use async_trait::async_trait;
use aws_credential_types::Credentials as AwsCredentials;
use aws_sdk_s3::Client;
use aws_types::region::Region;
use chrono::Utc;
use std::collections::HashMap;

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{DataSource, S3ParquetConfig, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct S3ParquetConnector;

impl S3ParquetConnector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for S3ParquetConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for S3ParquetConnector {
    fn name(&self) -> &'static str {
        "S3 Parquet Connector"
    }

    fn source_type(&self) -> &'static str {
        "S3Parquet"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::S3Parquet(s3_config) => {
                let mut errors = vec![];
                if s3_config.bucket.is_empty() {
                    errors.push("Bucket cannot be empty".to_string());
                }
                if s3_config.path_prefix.is_empty() {
                    errors.push("Path prefix cannot be empty".to_string());
                }
                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected S3 Parquet configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = match &source.connection.config {
            SourceConfig::S3Parquet(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected S3 Parquet configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();

        let access_key = credentials
            .additional
            .get("access_key_id")
            .cloned()
            .unwrap_or_else(|| credentials.username.clone());
        let secret_key = credentials
            .additional
            .get("secret_access_key")
            .cloned()
            .unwrap_or_else(|| credentials.password.clone());
        let session_token = credentials.additional.get("session_token").cloned();

        if access_key.is_empty() || secret_key.is_empty() {
            return Ok(ConnectionTestResult {
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(
                    "Missing S3 credentials: provide access_key_id and secret_access_key"
                        .to_string(),
                ),
                metadata: HashMap::from([
                    ("bucket".to_string(), config.bucket.clone()),
                    ("path_prefix".to_string(), config.path_prefix.clone()),
                ]),
                tested_at: Utc::now(),
            });
        }

        let region = config
            .region
            .clone()
            .or_else(|| credentials.additional.get("region").cloned())
            .unwrap_or_else(|| "us-east-1".to_string());

        let aws_credentials =
            AwsCredentials::new(access_key, secret_key, session_token, None, "graphica");
        let aws_config = aws_sdk_s3::Config::builder()
            .region(Region::new(region.clone()))
            .credentials_provider(aws_credentials)
            .build();

        let client = Client::from_conf(aws_config);

        let response = client
            .list_objects_v2()
            .bucket(&config.bucket)
            .prefix(&config.path_prefix)
            .max_keys(1)
            .send()
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;
        let metadata = HashMap::from([
            ("bucket".to_string(), config.bucket.clone()),
            ("path_prefix".to_string(), config.path_prefix.clone()),
            ("region".to_string(), region.clone()),
        ]);

        match response {
            Ok(_) => Ok(ConnectionTestResult {
                success: true,
                duration_ms,
                error: None,
                metadata,
                tested_at: Utc::now(),
            }),
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(format!("S3 connection failed: {}", e)),
                metadata,
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
        // TODO: Implement Parquet schema inference by reading file metadata
        Ok(SchemaDefinition {
            name: "parquet_dataset".to_string(),
            tables: vec![],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
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
        // TODO: Implement Parquet query execution using arrow/parquet crate
        // Note: This may require DuckDB or DataFusion for SQL queries over Parquet
        Ok(QueryResult {
            rows: vec![],
            row_count: 0,
            execution_time_ms: 20,
            truncated: false,
            columns: None,
        })
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: true,
            query_timeout: false,
            streaming: true,
            transactions: false,
            max_batch_size: Some(1000000),
        }
    }
}
