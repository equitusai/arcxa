//! Oracle Database Connector
//!
//! Connects to Oracle databases via TNS/OCI protocol.

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::time::Instant;
use tokio::task;

use odbc_api::{ColumnDescription, ConnectionOptions, Cursor, Environment, ResultSetMetadata};

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    connector_v2::{
        ConnectorV2Result, DataSourceConnectorV2, DataStream, ExportConfig, ExportFormat, RowBatch,
    },
    oracle_runtime::resolve_oracle_odbc_resolution,
    types::{DataSource, OracleConfig, SourceConfig},
};
use crate::errors::GraphicaError;
use crate::schema::{DataProfiler, ProfileConfig, SampleRow, UnifiedSchema};

/// Oracle connector
pub struct OracleConnector;

impl OracleConnector {
    pub fn new() -> Self {
        Self
    }

    fn extract_config(source: &DataSource) -> ConnectorResult<&OracleConfig> {
        match &source.connection.config {
            SourceConfig::Oracle(config) => Ok(config),
            _ => Err(GraphicaError::Configuration(
                "Expected Oracle configuration".to_string(),
            )),
        }
    }

    /// Build Oracle connection string (Easy Connect format)
    fn build_connection_string(config: &OracleConfig) -> ConnectorResult<String> {
        let normalized = config.normalized();
        let target = normalized.resolved_target().ok_or_else(|| {
            GraphicaError::Configuration("Missing non-empty serviceName or sid".to_string())
        })?;
        Ok(target.dbq(&normalized.host, normalized.port))
    }

    fn build_odbc_connection_string(
        source: &DataSource,
        credentials: &Credentials,
    ) -> ConnectorResult<String> {
        let config = Self::extract_config(source)?;
        let resolution = resolve_oracle_odbc_resolution(config, &source.metadata)?;
        Ok(resolution.build_connection_string(&credentials.username, &credentials.password))
    }

    fn validate_identifier(value: &str, field_name: &str) -> ConnectorResult<()> {
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#')
        {
            return Err(GraphicaError::Configuration(format!(
                "Invalid {} '{}': only [A-Za-z0-9_$#] allowed",
                field_name, value
            )));
        }
        Ok(())
    }

