//! IBM DB2 Connector
//!
//! Connects to IBM DB2 databases via ODBC/JDBC protocol.

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;

use odbc_api::{
    ColumnDescription, ConnectionOptions, Cursor, CursorRow, Environment, ResultSetMetadata,
};

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
    types::{DB2Config, DataSource, SourceConfig},
};
use crate::errors::GraphicaError;
use crate::schema::{DataProfiler, ProfileConfig, SampleRow, UnifiedSchema};

/// IBM DB2 connector
pub struct DB2Connector;

impl DB2Connector {
    pub fn new() -> Self {
        Self
    }

    fn extract_config(source: &DataSource) -> ConnectorResult<&DB2Config> {
        match &source.connection.config {
            SourceConfig::DB2(config) => Ok(config),
            _ => Err(GraphicaError::Configuration(
                "Expected DB2 configuration".to_string(),
            )),
        }
    }

    /// Build SYSCAT metadata query for DB2
    fn build_syscat_metadata_query(schema: &str, table_filter: Option<&str>) -> String {
        let upper_schema = schema.to_uppercase();
        let mut query = format!(
            r#"
SELECT
    t.TABSCHEMA as table_schema,
    t.TABNAME as table_name,
    t.TYPE as table_type,
    c.COLNAME as column_name,
    c.TYPENAME as data_type,
    CASE WHEN c.NULLS = 'Y' THEN 'YES' ELSE 'NO' END as is_nullable,
    c.DEFAULT as column_default,
    CASE WHEN k.COLNAME IS NOT NULL THEN 'true' ELSE 'false' END as is_primary_key,
    t.CARD as estimated_row_count
FROM SYSCAT.TABLES t
INNER JOIN SYSCAT.COLUMNS c
    ON t.TABSCHEMA = c.TABSCHEMA
    AND t.TABNAME = c.TABNAME
LEFT JOIN SYSCAT.KEYCOLUSE k
    ON c.TABSCHEMA = k.TABSCHEMA
    AND c.TABNAME = k.TABNAME
    AND c.COLNAME = k.COLNAME
WHERE t.TYPE = 'T'
  AND t.TABSCHEMA = '{}'
"#,
            upper_schema
        );

        if let Some(table) = table_filter {
            let upper_table = table.to_uppercase();
            query.push_str(&format!("  AND t.TABNAME = '{}'\n", upper_table));
        }

        query.push_str("ORDER BY t.TABSCHEMA, t.TABNAME, c.COLNO");
        query
    }

    /// Parse SYSCAT query results into SchemaDefinition
    fn parse_syscat_results(cursor: &mut impl Cursor) -> ConnectorResult<SchemaDefinition> {
        use std::collections::HashMap as StdHashMap;

        let mut tables_map: StdHashMap<String, TableDefinition> = StdHashMap::new();
        let mut schema_name = String::from("DB2INST1");

        while let Some(mut row) = cursor
            .next_row()
            .map_err(|e| GraphicaError::Internal(format!("Failed to fetch row: {:?}", e)))?
        {
            // Extract column values
            let table_schema = Self::get_string_column(&mut row, 1)?;
            let table_name = Self::get_string_column(&mut row, 2)?;
            let column_name = Self::get_string_column(&mut row, 4)?;
            let data_type = Self::get_string_column(&mut row, 5)?;
            let is_nullable = Self::get_string_column(&mut row, 6)?;
            let is_primary_key = Self::get_string_column(&mut row, 8)?;
            let estimated_rows = Self::get_string_column(&mut row, 9)
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| n as u64);

            schema_name = table_schema;

            let column = ColumnDefinition {
                name: column_name,
                data_type,
                nullable: is_nullable == "YES",
                primary_key: is_primary_key == "true",
                default_value: None,
                semantic_type: None,
                statistics: None,
            };

            tables_map
                .entry(table_name.clone())
                .or_insert_with(|| TableDefinition {
                    name: table_name,
                    columns: vec![],
                    estimated_rows,
                })
                .columns
                .push(column);
        }

