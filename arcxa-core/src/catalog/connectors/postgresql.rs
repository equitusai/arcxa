//! PostgreSQL Connector
//!
//! Connects to PostgreSQL databases via native protocol.
//! V2 interface adds unified profiling and streaming capabilities.

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use tokio::time::{timeout, Duration};
use tokio_postgres::{types::ToSql, Client};

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
    postgres_tls::connect_postgres_client,
    types::{DataSource, PostgreSQLConfig, SourceConfig},
};
use crate::errors::GraphicaError;
use crate::schema::{
    DataProfiler, PostgresProfiler, ProfileConfig, SampleRow, SourceType, TypeConverter,
    UnifiedSchema,
};

/// PostgreSQL connector
pub struct PostgreSQLConnector;

impl PostgreSQLConnector {
    pub fn new() -> Self {
        Self
    }

    fn extract_config(source: &DataSource) -> ConnectorResult<&PostgreSQLConfig> {
        match &source.connection.config {
            SourceConfig::PostgreSQL(config) => Ok(config),
            _ => Err(GraphicaError::Configuration(
                "Expected PostgreSQL configuration".to_string(),
            )),
        }
    }

    async fn connect(
        config: &PostgreSQLConfig,
        credentials: &Credentials,
    ) -> ConnectorResult<Client> {
        let mut connection_string = format!(
            "host={} port={} user={} password={} dbname={}",
            config.host, config.port, credentials.username, credentials.password, config.database
        );

        if let Some(ssl_mode) = &config.ssl_mode {
            connection_string.push_str(&format!(" sslmode={}", ssl_mode));
        }

        connect_postgres_client(&connection_string, config.ssl_mode.as_deref())
            .await
            .map_err(|e| GraphicaError::Internal(format!("PostgreSQL connection failed: {}", e)))
    }

    fn apply_limit_if_needed(query: &str, limit: Option<usize>) -> String {
        let Some(limit) = limit else {
            return query.trim().trim_end_matches(';').to_string();
        };

        let trimmed = query.trim().trim_end_matches(';');
        if trimmed.to_uppercase().contains("LIMIT ") {
            return trimmed.to_string();
        }

        format!("{} LIMIT {}", trimmed, limit)
    }

    fn rewrite_named_parameters(
        query: &str,
        parameters: &HashMap<String, serde_json::Value>,
    ) -> ConnectorResult<(String, Vec<PostgresParam>)> {
        if parameters.is_empty() {
            return Ok((query.to_string(), Vec::new()));
        }

        let chars: Vec<char> = query.chars().collect();
        let mut rewritten = String::with_capacity(query.len());
        let mut ordered = Vec::new();
        let mut positions: HashMap<String, usize> = HashMap::new();
        let mut i = 0;
        let mut in_single_quote = false;

        while i < chars.len() {
            let ch = chars[i];

            if ch == '\'' {
                rewritten.push(ch);
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    rewritten.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_single_quote = !in_single_quote;
                i += 1;
                continue;
            }

            if !in_single_quote
                && ch == ':'
                && (i == 0 || chars[i - 1] != ':')
                && i + 1 < chars.len()
                && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
            {
                let start = i + 1;
                let mut end = start + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }

                let name = chars[start..end].iter().collect::<String>();
                let position = if let Some(position) = positions.get(&name) {
                    *position
                } else {
                    let value = parameters.get(&name).ok_or_else(|| {
                        GraphicaError::Configuration(format!(
                            "Missing value for PostgreSQL query parameter '{}'",
                            name
                        ))
                    })?;
                    ordered.push(PostgresParam::from_json(value)?);
                    let position = ordered.len();
                    positions.insert(name.clone(), position);
                    position
                };

                rewritten.push_str(&format!("${}", position));
                i = end;
                continue;
            }

            rewritten.push(ch);
            i += 1;
        }