    async fn execute_odbc_query(
        connection_string: &str,
        query: &str,
        normalize_headers: bool,
    ) -> ConnectorResult<Vec<HashMap<String, String>>> {
        let connection_string = connection_string.to_string();
        let query = query.to_string();

        task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;
            let conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("ODBC connection error: {:?}", e)))?;

            let mut cursor = conn
                .execute(&query, (), None)
                .map_err(|e| GraphicaError::Internal(format!("ODBC query failed: {:?}", e)))?
                .ok_or_else(|| {
                    GraphicaError::Internal("ODBC query returned no result set".to_string())
                })?;

            let num_cols = cursor
                .num_result_cols()
                .map_err(|e| GraphicaError::Internal(format!("ODBC metadata error: {:?}", e)))?
                as usize;

            let mut column_names = Vec::with_capacity(num_cols);
            let mut description = ColumnDescription::default();
            for i in 1..=num_cols {
                cursor
                    .describe_col(i as u16, &mut description)
                    .map_err(|e| {
                        GraphicaError::Internal(format!("ODBC describe column error: {:?}", e))
                    })?;
                let mut name = description
                    .name_to_string()
                    .unwrap_or_else(|_| format!("col{}", i));
                if normalize_headers {
                    name = name.to_lowercase();
                }
                column_names.push(name);
            }

            let mut rows = Vec::new();
            while let Some(mut row) = cursor
                .next_row()
                .map_err(|e| GraphicaError::Internal(format!("ODBC row fetch error: {:?}", e)))?
            {
                let mut row_map = HashMap::with_capacity(num_cols);
                for (idx, column_name) in column_names.iter().enumerate() {
                    let mut buffer = Vec::new();
                    let not_null = row.get_text((idx + 1) as u16, &mut buffer).map_err(|e| {
                        GraphicaError::Internal(format!("ODBC column read error: {:?}", e))
                    })?;
                    let value = if not_null {
                        String::from_utf8_lossy(&buffer).to_string()
                    } else {
                        String::new()
                    };
                    row_map.insert(column_name.clone(), value);
                }
                rows.push(row_map);
            }

            Ok(rows)
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("ODBC task join error: {:?}", e)))?
    }

    fn build_metadata_query(schema: &str, table_name: Option<&str>) -> String {
        let mut query = format!(
            r#"
SELECT
    t.OWNER as TABLE_SCHEMA,
    t.TABLE_NAME as TABLE_NAME,
    c.COLUMN_NAME as COLUMN_NAME,
    c.DATA_TYPE as DATA_TYPE,
    c.NULLABLE as NULLABLE,
    c.DATA_DEFAULT as DATA_DEFAULT
FROM ALL_TABLES t
INNER JOIN ALL_TAB_COLUMNS c
    ON t.OWNER = c.OWNER
    AND t.TABLE_NAME = c.TABLE_NAME
WHERE t.OWNER = '{}'
"#,
            schema.to_uppercase()
        );

        if let Some(table) = table_name {
            query.push_str(&format!(
                "  AND t.TABLE_NAME = '{}'\n",
                table.to_uppercase()
            ));
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

    fn parse_nullable(value: &str) -> bool {
        matches!(value, "Y" | "YES" | "TRUE" | "1")
    }

    fn apply_limit_if_needed(query: &str, limit: Option<usize>) -> String {
        let Some(limit) = limit else {
            return query.to_string();
        };

        let trimmed = query.trim().trim_end_matches(';');
        let upper = trimmed.to_uppercase();
        if upper.contains("FETCH FIRST") || upper.contains("LIMIT ") {
            return trimmed.to_string();
        }
        format!("{} FETCH FIRST {} ROWS ONLY", trimmed, limit)
    }
}

impl Default for OracleConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for OracleConnector {
    fn name(&self) -> &'static str {
        "Oracle Database Connector"
    }

    fn source_type(&self) -> &'static str {
        "Oracle"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::Oracle(oracle_config) => {
                let mut errors = vec![];
                let mut warnings = vec![];

                if oracle_config.host.is_empty() {
                    errors.push("Host cannot be empty".to_string());
                }

                if oracle_config.port == 0 {
                    errors.push("Port must be greater than 0".to_string());
                }

                // Must have either service_name or SID
                let normalized = oracle_config.normalized();

                if normalized.resolved_target().is_none() {
                    errors.push("Either serviceName or sid must be specified".to_string());
                }

                // Warn if both provided
                if normalized.service_name.is_some() && normalized.sid.is_some() {
                    warnings.push(
                        "Both serviceName and sid specified, serviceName will be used".to_string(),
                    );
                }

                // Warn if no schema specified
                if normalized.schema.is_none() {
                    warnings
                        .push("No schema specified, will use user's default schema".to_string());
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid().with_warnings(warnings))
                } else {
                    Ok(ValidationResult::invalid(errors).with_warnings(warnings))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected Oracle configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = Self::extract_config(source)?;
        let start = Instant::now();

        let easy_connect = Self::build_connection_string(config)?;
        let odbc_resolution = resolve_oracle_odbc_resolution(config, &source.metadata)?;
        let odbc_connection_string = Self::build_odbc_connection_string(source, &credentials)?;

        tracing::info!(
            "Oracle connection test: host={} port={} target={}",
            config.host,
            config.port,
            easy_connect
        );

        let test_result = Self::execute_odbc_query(
            &odbc_connection_string,
            "SELECT 1 AS HEALTH_CHECK FROM DUAL",
            true,
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;
        let (success, error) = match test_result {
            Ok(rows) if !rows.is_empty() => (true, None),
            Ok(_) => (
                false,
                Some("Oracle health-check returned no rows".to_string()),
            ),
            Err(e) => (false, Some(e.to_string())),
        };

        Ok(ConnectionTestResult {
            success,
            duration_ms,
            error,
            metadata: HashMap::from([
                ("host".to_string(), config.host.clone()),
                ("port".to_string(), config.port.to_string()),
                (
                    "connection_type".to_string(),
                    odbc_resolution.target.connection_type().to_string(),
                ),
                ("connection_string".to_string(), easy_connect.clone()),
                ("dbq".to_string(), odbc_resolution.dbq),
                ("odbc_driver".to_string(), odbc_resolution.driver),
                (
                    "odbc_dsn".to_string(),
                    odbc_resolution.dsn.unwrap_or_default(),
                ),
                (
                    "odbc_driver_registered".to_string(),
                    odbc_resolution
                        .driver_registered
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
                (
                    "odbc_dsn_registered".to_string(),
                    odbc_resolution
                        .dsn_registered
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
                ("odbc_enabled".to_string(), "true".to_string()),
            ]),
            tested_at: Utc::now(),
        })
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = Self::extract_config(source)?;
        if let Some(table) = table_name {
            Self::validate_identifier(table, "table_name")?;
        }

        let normalized = config.normalized();
        let schema_name = normalized
            .schema
            .as_deref()
            .unwrap_or(&credentials.username)
            .to_uppercase();
        let metadata_query = Self::build_metadata_query(&schema_name, table_name);
        let connection_string = Self::build_odbc_connection_string(source, &credentials)?;

        tracing::info!(
            "Oracle schema inference: {} (table: {:?}, sample_size={})",
            schema_name,
            table_name,
            sample_size
        );

        let rows = Self::execute_odbc_query(&connection_string, &metadata_query, true).await?;

        let mut tables_by_name: HashMap<String, TableDefinition> = HashMap::new();
        for row in rows {
            let table = row.get("table_name").cloned().ok_or_else(|| {
                GraphicaError::Internal("Oracle metadata missing table_name column".to_string())
            })?;
            let table_entry =
                tables_by_name
                    .entry(table.clone())
                    .or_insert_with(|| TableDefinition {
                        name: table.clone(),
                        columns: Vec::new(),
                        estimated_rows: None,
                    });

            let column_name = row.get("column_name").cloned().ok_or_else(|| {
                GraphicaError::Internal("Oracle metadata missing column_name column".to_string())
            })?;
            let data_type = row
                .get("data_type")
                .cloned()
                .unwrap_or_else(|| "VARCHAR2".to_string());
            let nullable = row
                .get("nullable")
                .map(|v| Self::parse_nullable(v))
                .unwrap_or(true);
            let default_value = row.get("data_default").cloned().filter(|v| !v.is_empty());

            table_entry.columns.push(ColumnDefinition {
                name: column_name,
                data_type,
                nullable,
                primary_key: false,
                default_value,
                semantic_type: None,
                statistics: None,
            });
        }

        Ok(SchemaDefinition {
            name: schema_name,
            tables: tables_by_name.into_values().collect(),
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
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        if !parameters.is_empty() {
            return Err(GraphicaError::Configuration(
                "Oracle connector execute_query currently requires parameters to be pre-bound"
                    .to_string(),
            ));
        }

        let start = Instant::now();
        let connection_string = Self::build_odbc_connection_string(source, &credentials)?;
        let final_query = Self::apply_limit_if_needed(query, limit);

        tracing::info!("Oracle query execution: {} (limit: {:?})", query, limit);

        let rows = Self::execute_odbc_query(&connection_string, &final_query, false).await?;
        let execution_time_ms = start.elapsed().as_millis() as u64;

        let row_count = rows.len();
        let truncated = limit.map(|l| row_count >= l).unwrap_or(false);

        let columns = rows.first().map(|row| {
            row.keys()
                .map(|name| ColumnDefinition {
                    name: name.clone(),
                    data_type: "VARCHAR2".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                })
                .collect::<Vec<_>>()
        });

        let json_rows = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, serde_json::Value::String(v));
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(QueryResult {
            rows: json_rows,
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
            query_timeout: false,
            streaming: false,
            transactions: true,
            max_batch_size: Some(10000),
        }
    }
}

// ============================================================================
// V2 Interface Implementation
// ============================================================================

#[async_trait]
impl DataSourceConnectorV2 for OracleConnector {
    fn get_profiler(&self) -> Box<dyn DataProfiler> {
        // TODO: Implement OracleProfiler similar to PostgresProfiler
        // For now, return a PostgresProfiler as a placeholder
        // since Oracle and PostgreSQL have similar metadata queries
        use crate::schema::PostgresProfiler;
        Box::new(PostgresProfiler::new("oracle://placeholder".to_string()))
    }

    async fn get_unified_schema(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        table_name: &str,
        _config: ProfileConfig,
    ) -> ConnectorV2Result<UnifiedSchema> {
        use crate::schema::{SourceType, UnifiedField, UniversalDataType};

        let oracle_config = Self::extract_config(source)?;

        // Build connection identifier
        let connection_id = format!(
            "oracle://{}:{}/{}",
            oracle_config.host,
            oracle_config.port,
            oracle_config
                .service_name
                .as_deref()
                .or(oracle_config.sid.as_deref())
                .unwrap_or("UNKNOWN")
        );

        let mut schema =
            UnifiedSchema::new(table_name.to_string(), SourceType::Oracle, connection_id);

        // TODO: In production, query ALL_TAB_COLUMNS to get actual schema
        // For now, return sample schema based on table name
        tracing::info!(
            "Oracle get_unified_schema: {}.{}",
            oracle_config.schema.as_deref().unwrap_or("USER_SCHEMA"),
            table_name
        );

        // Add sample fields (in production, query metadata)
        schema.add_field(UnifiedField::new(
            "ID".to_string(),
            UniversalDataType::Integer { bits: Some(32) },
        ));

        schema.add_field(UnifiedField::new(
            "NAME".to_string(),
            UniversalDataType::String {
                max_length: Some(100),
            },
        ));

        schema.add_field(UnifiedField::new(
            "CREATED_DATE".to_string(),
            UniversalDataType::Date,
        ));

        schema.row_count = Some(5000);

        Ok(schema)
    }

    async fn stream_data(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_or_query: &str,
        batch_size: usize,
    ) -> ConnectorV2Result<DataStream> {
        let oracle_config = Self::extract_config(source)?;

        tracing::info!(
            "Oracle stream_data: {} (batch_size: {})",
            table_or_query,
            batch_size
        );

        // TODO: In production, execute actual Oracle query with cursor
        // Use oracle crate to create connection and stream rows
        // Example:
        //   let conn = Connection::connect(&credentials.username, &credentials.password, connection_string)?;
        //   let mut stmt = conn.statement("SELECT * FROM table").build()?;
        //   let rows = stmt.query(&[])?;
        //
        // For now, return mock stream with sample data

        let sample_batch: RowBatch = vec![
            SampleRow::from([
                ("ID".to_string(), serde_json::json!(1)),
                ("NAME".to_string(), serde_json::json!("Oracle Sample 1")),
                ("CREATED_DATE".to_string(), serde_json::json!("2024-01-01")),
            ]),
            SampleRow::from([
                ("ID".to_string(), serde_json::json!(2)),
                ("NAME".to_string(), serde_json::json!("Oracle Sample 2")),
                ("CREATED_DATE".to_string(), serde_json::json!("2024-01-02")),
            ]),
        ];

        let _ = (credentials, oracle_config);

        // Return stream with single batch
        let stream = stream::iter(vec![Ok(sample_batch)]);
        Ok(Box::pin(stream))
    }

    async fn export_to_format(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        format: ExportFormat,
        config: ExportConfig,
    ) -> ConnectorV2Result<Vec<u8>> {
        tracing::info!("Oracle export_to_format: {} as {:?}", table_name, format);

        // Stream data and convert to requested format
        let mut stream = self
            .stream_data(
                source,
                credentials,
                table_name,
                config.max_rows.unwrap_or(10000),
            )
            .await?;

        let mut all_rows = Vec::new();
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;
            all_rows.extend(batch);

            if let Some(max) = config.max_rows {
                if all_rows.len() >= max {
                    all_rows.truncate(max);
                    break;
                }
            }
        }

        match format {
            ExportFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());

                // Write headers
                if config.include_headers && !all_rows.is_empty() {
                    let headers: Vec<String> = all_rows[0].keys().cloned().collect();
                    writer
                        .write_record(&headers)
                        .map_err(|e| GraphicaError::Internal(format!("CSV write error: {}", e)))?;
                }

                // Write rows
                for row in &all_rows {
                    let values: Vec<String> = row
                        .values()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        })
                        .collect();
                    writer
                        .write_record(&values)
                        .map_err(|e| GraphicaError::Internal(format!("CSV write error: {}", e)))?;
                }

                writer
                    .into_inner()
                    .map_err(|e| GraphicaError::Internal(format!("CSV finalization error: {}", e)))
            }

            ExportFormat::JsonLines => {
                let mut output = Vec::new();
                for row in &all_rows {
                    serde_json::to_writer(&mut output, row)
                        .map_err(|e| GraphicaError::Serialization(e.to_string()))?;
                    output.push(b'\n');
                }
                Ok(output)
            }

            ExportFormat::JsonArray => serde_json::to_vec(&all_rows)
                .map_err(|e| GraphicaError::Serialization(e.to_string())),

            ExportFormat::Parquet | ExportFormat::Arrow => Err(GraphicaError::Internal(format!(
                "{:?} export not yet implemented for Oracle",
                format
            ))),
        }
    }

    async fn estimate_row_count(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        table_name: &str,
    ) -> ConnectorV2Result<Option<u64>> {
        let oracle_config = Self::extract_config(source)?;

        // TODO: In production, query ALL_TABLES.NUM_ROWS or use DBMS_STATS
        // Example query:
        //   SELECT NUM_ROWS FROM ALL_TABLES
        //   WHERE OWNER = ? AND TABLE_NAME = ?
        //
        // Or use:
        //   SELECT COUNT(*) FROM (SELECT 1 FROM table WHERE ROWNUM <= 100000)
        //   to get fast estimate for large tables

        tracing::info!(
            "Oracle estimate_row_count: {}.{}",
            oracle_config.schema.as_deref().unwrap_or("USER_SCHEMA"),
            table_name
        );

        // Return mock estimate
        Ok(Some(5000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::types::ConnectionDetails;

    fn create_test_source() -> DataSource {
        DataSource::new(
            "Test Oracle".to_string(),
            "Oracle".to_string(),
            ConnectionDetails {
                secret_ref: "test://secret".to_string(),
                config: SourceConfig::Oracle(OracleConfig {
                    host: "oracle-db.example.com".to_string(),
                    port: 1521,
                    service_name: Some("ORCL".to_string()),
                    sid: None,
                    schema: Some("HR".to_string()),
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        )
    }

    #[test]
    fn test_validate_config() {
        let connector = OracleConnector::new();
        let source = create_test_source();

        let result = connector
            .validate_config(&source.connection.config)
            .unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_validate_invalid_config() {
        let connector = OracleConnector::new();
        let invalid_config = SourceConfig::Oracle(OracleConfig {
            host: "".to_string(),
            port: 1521,
            service_name: None,
            sid: None,
            schema: None,
        });

        let result = connector.validate_config(&invalid_config).unwrap();
        assert!(!result.valid);
        assert!(result.errors.len() >= 2); // host and service_name/sid
    }

    #[test]
    fn test_validate_both_service_and_sid() {
        let connector = OracleConnector::new();
        let config = SourceConfig::Oracle(OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: Some("ORCL".to_string()),
            sid: Some("ORCL".to_string()),
            schema: None,
        });

        let result = connector.validate_config(&config).unwrap();
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 2); // both provided + no schema
    }

    #[tokio::test]
    async fn test_connection() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let result = connector.test_connection(&source, creds).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(
            result.metadata.get("connection_string"),
            Some(&"oracle-db.example.com:1521/ORCL".to_string())
        );
    }

    #[tokio::test]
    async fn test_infer_schema() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let schema = connector
            .infer_schema(&source, creds, Some("EMPLOYEES"), 1000)
            .await;
        assert!(schema.is_err());
    }

    #[tokio::test]
    async fn test_execute_query() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let result = connector
            .execute_query(
                &source,
                creds,
                "SELECT * FROM EMPLOYEES",
                HashMap::new(),
                Some(10),
                30,
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_capabilities() {
        let connector = OracleConnector::new();
        let caps = connector.capabilities();

        assert!(!caps.parameterized_queries);
        assert!(caps.schema_inference);
        assert!(caps.transactions);
        assert_eq!(caps.max_batch_size, Some(10000));
    }

    #[test]
    fn test_build_connection_string() {
        let config_service = OracleConfig {
            host: "db.example.com".to_string(),
            port: 1521,
            service_name: Some("PROD".to_string()),
            sid: None,
            schema: None,
        };
        assert_eq!(
            OracleConnector::build_connection_string(&config_service).unwrap(),
            "db.example.com:1521/PROD"
        );

        let config_sid = OracleConfig {
            host: "db.example.com".to_string(),
            port: 1521,
            service_name: None,
            sid: Some("ORCL".to_string()),
            schema: None,
        };
        assert_eq!(
            OracleConnector::build_connection_string(&config_sid).unwrap(),
            "db.example.com:1521/ORCL"
        );
    }

    #[test]
    fn test_build_connection_string_falls_back_to_sid_when_service_name_blank() {
        let config = OracleConfig {
            host: "db.example.com".to_string(),
            port: 1521,
            service_name: Some("   ".to_string()),
            sid: Some("XE".to_string()),
            schema: None,
        };

        assert_eq!(
            OracleConnector::build_connection_string(&config).unwrap(),
            "db.example.com:1521/XE"
        );
    }

    // ============================================================================
    // V2 Interface Tests
    // ============================================================================

    #[test]
    fn test_get_profiler() {
        let connector = OracleConnector::new();
        let _profiler = connector.get_profiler();
        // Profiler creation should succeed
    }

    #[tokio::test]
    async fn test_get_unified_schema() {
        use crate::schema::SourceType;

        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let schema = connector
            .get_unified_schema(&source, creds, "EMPLOYEES", ProfileConfig::default())
            .await
            .unwrap();

        assert_eq!(schema.name, "EMPLOYEES");
        assert_eq!(schema.source_type, SourceType::Oracle);
        assert!(schema.source_ref.contains("oracle://"));
        assert!(schema.source_ref.contains("oracle-db.example.com"));
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.row_count, Some(5000));

        // Verify field names
        assert_eq!(schema.fields[0].name, "ID");
        assert_eq!(schema.fields[1].name, "NAME");
        assert_eq!(schema.fields[2].name, "CREATED_DATE");
    }

    #[tokio::test]
    async fn test_stream_data() {
        use futures::StreamExt;

        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let mut stream = connector
            .stream_data(&source, creds, "SELECT * FROM EMPLOYEES", 1000)
            .await
            .unwrap();

        let mut total_rows = 0;
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.unwrap();
            total_rows += batch.len();
        }

        assert!(total_rows > 0);
    }

    #[tokio::test]
    async fn test_export_to_csv() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let csv_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEES",
                ExportFormat::Csv,
                ExportConfig::default(),
            )
            .await
            .unwrap();

        let csv_str = String::from_utf8(csv_data).unwrap();
        assert!(csv_str.contains("ID")); // Header
        assert!(csv_str.contains("Oracle Sample")); // Data
    }

    #[tokio::test]
    async fn test_export_to_json_lines() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let json_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEES",
                ExportFormat::JsonLines,
                ExportConfig::default(),
            )
            .await
            .unwrap();

        let json_str = String::from_utf8(json_data).unwrap();
        let lines: Vec<&str> = json_str.trim().split('\n').collect();
        assert!(lines.len() > 0);

        // Each line should be valid JSON
        for line in lines {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }

    #[tokio::test]
    async fn test_export_to_json_array() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let json_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEES",
                ExportFormat::JsonArray,
                ExportConfig::default(),
            )
            .await
            .unwrap();

        let rows: Vec<SampleRow> = serde_json::from_slice(&json_data).unwrap();
        assert!(rows.len() > 0);
        assert!(rows[0].contains_key("ID"));
    }

    #[tokio::test]
    async fn test_estimate_row_count() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let count = connector
            .estimate_row_count(&source, creds, "EMPLOYEES")
            .await
            .unwrap();

        assert!(count.is_some());
        assert_eq!(count.unwrap(), 5000);
    }

    #[tokio::test]
    async fn test_get_sample_rows() {
        let connector = OracleConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("scott".to_string(), "tiger".to_string());

        let rows = connector
            .get_sample_rows(&source, creds, "EMPLOYEES", 10)
            .await
            .unwrap();

        assert!(rows.len() > 0);
        assert!(rows.len() <= 10);
    }
}