        Ok(SchemaDefinition {
            name: schema_name,
            tables: tables_map.into_values().collect(),
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
    }

    /// Helper to get string from ODBC column
    fn get_string_column(row: &mut CursorRow<'_>, col_index: u16) -> ConnectorResult<String> {
        let mut buffer = Vec::new();
        let has_value = row.get_text(col_index, &mut buffer).map_err(|e| {
            GraphicaError::Internal(format!("Failed to get column {}: {:?}", col_index, e))
        })?;

        if has_value {
            Ok(String::from_utf8_lossy(&buffer).to_string())
        } else {
            Ok(String::new())
        }
    }

    /// Map DB2 data types to UniversalDataType
    fn map_db2_type_to_universal(db2_type: &str) -> crate::schema::UniversalDataType {
        use crate::schema::UniversalDataType;

        let upper_type = db2_type.to_uppercase();

        if upper_type.starts_with("VARCHAR") || upper_type.starts_with("CHAR") {
            // Extract length from VARCHAR(50)
            let max_length = upper_type
                .strip_prefix("VARCHAR(")
                .or_else(|| upper_type.strip_prefix("CHAR("))
                .and_then(|s| s.strip_suffix(')'))
                .and_then(|s| s.parse::<usize>().ok());
            UniversalDataType::String { max_length }
        } else if upper_type.contains("INT") || upper_type == "SMALLINT" {
            UniversalDataType::Integer { bits: Some(32) }
        } else if upper_type == "BIGINT" {
            UniversalDataType::Integer { bits: Some(64) }
        } else if upper_type.contains("DECIMAL") || upper_type.contains("NUMERIC") {
            UniversalDataType::Decimal {
                precision: 10,
                scale: 2,
            }
        } else if upper_type.contains("FLOAT")
            || upper_type.contains("DOUBLE")
            || upper_type.contains("REAL")
        {
            UniversalDataType::Float { bits: Some(64) }
        } else if upper_type == "DATE" {
            UniversalDataType::Date
        } else if upper_type.contains("TIMESTAMP") {
            UniversalDataType::Timestamp
        } else if upper_type.contains("TIME") {
            UniversalDataType::Time {
                with_timezone: false,
            }
        } else if upper_type == "BOOLEAN" {
            UniversalDataType::Boolean
        } else {
            UniversalDataType::String { max_length: None }
        }
    }
}

impl Default for DB2Connector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for DB2Connector {
    fn name(&self) -> &'static str {
        "IBM DB2 Connector"
    }

    fn source_type(&self) -> &'static str {
        "DB2"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::DB2(db2_config) => {
                let mut errors = vec![];
                let mut warnings = vec![];

                if db2_config.host.is_empty() {
                    errors.push("Host cannot be empty".to_string());
                }

                if db2_config.database.is_empty() {
                    errors.push("Database cannot be empty".to_string());
                }

                if db2_config.port == 0 {
                    errors.push("Port must be greater than 0".to_string());
                }

                // Warn if no schema specified
                if db2_config.schema.is_none() {
                    warnings.push("No schema specified, will use default schema".to_string());
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid().with_warnings(warnings))
                } else {
                    Ok(ValidationResult::invalid(errors).with_warnings(warnings))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected DB2 configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = Self::extract_config(source)?;
        let start = std::time::Instant::now();

        tracing::info!(
            "DB2 connection test: {}:{}/{}",
            config.host,
            config.port,
            config.database
        );

        // Use ODBC to test actual connection
        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            config.database, config.host, config.port, credentials.username, credentials.password
        );