        Ok((rewritten, ordered))
    }

    fn row_value_to_json(row: &tokio_postgres::Row, idx: usize) -> serde_json::Value {
        if let Ok(value) = row.try_get::<_, Option<bool>>(idx) {
            return value
                .map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<i16>>(idx) {
            return value
                .map(|v| serde_json::Value::Number((v as i64).into()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<i32>>(idx) {
            return value
                .map(|v| serde_json::Value::Number(v.into()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<i64>>(idx) {
            return value
                .map(|v| serde_json::Value::Number(v.into()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<f64>>(idx) {
            return value
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<NaiveDate>>(idx) {
            return value
                .map(|v| serde_json::Value::String(v.format("%Y-%m-%d").to_string()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<NaiveDateTime>>(idx) {
            return value
                .map(|v| serde_json::Value::String(v.format("%Y-%m-%dT%H:%M:%S%.f").to_string()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<DateTime<Utc>>>(idx) {
            return value
                .map(|v| serde_json::Value::String(v.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<DateTime<FixedOffset>>>(idx) {
            return value
                .map(|v| serde_json::Value::String(v.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null);
        }
        if let Ok(value) = row.try_get::<_, Option<String>>(idx) {
            return value
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);
        }

        serde_json::Value::Null
    }

    fn row_to_json(row: &tokio_postgres::Row) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        for (idx, column) in row.columns().iter().enumerate() {
            obj.insert(column.name().to_string(), Self::row_value_to_json(row, idx));
        }
        serde_json::Value::Object(obj)
    }

    fn column_definitions(columns: &[tokio_postgres::Column]) -> Vec<ColumnDefinition> {
        columns
            .iter()
            .map(|column| ColumnDefinition {
                name: column.name().to_string(),
                data_type: column.type_().name().to_string(),
                nullable: true,
                primary_key: false,
                default_value: None,
                semantic_type: None,
                statistics: None,
            })
            .collect()
    }

    fn normalize_identifier_segment(segment: &str) -> ConnectorResult<String> {
        let trimmed = segment.trim().trim_matches('"').trim_matches('`');
        if trimmed.is_empty() {
            return Err(GraphicaError::Configuration(
                "PostgreSQL identifier segment cannot be empty".to_string(),
            ));
        }

        if trimmed
            .chars()
            .any(|ch| ch == '\0' || ch.is_ascii_control())
        {
            return Err(GraphicaError::Configuration(
                "PostgreSQL identifier segment contains unsupported control characters".to_string(),
            ));
        }

        Ok(trimmed.replace("\"\"", "\""))
    }

    fn quote_identifier_segment(segment: &str) -> ConnectorResult<String> {
        let normalized = Self::normalize_identifier_segment(segment)?;
        Ok(format!("\"{}\"", normalized.replace('"', "\"\"")))
    }

    fn quote_qualified_identifier(identifier: &str) -> ConnectorResult<String> {
        let parts = identifier
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(Self::quote_identifier_segment)
            .collect::<ConnectorResult<Vec<_>>>()?;

        if parts.is_empty() {
            return Err(GraphicaError::Configuration(
                "Qualified PostgreSQL identifier cannot be empty".to_string(),
            ));
        }

        Ok(parts.join("."))
    }

    fn split_schema_and_table(
        config: &PostgreSQLConfig,
        table_name: &str,
    ) -> ConnectorResult<(String, String)> {
        let parts = table_name
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();

        match parts.as_slice() {
            [table] => Ok((
                config
                    .schema
                    .clone()
                    .unwrap_or_else(|| "public".to_string()),
                Self::normalize_identifier_segment(table)?,
            )),
            [schema, table] => Ok((
                Self::normalize_identifier_segment(schema)?,
                Self::normalize_identifier_segment(table)?,
            )),
            [_, schema, table] => Ok((
                Self::normalize_identifier_segment(schema)?,
                Self::normalize_identifier_segment(table)?,
            )),
            _ => Err(GraphicaError::Configuration(format!(
                "Unsupported PostgreSQL table reference '{}'",
                table_name
            ))),
        }
    }

    async fn fetch_schema_definition(
        client: &Client,
        schema_name: &str,
        table_name: Option<&str>,
    ) -> ConnectorResult<SchemaDefinition> {
        let table_name_param = table_name.map(|name| name.to_string());

        let rows = client
            .query(
                r#"
SELECT
    c.table_name,
    c.column_name,
    c.data_type,
    c.is_nullable,
    c.column_default,
    EXISTS (
        SELECT 1
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
            AND tc.table_schema = c.table_schema
            AND tc.table_name = c.table_name
            AND kcu.column_name = c.column_name
    ) AS is_primary_key
FROM information_schema.columns c
WHERE c.table_schema = $1
    AND ($2::text IS NULL OR c.table_name = $2)
ORDER BY c.table_name, c.ordinal_position
                "#,
                &[&schema_name, &table_name_param],
            )
            .await
            .map_err(|e| {
                GraphicaError::Internal(format!("PostgreSQL schema inference failed: {}", e))
            })?;

        let mut tables_map: HashMap<String, TableDefinition> = HashMap::new();
        for row in rows {
            let table = row.get::<_, String>("table_name");
            let column = ColumnDefinition {
                name: row.get::<_, String>("column_name"),
                data_type: row.get::<_, String>("data_type"),
                nullable: row.get::<_, String>("is_nullable") == "YES",
                primary_key: row.get::<_, bool>("is_primary_key"),
                default_value: row.get::<_, Option<String>>("column_default"),
                semantic_type: None,
                statistics: None,
            };

            tables_map
                .entry(table.clone())
                .or_insert_with(|| TableDefinition {
                    name: table.clone(),
                    columns: Vec::new(),
                    estimated_rows: None,
                })
                .columns
                .push(column);
        }

        Ok(SchemaDefinition {
            name: schema_name.to_string(),
            tables: tables_map.into_values().collect(),
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
    }

    fn build_stream_base_query(
        _source: &PostgreSQLConfig,
        table_or_query: &str,
    ) -> ConnectorResult<String> {
        let trimmed = table_or_query.trim().trim_end_matches(';').trim();
        if trimmed.is_empty() {
            return Err(GraphicaError::Configuration(
                "PostgreSQL stream source cannot be empty".to_string(),
            ));
        }

        let uppercase = trimmed.to_ascii_uppercase();
        if uppercase.starts_with("SELECT ") || uppercase.starts_with("WITH ") {
            return Ok(format!(
                "SELECT * FROM ({trimmed}) AS graphica_stream_batch"
            ));
        }

        Ok(format!(
            "SELECT * FROM {}",
            Self::quote_qualified_identifier(trimmed)?
        ))
    }

    fn row_to_sample_row(row: &tokio_postgres::Row) -> SampleRow {
        row.columns()
            .iter()
            .enumerate()
            .map(|(idx, column)| (column.name().to_string(), Self::row_value_to_json(row, idx)))
            .collect()
    }
}

enum PostgresParam {
    Null(Option<String>),
    Bool(bool),
    I64(i64),
    F64(f64),
    Text(String),
}

impl PostgresParam {
    fn from_json(value: &serde_json::Value) -> ConnectorResult<Self> {
        match value {
            serde_json::Value::Null => Ok(Self::Null(None)),
            serde_json::Value::Bool(value) => Ok(Self::Bool(*value)),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    Ok(Self::I64(value))
                } else if let Some(value) = value.as_f64() {
                    Ok(Self::F64(value))
                } else {
                    Ok(Self::Text(value.to_string()))
                }
            }
            serde_json::Value::String(value) => Ok(Self::Text(value.clone())),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                Ok(Self::Text(value.to_string()))
            }
        }
    }

    fn as_tosql(&self) -> &(dyn ToSql + Sync) {
        match self {
            Self::Null(value) => value,
            Self::Bool(value) => value,
            Self::I64(value) => value,
            Self::F64(value) => value,
            Self::Text(value) => value,
        }
    }
}

impl Default for PostgreSQLConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for PostgreSQLConnector {
    fn name(&self) -> &'static str {
        "PostgreSQL Connector"
    }

    fn source_type(&self) -> &'static str {
        "PostgreSQL"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::PostgreSQL(pg_config) => {
                let mut errors = vec![];
                let mut warnings = vec![];

                if pg_config.host.is_empty() {
                    errors.push("Host cannot be empty".to_string());
                }

                if pg_config.database.is_empty() {
                    errors.push("Database cannot be empty".to_string());
                }

                if pg_config.port == 0 {
                    errors.push("Port must be greater than 0".to_string());
                }

                // Warn about SSL mode
                if pg_config.ssl_mode.is_none() {
                    warnings.push("No SSL mode specified, connection may be insecure".to_string());
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid().with_warnings(warnings))
                } else {
                    Ok(ValidationResult::invalid(errors).with_warnings(warnings))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected PostgreSQL configuration".to_string(),
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
            "PostgreSQL connection test: {}:{}/{}",
            config.host,
            config.port,
            config.database
        );

        let mut connection_string = format!(
            "host={} port={} user={} password={} dbname={}",
            config.host, config.port, credentials.username, credentials.password, config.database
        );

        if let Some(ssl_mode) = &config.ssl_mode {
            connection_string.push_str(&format!(" sslmode={}", ssl_mode));
        }

        connection_string.push_str(" connect_timeout=5");

        let connect = async {
            let client = connect_postgres_client(&connection_string, config.ssl_mode.as_deref())
                .await
                .map_err(|e| {
                    GraphicaError::Internal(format!("PostgreSQL connection failed: {}", e))
                })?;

            client.query_one("SELECT 1", &[]).await.map_err(|e| {
                GraphicaError::Internal(format!("PostgreSQL health-check failed: {}", e))
            })?;

            Ok::<(), GraphicaError>(())
        };

        let result = match timeout(Duration::from_secs(5), connect).await {
            Ok(res) => res,
            Err(_) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                return Ok(ConnectionTestResult {
                    success: false,
                    duration_ms,
                    error: Some("PostgreSQL connection test timed out".to_string()),
                    metadata: HashMap::from([
                        ("host".to_string(), config.host.clone()),
                        ("port".to_string(), config.port.to_string()),
                        ("database".to_string(), config.database.clone()),
                        (
                            "ssl_mode".to_string(),
                            config
                                .ssl_mode
                                .clone()
                                .unwrap_or_else(|| "default".to_string()),
                        ),
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
                        "ssl_mode".to_string(),
                        config
                            .ssl_mode
                            .clone()
                            .unwrap_or_else(|| "default".to_string()),
                    ),
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
                        "ssl_mode".to_string(),
                        config
                            .ssl_mode
                            .clone()
                            .unwrap_or_else(|| "default".to_string()),
                    ),
                ]),
                tested_at: Utc::now(),
            }),
        }
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        table_name: Option<&str>,
        _sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = Self::extract_config(source)?;
        let client = Self::connect(config, &_credentials).await?;
        let (schema_name, inferred_table_name) = if let Some(table_name) = table_name {
            let (schema_name, table_name) = Self::split_schema_and_table(config, table_name)?;
            (schema_name, Some(table_name))
        } else {
            (
                config
                    .schema
                    .clone()
                    .unwrap_or_else(|| "public".to_string()),
                None,
            )
        };

        Self::fetch_schema_definition(&client, &schema_name, inferred_table_name.as_deref()).await
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let config = Self::extract_config(source)?;
        let start = std::time::Instant::now();
        let client = Self::connect(config, &_credentials).await?;
        let limited_query = Self::apply_limit_if_needed(query, limit);
        let (bound_query, ordered_params) =
            Self::rewrite_named_parameters(&limited_query, &parameters)?;
        let parameter_refs: Vec<&(dyn ToSql + Sync)> = ordered_params
            .iter()
            .map(|value| value.as_tosql())
            .collect();
        let statement = client.prepare(&bound_query).await.map_err(|e| {
            GraphicaError::Internal(format!("PostgreSQL query prepare failed: {}", e))
        })?;
        let wrapped_query = format!(
            "SELECT to_jsonb(graphica_row) AS graphica_row FROM ({bound_query}) AS graphica_row"
        );

        tracing::info!(
            "PostgreSQL query execution: {} (limit: {:?}, params={})",
            bound_query,
            limit,
            parameter_refs.len()
        );

        let rows = timeout(
            Duration::from_secs(timeout_secs),
            client.query(&wrapped_query, &parameter_refs),
        )
        .await
        .map_err(|_| {
            GraphicaError::Internal(format!(
                "PostgreSQL query timed out after {}s",
                timeout_secs
            ))
        })?
        .map_err(|e| GraphicaError::Internal(format!("PostgreSQL query failed: {}", e)))?;

        let execution_time_ms = start.elapsed().as_millis() as u64;
        let row_count = rows.len();
        let truncated = limit.map(|value| row_count >= value).unwrap_or(false);
        let columns = Some(Self::column_definitions(statement.columns()));
        let rows = rows
            .iter()
            .map(|row| {
                row.try_get::<_, serde_json::Value>("graphica_row")
                    .map_err(|e| {
                        GraphicaError::Internal(format!(
                            "PostgreSQL query JSON materialization failed: {}",
                            e
                        ))
                    })
            })
            .collect::<ConnectorResult<Vec<_>>>()?;

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
            parameterized_queries: true,
            schema_inference: true,
            query_timeout: true,
            streaming: true,
            transactions: true,
            max_batch_size: Some(50000),
        }
    }
}

// V2 Interface Implementation
#[async_trait]
impl DataSourceConnectorV2 for PostgreSQLConnector {
    fn get_profiler(&self) -> Box<dyn DataProfiler> {
        // Return PostgreSQL profiler with appropriate connection ID
        Box::new(PostgresProfiler::new("postgres".to_string()))
    }

    async fn get_unified_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
        config: ProfileConfig,
    ) -> ConnectorV2Result<UnifiedSchema> {
        let pg_config = Self::extract_config(source)?;
        let schema_definition = self
            .infer_schema(
                source,
                credentials.clone(),
                Some(table_name),
                config.sample_size.unwrap_or(1000),
            )
            .await?;
        let (_, bare_table_name) = Self::split_schema_and_table(pg_config, table_name)?;
        let connection_id = format!(
            "postgres://{}:{}/{}",
            pg_config.host, pg_config.port, pg_config.database
        );

        let table_definition = schema_definition
            .tables
            .iter()
            .find(|table| table.name == bare_table_name)
            .or_else(|| schema_definition.tables.first())
            .ok_or_else(|| {
                GraphicaError::NotFound(format!(
                    "PostgreSQL table '{}' not found during unified schema inference",
                    table_name
                ))
            })?;

        let mut schema = UnifiedSchema::new(
            table_definition.name.clone(),
            SourceType::PostgreSQL,
            connection_id.clone(),
        );

        for column in &table_definition.columns {
            schema.add_field(TypeConverter::column_to_unified_field(
                column,
                connection_id.clone(),
            ));
        }

        schema.metadata.insert(
            "schema_name".to_string(),
            serde_json::Value::String(schema_definition.name.clone()),
        );
        schema.row_count = self
            .estimate_row_count(source, credentials, table_name)
            .await?;
        schema.last_profiled = Some(Utc::now());

        Ok(schema)
    }

    async fn stream_data(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_or_query: &str,
        batch_size: usize,
    ) -> ConnectorV2Result<DataStream> {
        let config = Self::extract_config(source)?;
        let client = Self::connect(config, &credentials).await?;
        let base_query = Self::build_stream_base_query(config, table_or_query)?;
        let effective_batch_size = batch_size.max(1);

        tracing::info!(
            "Streaming data from PostgreSQL: {} (batch_size: {}, user: {})",
            table_or_query,
            effective_batch_size,
            credentials.username
        );

        let stream = stream::try_unfold((client, Some(0usize)), move |(client, next_offset)| {
            let base_query = base_query.clone();
            async move {
                let Some(offset) = next_offset else {
                    return Ok(None);
                };

                let paged_query = format!(
                    "{} LIMIT {} OFFSET {}",
                    base_query, effective_batch_size, offset
                );

                let rows = client.query(&paged_query, &[]).await.map_err(|e| {
                    GraphicaError::Internal(format!("PostgreSQL stream query failed: {}", e))
                })?;

                if rows.is_empty() {
                    return Ok(None);
                }

                let batch = rows.iter().map(Self::row_to_sample_row).collect::<Vec<_>>();
                let fetched = batch.len();

                let next_offset = if fetched < effective_batch_size {
                    None
                } else {
                    Some(offset + fetched)
                };

                Ok(Some((batch, (client, next_offset))))
            }
        });

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
        // Stream all data
        let mut data_stream = self
            .stream_data(
                source,
                credentials,
                table_name,
                config.max_rows.unwrap_or(10000),
            )
            .await?;

        // Collect all rows
        let mut all_rows: Vec<SampleRow> = Vec::new();
        let max_rows = config.max_rows.unwrap_or(usize::MAX);

        while let Some(batch_result) = data_stream.next().await {
            let batch = batch_result?;
            all_rows.extend(batch);

            if all_rows.len() >= max_rows {
                all_rows.truncate(max_rows);
                break;
            }
        }

        // Export based on format
        match format {
            ExportFormat::Csv => self.export_to_csv(&all_rows, &config),
            ExportFormat::JsonLines => self.export_to_json_lines(&all_rows),
            ExportFormat::JsonArray => self.export_to_json_array(&all_rows),
            ExportFormat::Parquet => Err(GraphicaError::Internal(
                "Parquet export not yet implemented".to_string(),
            )),
            ExportFormat::Arrow => Err(GraphicaError::Internal(
                "Arrow export not yet implemented".to_string(),
            )),
        }
    }

    async fn estimate_row_count(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: &str,
    ) -> ConnectorV2Result<Option<u64>> {
        let config = Self::extract_config(source).map_err(|e| {
            GraphicaError::Configuration(format!("Config extraction failed: {}", e))
        })?;
        let client = Self::connect(config, &credentials).await?;
        let (schema_name, bare_table_name) = Self::split_schema_and_table(config, table_name)?;

        tracing::info!(
            "Estimating row count for {}.{}",
            schema_name,
            bare_table_name
        );

        let row = client
            .query_opt(
                r#"
SELECT c.reltuples::bigint
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1
  AND c.relname = $2
  AND c.relkind IN ('r', 'p', 'm', 'f')
                "#,
                &[&schema_name, &bare_table_name],
            )
            .await
            .map_err(|e| {
                GraphicaError::Internal(format!("PostgreSQL row count estimate failed: {}", e))
            })?;

        Ok(row.map(|row| row.get::<_, i64>(0).max(0) as u64))
    }
}

impl PostgreSQLConnector {
    /// Export rows to CSV format
    fn export_to_csv(
        &self,
        rows: &[SampleRow],
        config: &ExportConfig,
    ) -> ConnectorV2Result<Vec<u8>> {
        use std::io::Write;

        let mut output = Vec::new();
        let csv_opts = config.csv_options.as_ref();
        let delimiter = csv_opts.map(|o| o.delimiter).unwrap_or(b',');

        // Get column names from first row
        if rows.is_empty() {
            return Ok(output);
        }

        let columns: Vec<String> = rows[0].keys().cloned().collect();

        // Write header if requested
        if config.include_headers {
            let header = columns.join(&(delimiter as char).to_string());
            writeln!(output, "{}", header)
                .map_err(|e| GraphicaError::Internal(format!("CSV write error: {}", e)))?;
        }

        // Write rows
        for row in rows {
            let values: Vec<String> = columns
                .iter()
                .map(|col| {
                    row.get(col)
                        .map(|v| match v {
                            serde_json::Value::String(s) => format!("\"{}\"", s),
                            serde_json::Value::Null => String::new(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default()
                })
                .collect();

            let line = values.join(&(delimiter as char).to_string());
            writeln!(output, "{}", line)
                .map_err(|e| GraphicaError::Internal(format!("CSV write error: {}", e)))?;
        }

        Ok(output)
    }

    /// Export rows to JSON Lines format (newline-delimited JSON)
    fn export_to_json_lines(&self, rows: &[SampleRow]) -> ConnectorV2Result<Vec<u8>> {
        use std::io::Write;

        let mut output = Vec::new();

        for row in rows {
            let json = serde_json::to_string(row)
                .map_err(|e| GraphicaError::Serialization(e.to_string()))?;
            writeln!(output, "{}", json)
                .map_err(|e| GraphicaError::Internal(format!("JSON write error: {}", e)))?;
        }

        Ok(output)
    }

    /// Export rows to JSON array format
    fn export_to_json_array(&self, rows: &[SampleRow]) -> ConnectorV2Result<Vec<u8>> {
        let json = serde_json::to_vec_pretty(rows)
            .map_err(|e| GraphicaError::Serialization(e.to_string()))?;
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::types::ConnectionDetails;

    fn create_test_source() -> DataSource {
        DataSource::new(
            "Test PostgreSQL".to_string(),
            "PostgreSQL".to_string(),
            ConnectionDetails {
                secret_ref: "test://secret".to_string(),
                config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "testdb".to_string(),
                    schema: Some("public".to_string()),
                    ssl_mode: Some("require".to_string()),
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        )
    }

    #[test]
    fn test_validate_config() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();

        let result = connector
            .validate_config(&source.connection.config)
            .unwrap();
        assert!(result.valid);
    }

    #[tokio::test]
    async fn test_connection() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let result = connector.test_connection(&source, creds).await.unwrap();
        if !result.success {
            eprintln!(
                "Skipping PostgreSQL connection test (no live database available): {:?}",
                result.error
            );
            return;
        }
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_infer_schema() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        match connector
            .infer_schema(&source, creds, Some("users"), 1000)
            .await
        {
            Ok(schema) => {
                assert_eq!(schema.name, "public");
                assert_eq!(schema.tables.len(), 1);
            }
            Err(err) => {
                eprintln!(
                    "Skipping PostgreSQL schema inference test (no live database available): {}",
                    err
                );
            }
        }
    }

    #[tokio::test]
    async fn test_execute_query() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        match connector
            .execute_query(
                &source,
                creds,
                "SELECT 1 AS id",
                HashMap::new(),
                Some(10),
                30,
            )
            .await
        {
            Ok(result) => {
                assert_eq!(result.row_count, 1);
                assert!(!result.truncated);
            }
            Err(err) => {
                eprintln!(
                    "Skipping PostgreSQL query execution test (no live database available): {}",
                    err
                );
            }
        }
    }

    // V2 Interface Tests
    #[test]
    fn test_get_profiler() {
        let connector = PostgreSQLConnector::new();
        let _profiler = connector.get_profiler();

        // Profiler should be created successfully - test passes if no panic
        assert!(true);
    }

    #[tokio::test]
    async fn test_get_unified_schema() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());
        let config = ProfileConfig::default();

        let schema = match connector
            .get_unified_schema(&source, creds, "users", config)
            .await
        {
            Ok(schema) => schema,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL unified schema test (no live database available): {}",
                    error
                );
                return;
            }
        };

        assert_eq!(schema.name, "users");
        assert_eq!(schema.source_type, SourceType::PostgreSQL);
        assert!(!schema.fields.is_empty());
    }

    #[tokio::test]
    async fn test_stream_data() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let mut stream = match connector.stream_data(&source, creds, "users", 100).await {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL stream_data test (no live database available): {}",
                    error
                );
                return;
            }
        };

        // Collect all batches
        let mut total_rows = 0;
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.unwrap();
            total_rows += batch.len();
        }

        assert!(total_rows > 0);
    }

    #[tokio::test]
    async fn test_export_to_csv() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let csv_data = match connector
            .export_to_format(
                &source,
                creds,
                "users",
                ExportFormat::Csv,
                ExportConfig::default(),
            )
            .await
        {
            Ok(data) => data,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL CSV export test (no live database available): {}",
                    error
                );
                return;
            }
        };

        let csv_string = String::from_utf8(csv_data).unwrap();

        // Should have header + 3 data rows
        let lines: Vec<&str> = csv_string.lines().collect();
        assert!(lines.len() >= 3); // At least header + 2 rows

        // Check header contains column names
        assert!(lines[0].contains("id") || lines[0].contains("email"));
    }

    #[tokio::test]
    async fn test_export_to_json_lines() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let json_data = match connector
            .export_to_format(
                &source,
                creds,
                "users",
                ExportFormat::JsonLines,
                ExportConfig::default(),
            )
            .await
        {
            Ok(data) => data,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL JSON lines export test (no live database available): {}",
                    error
                );
                return;
            }
        };

        let json_string = String::from_utf8(json_data).unwrap();
        let lines: Vec<&str> = json_string.lines().collect();

        // Should have 3 JSON lines
        assert_eq!(lines.len(), 3);

        // Each line should be valid JSON
        for line in lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    #[tokio::test]
    async fn test_export_to_json_array() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let json_data = match connector
            .export_to_format(
                &source,
                creds,
                "users",
                ExportFormat::JsonArray,
                ExportConfig::default(),
            )
            .await
        {
            Ok(data) => data,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL JSON array export test (no live database available): {}",
                    error
                );
                return;
            }
        };

        let json_string = String::from_utf8(json_data).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_string).unwrap();

        // Should be an array
        assert!(parsed.is_array());
        let array = parsed.as_array().unwrap();
        assert!(!array.is_empty());
    }

    #[tokio::test]
    async fn test_estimate_row_count() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let estimate = match connector.estimate_row_count(&source, creds, "users").await {
            Ok(estimate) => estimate,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL row count estimate test (no live database available): {}",
                    error
                );
                return;
            }
        };

        assert!(estimate.is_some());
    }

    #[tokio::test]
    async fn test_get_sample_rows() {
        let connector = PostgreSQLConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        let samples = match connector.get_sample_rows(&source, creds, "users", 2).await {
            Ok(samples) => samples,
            Err(error) => {
                eprintln!(
                    "Skipping PostgreSQL sample rows test (no live database available): {}",
                    error
                );
                return;
            }
        };

        assert!(samples.len() <= 2);
        assert!(!samples.is_empty());
    }

    #[test]
    fn quotes_postgresql_qualified_identifiers() {
        let quoted = PostgreSQLConnector::quote_qualified_identifier("public.User").unwrap();
        assert_eq!(quoted, "\"public\".\"User\"");
    }
}