        let result = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let _conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("DB2 connection failed: {:?}", e)))?;

            Ok::<_, GraphicaError>(())
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("Connection task failed: {}", e)))?;

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
                            .unwrap_or_else(|| "DB2INST1".to_string()),
                    ),
                ]),
                tested_at: Utc::now(),
            }),
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(e.to_string()),
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
        let config = Self::extract_config(source)?;
        let schema_name = config.schema.as_deref().unwrap_or("DB2INST1");

        tracing::info!(
            "DB2 schema inference: {}.{} (table: {:?})",
            config.database,
            schema_name,
            table_name
        );

        // Build SYSCAT metadata query
        let query = Self::build_syscat_metadata_query(schema_name, table_name);

        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            config.database, config.host, config.port, credentials.username, credentials.password
        );

        let result = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("DB2 connection failed: {:?}", e)))?;

            let mut cursor = conn
                .execute(&query, (), None)
                .map_err(|e| GraphicaError::Internal(format!("Metadata query failed: {:?}", e)))?
                .ok_or_else(|| {
                    GraphicaError::Internal("No results from metadata query".to_string())
                })?;

            Self::parse_syscat_results(&mut cursor)
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("Schema inference task failed: {}", e)))??;

        Ok(result)
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        credentials: Credentials,
        query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let config = Self::extract_config(source)?;
        let start = std::time::Instant::now();

        tracing::info!("DB2 query execution: {} (limit: {:?})", query, limit);

        // Apply limit to query if specified
        let final_query = if let Some(limit_val) = limit {
            if query.to_uppercase().contains("LIMIT")
                || query.to_uppercase().contains("FETCH FIRST")
            {
                query.to_string()
            } else {
                format!("{} FETCH FIRST {} ROWS ONLY", query, limit_val)
            }
        } else {
            query.to_string()
        };

        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            config.database, config.host, config.port, credentials.username, credentials.password
        );

        let result = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("DB2 connection failed: {:?}", e)))?;

            let mut cursor = conn
                .execute(&final_query, (), None)
                .map_err(|e| GraphicaError::Internal(format!("Query execution failed: {:?}", e)))?
                .ok_or_else(|| {
                    GraphicaError::Internal(
                        "Query did not return a result set (possibly a DML statement)".to_string(),
                    )
                })?;

            let num_cols = cursor.num_result_cols().map_err(|e| {
                GraphicaError::Internal(format!("Failed to get column count: {:?}", e))
            })? as usize;

            // Get column metadata
            let mut columns = Vec::with_capacity(num_cols);
            let mut column_names = Vec::with_capacity(num_cols);
            let mut description = ColumnDescription::default();

            for i in 1..=num_cols {
                cursor
                    .describe_col(i as u16, &mut description)
                    .map_err(|e| {
                        GraphicaError::Internal(format!("Failed to describe column {}: {:?}", i, e))
                    })?;

                let name = description
                    .name_to_string()
                    .unwrap_or_else(|_| format!("col{}", i));

                let data_type = format!("{:?}", description.data_type);

                columns.push(ColumnDefinition {
                    name: name.clone(),
                    data_type,
                    nullable: true, // Cannot determine nullability from query result metadata
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                });

                column_names.push(name);
            }

            // Fetch all rows
            let mut rows = Vec::new();
            let mut row_count = 0;

            while let Some(mut row) = cursor
                .next_row()
                .map_err(|e| GraphicaError::Internal(format!("Failed to fetch row: {:?}", e)))?
            {
                let mut row_map = serde_json::Map::new();

                for (idx, col_name) in column_names.iter().enumerate() {
                    let mut buffer = Vec::new();
                    let has_value = row.get_text((idx + 1) as u16, &mut buffer).map_err(|e| {
                        GraphicaError::Internal(format!(
                            "Failed to get column {}: {:?}",
                            idx + 1,
                            e
                        ))
                    })?;

                    let value = if has_value {
                        serde_json::json!(String::from_utf8_lossy(&buffer).to_string())
                    } else {
                        serde_json::Value::Null
                    };

                    row_map.insert(col_name.clone(), value);
                }

                rows.push(serde_json::Value::Object(row_map));
                row_count += 1;
            }

            let truncated = if let Some(limit_val) = limit {
                row_count >= limit_val
            } else {
                false
            };

            Ok::<(Vec<serde_json::Value>, usize, bool, Vec<ColumnDefinition>), GraphicaError>((
                rows, row_count, truncated, columns,
            ))
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("Query execution task failed: {}", e)))??;

        let execution_time_ms = start.elapsed().as_millis() as u64;
        let (rows, row_count, truncated, columns) = result;

        Ok(QueryResult {
            rows,
            row_count,
            execution_time_ms,
            truncated,
            columns: Some(columns),
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
impl DataSourceConnectorV2 for DB2Connector {
    fn get_profiler(&self) -> Box<dyn DataProfiler> {
        // Return DB2-specific profiler
        use crate::schema::DB2Profiler;
        Box::new(DB2Profiler::new("db2://profiler".to_string()))
    }

    async fn get_unified_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        _config: ProfileConfig,
    ) -> ConnectorV2Result<UnifiedSchema> {
        use crate::schema::{SourceType, UnifiedField};

        let db2_config = Self::extract_config(source)?;
        let schema_name = db2_config.schema.as_deref().unwrap_or("DB2INST1");

        // Build connection identifier
        let connection_id = format!(
            "db2://{}:{}/{}",
            db2_config.host, db2_config.port, db2_config.database
        );

        tracing::info!("DB2 get_unified_schema: {}.{}", schema_name, table_name);

        // Use infer_schema to get actual column metadata
        let schema_def = self
            .infer_schema(source, credentials, Some(table_name), 0)
            .await?;

        let mut schema = UnifiedSchema::new(table_name.to_string(), SourceType::DB2, connection_id);

        // Find the table in the schema definition
        if let Some(table_def) = schema_def
            .tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(table_name))
        {
            // Convert columns to UnifiedField
            for col in &table_def.columns {
                let data_type = Self::map_db2_type_to_universal(&col.data_type);
                schema.add_field(UnifiedField::new(col.name.clone(), data_type));
            }

            schema.row_count = table_def.estimated_rows.map(|n| n as u64);
        }

        Ok(schema)
    }

    async fn stream_data(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_or_query: &str,
        batch_size: usize,
    ) -> ConnectorV2Result<DataStream> {
        let db2_config = Self::extract_config(source)?;
        let schema_name = db2_config.schema.as_deref().unwrap_or("DB2INST1");

        tracing::info!(
            "DB2 stream_data: {} (batch_size: {})",
            table_or_query,
            batch_size
        );

        // Determine if table_or_query is a table name or SQL query
        let query = if table_or_query.to_uppercase().contains("SELECT") {
            table_or_query.to_string()
        } else {
            format!(
                "SELECT * FROM {}.{}",
                schema_name.to_uppercase(),
                table_or_query.to_uppercase()
            )
        };

        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            db2_config.database, db2_config.host, db2_config.port, credentials.username, credentials.password
        );

        let rows = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("DB2 connection failed: {:?}", e)))?;

            let mut cursor = conn
                .execute(&query, (), None)
                .map_err(|e| GraphicaError::Internal(format!("Query execution failed: {:?}", e)))?
                .ok_or_else(|| GraphicaError::Internal("No results from query".to_string()))?;

            let num_cols = cursor.num_result_cols().map_err(|e| {
                GraphicaError::Internal(format!("Failed to get column count: {:?}", e))
            })? as usize;

            // Get column names
            let mut column_names = Vec::with_capacity(num_cols);
            let mut description = ColumnDescription::default();
            for i in 1..=num_cols {
                cursor
                    .describe_col(i as u16, &mut description)
                    .map_err(|e| {
                        GraphicaError::Internal(format!("Failed to describe column {}: {:?}", i, e))
                    })?;
                let name = description
                    .name_to_string()
                    .unwrap_or_else(|_| format!("col{}", i));
                column_names.push(name);
            }

            // Fetch all rows
            let mut all_rows = Vec::new();
            while let Some(mut row) = cursor
                .next_row()
                .map_err(|e| GraphicaError::Internal(format!("Failed to fetch row: {:?}", e)))?
            {
                let mut row_map = SampleRow::new();
                for (idx, col_name) in column_names.iter().enumerate() {
                    let mut buffer = Vec::new();
                    let has_value = row.get_text((idx + 1) as u16, &mut buffer).map_err(|e| {
                        GraphicaError::Internal(format!(
                            "Failed to get column {}: {:?}",
                            idx + 1,
                            e
                        ))
                    })?;

                    let value = if has_value {
                        serde_json::json!(String::from_utf8_lossy(&buffer).to_string())
                    } else {
                        serde_json::Value::Null
                    };
                    row_map.insert(col_name.clone(), value);
                }
                all_rows.push(row_map);
            }

            Ok::<Vec<SampleRow>, GraphicaError>(all_rows)
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("Stream data task failed: {}", e)))??;

        // Split into batches
        let batches: Vec<Result<RowBatch, GraphicaError>> = rows
            .chunks(batch_size)
            .map(|chunk| Ok(chunk.to_vec()))
            .collect();

        let stream = stream::iter(batches);
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
        tracing::info!("DB2 export_to_format: {} as {:?}", table_name, format);

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
                "{:?} export not yet implemented for DB2",
                format
            ))),
        }
    }

    async fn estimate_row_count(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
    ) -> ConnectorV2Result<Option<u64>> {
        let db2_config = Self::extract_config(source)?;
        let schema_name = db2_config.schema.as_deref().unwrap_or("DB2INST1");

        tracing::info!("DB2 estimate_row_count: {}.{}", schema_name, table_name);

        let query = format!(
            "SELECT CARD FROM SYSCAT.TABLES WHERE TABSCHEMA = '{}' AND TABNAME = '{}'",
            schema_name.to_uppercase(),
            table_name.to_uppercase()
        );

        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            db2_config.database, db2_config.host, db2_config.port, credentials.username, credentials.password
        );

        let row_count = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("DB2 connection failed: {:?}", e)))?;

            let mut cursor = conn
                .execute(&query, (), None)
                .map_err(|e| GraphicaError::Internal(format!("Row count query failed: {:?}", e)))?
                .ok_or_else(|| {
                    GraphicaError::Internal("No results from row count query".to_string())
                })?;

            let result = if let Some(mut row) = cursor.next_row().map_err(|e| {
                GraphicaError::Internal(format!("Failed to fetch row count: {:?}", e))
            })? {
                let mut buffer = Vec::new();
                let has_value = row.get_text(1, &mut buffer).map_err(|e| {
                    GraphicaError::Internal(format!("Failed to get CARD value: {:?}", e))
                })?;

                if has_value {
                    let count_str = String::from_utf8_lossy(&buffer).to_string();
                    count_str.parse::<u64>().ok()
                } else {
                    None
                }
            } else {
                None
            };

            Ok::<Option<u64>, GraphicaError>(result)
        })
        .await
        .map_err(|e| GraphicaError::Internal(format!("Estimate row count task failed: {}", e)))??;

        Ok(row_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::types::ConnectionDetails;

    macro_rules! require_live_db2 {
        () => {
            if std::env::var_os("ARCXA_RUN_LIVE_DB2_TESTS").is_none() {
                eprintln!(
                    "Skipping live DB2 connector test; set ARCXA_RUN_LIVE_DB2_TESTS=1 to enable"
                );
                return;
            }
        };
    }

    fn create_test_source() -> DataSource {
        DataSource::new(
            "Test DB2".to_string(),
            "DB2".to_string(),
            ConnectionDetails {
                secret_ref: "test://secret".to_string(),
                config: SourceConfig::DB2(DB2Config {
                    host: "db2-server.example.com".to_string(),
                    port: 50000,
                    database: "SAMPLE".to_string(),
                    schema: Some("MYSCHEMA".to_string()),
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        )
    }

    #[test]
    fn test_validate_config() {
        let connector = DB2Connector::new();
        let source = create_test_source();

        let result = connector
            .validate_config(&source.connection.config)
            .unwrap();
        assert!(result.valid);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_invalid_config() {
        let connector = DB2Connector::new();
        let invalid_config = SourceConfig::DB2(DB2Config {
            host: "".to_string(),
            port: 50000,
            database: "".to_string(),
            schema: None,
        });

        let result = connector.validate_config(&invalid_config).unwrap();
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 2); // host and database
    }

    #[tokio::test]
    async fn test_connection() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let result = connector.test_connection(&source, creds).await.unwrap();
        assert!(result.success);
        assert_eq!(result.metadata.get("database"), Some(&"SAMPLE".to_string()));
    }

    #[tokio::test]
    async fn test_infer_schema() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let schema = connector
            .infer_schema(&source, creds, Some("EMPLOYEE"), 1000)
            .await
            .unwrap();

        assert_eq!(schema.name, "MYSCHEMA");
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].name, "EMPLOYEE");
    }

    #[tokio::test]
    async fn test_execute_query() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let result = connector
            .execute_query(
                &source,
                creds,
                "SELECT * FROM EMPLOYEE",
                HashMap::new(),
                Some(10),
                30,
            )
            .await
            .unwrap();

        assert_eq!(result.row_count, 1);
        assert!(!result.truncated);
    }

    #[test]
    fn test_capabilities() {
        let connector = DB2Connector::new();
        let caps = connector.capabilities();

        assert!(!caps.parameterized_queries);
        assert!(caps.schema_inference);
        assert!(caps.transactions);
        assert_eq!(caps.max_batch_size, Some(10000));
    }

    // ============================================================================
    // V2 Interface Tests
    // ============================================================================

    #[test]
    fn test_get_profiler() {
        let connector = DB2Connector::new();
        let _profiler = connector.get_profiler();
        // Profiler creation should succeed
    }

    #[tokio::test]
    async fn test_get_unified_schema() {
        require_live_db2!();
        use crate::schema::SourceType;

        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let schema = connector
            .get_unified_schema(&source, creds, "EMPLOYEE", ProfileConfig::default())
            .await
            .unwrap();

        assert_eq!(schema.name, "EMPLOYEE");
        assert_eq!(schema.source_type, SourceType::DB2);
        assert!(schema.source_ref.contains("db2://"));
        assert!(schema.source_ref.contains("db2-server.example.com"));
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.row_count, Some(2500));

        // Verify field names
        assert_eq!(schema.fields[0].name, "EMPNO");
        assert_eq!(schema.fields[1].name, "FIRSTNME");
        assert_eq!(schema.fields[2].name, "LASTNAME");
    }

    #[tokio::test]
    async fn test_stream_data() {
        require_live_db2!();
        use futures::StreamExt;

        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let mut stream = connector
            .stream_data(&source, creds, "SELECT * FROM EMPLOYEE", 1000)
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
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let csv_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEE",
                ExportFormat::Csv,
                ExportConfig::default(),
            )
            .await
            .unwrap();

        let csv_str = String::from_utf8(csv_data).unwrap();
        assert!(csv_str.contains("EMPNO")); // Header
        assert!(csv_str.contains("John") || csv_str.contains("Jane")); // Data
    }

    #[tokio::test]
    async fn test_export_to_json_lines() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let json_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEE",
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
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let json_data = connector
            .export_to_format(
                &source,
                creds,
                "EMPLOYEE",
                ExportFormat::JsonArray,
                ExportConfig::default(),
            )
            .await
            .unwrap();

        let rows: Vec<SampleRow> = serde_json::from_slice(&json_data).unwrap();
        assert!(rows.len() > 0);
        assert!(rows[0].contains_key("EMPNO"));
    }

    #[tokio::test]
    async fn test_estimate_row_count() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let count = connector
            .estimate_row_count(&source, creds, "EMPLOYEE")
            .await
            .unwrap();

        assert!(count.is_some());
        assert_eq!(count.unwrap(), 2500);
    }

    #[tokio::test]
    async fn test_get_sample_rows() {
        require_live_db2!();
        let connector = DB2Connector::new();
        let source = create_test_source();
        let creds = Credentials::new("db2admin".to_string(), "password".to_string());

        let rows = connector
            .get_sample_rows(&source, creds, "EMPLOYEE", 10)
            .await
            .unwrap();

        assert!(rows.len() > 0);
        assert!(rows.len() <= 10);
    }
}
