//! Databricks Connector
//!
//! Native Databricks SQL Warehouse connector built on the Databricks SQL
//! Statement Execution API. This path is the source of truth for connection
//! testing, schema inference, query execution, and workflow query access.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;
use tokio::time::{sleep, Instant};

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

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const POLL_INTERVAL_MS: u64 = 250;

pub struct DatabricksConnector;

#[derive(Clone)]
pub struct DatabricksSqlClient {
    client: Client,
    workspace_url: String,
    warehouse_id: String,
    catalog: Option<String>,
    schema: Option<String>,
    token: String,
}

impl DatabricksSqlClient {
    pub fn from_config(
        config: &DatabricksConfig,
        credentials: &Credentials,
    ) -> ConnectorResult<Self> {
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
            })
            .ok_or_else(|| {
                GraphicaError::Configuration(
                    "Databricks token missing: provide credentials.additional['token'] or password"
                        .to_string(),
                )
            })?;

        let warehouse_id = config
            .warehouse_id
            .clone()
            .or_else(|| Self::warehouse_id_from_http_path(&config.http_path))
            .ok_or_else(|| {
                GraphicaError::Configuration(
                    "Databricks warehouseId missing and could not be derived from httpPath"
                        .to_string(),
                )
            })?;

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());

        Ok(Self {
            client,
            workspace_url: config.workspace_url.trim_end_matches('/').to_string(),
            warehouse_id,
            catalog: config.catalog.clone(),
            schema: config.schema.clone(),
            token,
        })
    }

    pub async fn execute_query(
        &self,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let start = std::time::Instant::now();
        let (statement, applied_limit) = Self::apply_limit(query, limit);
        let execution = self
            .execute_statement_internal(&statement, parameters, timeout_secs)
            .await?;

        let columns = if execution.columns.is_empty() {
            None
        } else {
            Some(
                execution
                    .columns
                    .iter()
                    .map(|column| ColumnDefinition {
                        name: column.name.clone(),
                        data_type: Self::map_databricks_type(&column.type_name),
                        nullable: column.nullable.unwrap_or(true),
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    })
                    .collect(),
            )
        };

        let rows = execution
            .rows
            .into_iter()
            .map(|row| {
                let mut object = serde_json::Map::new();
                for (index, value) in row.into_iter().enumerate() {
                    if let Some(column) = execution.columns.get(index) {
                        object.insert(column.name.clone(), value);
                    }
                }
                serde_json::Value::Object(object)
            })
            .collect::<Vec<_>>();

        let row_count = rows.len();
        Ok(QueryResult {
            rows,
            row_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: applied_limit && limit.is_some() && row_count >= limit.unwrap_or_default(),
            columns,
        })
    }

    pub async fn execute_command(
        &self,
        statement: &str,
        parameters: HashMap<String, serde_json::Value>,
        timeout_secs: u64,
    ) -> ConnectorResult<()> {
        self.execute_statement_internal(statement, parameters, timeout_secs)
            .await
            .map(|_| ())
    }

    fn warehouse_id_from_http_path(http_path: &str) -> Option<String> {
        let trimmed = http_path.trim().trim_matches('/');
        let segments = trimmed.split('/').collect::<Vec<_>>();
        segments
            .windows(2)
            .find(|window| window[0].eq_ignore_ascii_case("warehouses"))
            .map(|window| window[1].to_string())
    }

    fn apply_limit(query: &str, limit: Option<usize>) -> (String, bool) {
        let trimmed = query.trim().trim_end_matches(';').trim();
        let Some(limit) = limit else {
            return (trimmed.to_string(), false);
        };

        let upper = trimmed.to_ascii_uppercase();
        let can_limit = upper.starts_with("SELECT") || upper.starts_with("WITH");
        let already_has_limit =
            upper.contains(" LIMIT ") || upper.ends_with(" LIMIT") || upper.contains("\nLIMIT ");

        if can_limit && !already_has_limit {
            (format!("{trimmed} LIMIT {limit}"), true)
        } else {
            (trimmed.to_string(), false)
        }
    }

    fn map_databricks_type(source_type: &str) -> String {
        let normalized = source_type.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "BOOLEAN" => "BOOLEAN".to_string(),
            "BYTE" | "SHORT" | "INT" | "INTEGER" | "LONG" | "BIGINT" => "INTEGER".to_string(),
            "FLOAT" | "DOUBLE" | "REAL" => "FLOAT".to_string(),
            "DECIMAL" | "NUMERIC" => "NUMERIC".to_string(),
            "DATE" => "DATE".to_string(),
            "TIMESTAMP" | "TIMESTAMP_NTZ" | "TIMESTAMP_LTZ" | "TIMESTAMP_TZ" => {
                "TIMESTAMP".to_string()
            }
            "BINARY" => "BINARY".to_string(),
            "STRING" | "VARCHAR" | "CHAR" => "VARCHAR".to_string(),
            value if value.starts_with("DECIMAL(") || value.starts_with("NUMERIC(") => {
                "NUMERIC".to_string()
            }
            value
                if value.starts_with("ARRAY")
                    || value.starts_with("MAP")
                    || value.starts_with("STRUCT") =>
            {
                "JSON".to_string()
            }
            _ => source_type.to_string(),
        }
    }

    fn statement_url(&self) -> String {
        format!("{}/api/2.0/sql/statements", self.workspace_url)
    }

    fn resolve_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            format!("{}{}", self.workspace_url, path)
        } else {
            format!("{}/{}", self.workspace_url, path.trim_start_matches('/'))
        }
    }

    fn default_catalog(&self) -> Option<&str> {
        self.catalog
            .as_deref()
            .filter(|value| !value.trim().is_empty())
    }

    fn default_schema(&self) -> Option<&str> {
        self.schema
            .as_deref()
            .filter(|value| !value.trim().is_empty())
    }

    async fn execute_statement_internal(
        &self,
        statement: &str,
        parameters: HashMap<String, serde_json::Value>,
        timeout_secs: u64,
    ) -> ConnectorResult<DatabricksStatementExecution> {
        let request = DatabricksStatementRequest {
            statement: statement.to_string(),
            warehouse_id: self.warehouse_id.clone(),
            catalog: self.default_catalog().map(str::to_string),
            schema: self.default_schema().map(str::to_string),
            disposition: "INLINE".to_string(),
            parameters: Self::encode_parameters(parameters)?,
        };

        let response = self
            .client
            .post(self.statement_url())
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                GraphicaError::Internal(format!("Failed to submit Databricks statement: {}", error))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            GraphicaError::Internal(format!("Failed to read Databricks response: {}", error))
        })?;

        if !status.is_success() {
            return Err(GraphicaError::Internal(format!(
                "Databricks statement submit failed with status {}: {}",
                status, body
            )));
        }

        let mut envelope: DatabricksStatementEnvelope =
            serde_json::from_str(&body).map_err(|error| {
                GraphicaError::Serialization(format!(
                    "Failed to parse Databricks statement response: {}",
                    error
                ))
            })?;

        envelope = self.wait_for_completion(envelope, timeout_secs).await?;
        self.collect_execution(envelope).await
    }

    async fn wait_for_completion(
        &self,
        mut envelope: DatabricksStatementEnvelope,
        timeout_secs: u64,
    ) -> ConnectorResult<DatabricksStatementEnvelope> {
        let timeout = timeout_secs.max(1);
        let deadline = Instant::now() + Duration::from_secs(timeout);

        loop {
            match envelope.status.state.to_ascii_uppercase().as_str() {
                "SUCCEEDED" => return Ok(envelope),
                "FAILED" | "CANCELED" | "CLOSED" => {
                    return Err(GraphicaError::Internal(Self::statement_error_message(
                        &envelope,
                        "Databricks statement failed",
                    )))
                }
                _ => {}
            }

            if Instant::now() >= deadline {
                let _ = self.cancel_statement(&envelope.statement_id).await;
                return Err(GraphicaError::BatchTimeout(timeout * 1000));
            }

            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
            envelope = self.get_statement(&envelope.statement_id).await?;
        }
    }

    async fn get_statement(
        &self,
        statement_id: &str,
    ) -> ConnectorResult<DatabricksStatementEnvelope> {
        let url = format!("{}/{}", self.statement_url(), statement_id);
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| {
                GraphicaError::Internal(format!(
                    "Failed to poll Databricks statement '{}': {}",
                    statement_id, error
                ))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            GraphicaError::Internal(format!(
                "Failed to read Databricks poll response '{}': {}",
                statement_id, error
            ))
        })?;

        if !status.is_success() {
            return Err(GraphicaError::Internal(format!(
                "Databricks poll failed with status {} for statement '{}': {}",
                status, statement_id, body
            )));
        }

        serde_json::from_str(&body).map_err(|error| {
            GraphicaError::Serialization(format!(
                "Failed to parse Databricks poll response '{}': {}",
                statement_id, error
            ))
        })
    }

    async fn cancel_statement(&self, statement_id: &str) -> ConnectorResult<()> {
        let url = format!("{}/{}/cancel", self.statement_url(), statement_id);
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| {
                GraphicaError::Internal(format!(
                    "Failed to cancel Databricks statement '{}': {}",
                    statement_id, error
                ))
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(GraphicaError::Internal(format!(
                "Databricks cancel failed with status {} for statement '{}'",
                response.status(),
                statement_id
            )))
        }
    }

    async fn collect_execution(
        &self,
        envelope: DatabricksStatementEnvelope,
    ) -> ConnectorResult<DatabricksStatementExecution> {
        let columns = envelope
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.schema.as_ref())
            .map(|schema| schema.columns.clone())
            .unwrap_or_default();

        let mut rows = envelope
            .result
            .as_ref()
            .map(|result| result.data_array.clone())
            .unwrap_or_default();
        let mut next_chunk = envelope
            .result
            .as_ref()
            .and_then(|result| result.next_chunk_internal_link.clone());

        while let Some(link) = next_chunk {
            let chunk = self.fetch_chunk(&link).await?;
            rows.extend(chunk.data_array);
            next_chunk = chunk.next_chunk_internal_link;
        }

        Ok(DatabricksStatementExecution { columns, rows })
    }

    async fn fetch_chunk(&self, path: &str) -> ConnectorResult<DatabricksStatementChunk> {
        let response = self
            .client
            .get(self.resolve_url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| {
                GraphicaError::Internal(format!(
                    "Failed to fetch Databricks result chunk: {}",
                    error
                ))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            GraphicaError::Internal(format!(
                "Failed to read Databricks chunk response: {}",
                error
            ))
        })?;

        if !status.is_success() {
            return Err(GraphicaError::Internal(format!(
                "Databricks chunk fetch failed with status {}: {}",
                status, body
            )));
        }

        serde_json::from_str(&body).map_err(|error| {
            GraphicaError::Serialization(format!(
                "Failed to parse Databricks chunk response: {}",
                error
            ))
        })
    }

    fn statement_error_message(envelope: &DatabricksStatementEnvelope, fallback: &str) -> String {
        if let Some(error) = envelope.status.error.as_ref().or(envelope.error.as_ref()) {
            if let Some(code) = &error.error_code {
                return format!("{fallback}: {} ({})", error.message, code);
            }
            return format!("{fallback}: {}", error.message);
        }

        if let Some(message) = &envelope.status.message {
            return format!("{fallback}: {}", message);
        }

        format!("{fallback}: state={}", envelope.status.state)
    }

    fn encode_parameters(
        parameters: HashMap<String, serde_json::Value>,
    ) -> ConnectorResult<Vec<DatabricksStatementParameter>> {
        let mut ordered = BTreeMap::new();
        for (name, value) in parameters {
            ordered.insert(name, value);
        }

        ordered
            .into_iter()
            .map(|(name, value)| {
                if name.trim().is_empty() {
                    return Err(GraphicaError::Configuration(
                        "Databricks statement parameters require non-empty names".to_string(),
                    ));
                }

                Ok(match value {
                    serde_json::Value::Null => DatabricksStatementParameter {
                        name,
                        value: None,
                        type_name: "STRING".to_string(),
                    },
                    serde_json::Value::Bool(value) => DatabricksStatementParameter {
                        name,
                        value: Some(value.to_string()),
                        type_name: "BOOLEAN".to_string(),
                    },
                    serde_json::Value::Number(value) => {
                        let (param_value, type_name) = if value.is_i64() {
                            (value.to_string(), "BIGINT".to_string())
                        } else if value.is_u64() {
                            (value.to_string(), "BIGINT".to_string())
                        } else {
                            (value.to_string(), "DOUBLE".to_string())
                        };

                        DatabricksStatementParameter {
                            name,
                            value: Some(param_value),
                            type_name,
                        }
                    }
                    serde_json::Value::String(value) => DatabricksStatementParameter {
                        name,
                        value: Some(value),
                        type_name: "STRING".to_string(),
                    },
                    other => DatabricksStatementParameter {
                        name,
                        value: Some(other.to_string()),
                        type_name: "STRING".to_string(),
                    },
                })
            })
            .collect()
    }

    fn parse_table_reference(
        table_name: &str,
    ) -> ConnectorResult<(Option<String>, Option<String>, String)> {
        let parts = table_name
            .split('.')
            .map(Self::sanitize_identifier_segment)
            .collect::<ConnectorResult<Vec<_>>>()?;

        match parts.as_slice() {
            [table] => Ok((None, None, table.clone())),
            [schema, table] => Ok((None, Some(schema.clone()), table.clone())),
            [catalog, schema, table] => Ok((Some(catalog.clone()), Some(schema.clone()), table.clone())),
            _ => Err(GraphicaError::Configuration(format!(
                "Invalid Databricks table reference '{}': expected table, schema.table, or catalog.schema.table",
                table_name
            ))),
        }
    }

    pub fn sanitize_identifier_segment(segment: &str) -> ConnectorResult<String> {
        let trimmed = segment.trim().trim_matches('`').trim_matches('"');
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(GraphicaError::Configuration(format!(
                "Invalid Databricks identifier segment '{}': only letters, digits, and underscores are allowed",
                segment
            )));
        }

        Ok(trimmed.to_string())
    }

    pub fn quote_identifier(identifier: &str) -> ConnectorResult<String> {
        let segments = identifier
            .split('.')
            .map(Self::sanitize_identifier_segment)
            .collect::<ConnectorResult<Vec<_>>>()?;
        Ok(segments
            .iter()
            .map(|segment| format!("`{segment}`"))
            .collect::<Vec<_>>()
            .join("."))
    }

    pub fn escape_sql_literal(value: &str) -> String {
        value.replace('\'', "''")
    }

    fn table_key(table_schema: &str, table_name: &str, include_schema_in_name: bool) -> String {
        if include_schema_in_name && !table_schema.is_empty() {
            format!("{table_schema}.{table_name}")
        } else {
            table_name.to_string()
        }
    }

    async fn fetch_primary_keys(
        &self,
        info_schema_prefix: &str,
        schema_name: &str,
        table_filter: Option<&str>,
        include_schema_in_name: bool,
    ) -> ConnectorResult<BTreeMap<String, BTreeSet<String>>> {
        let mut sql = format!(
            "SELECT kcu.table_schema, kcu.table_name, kcu.column_name \
             FROM {info_schema_prefix}information_schema.key_column_usage kcu \
             JOIN {info_schema_prefix}information_schema.table_constraints tc \
               ON tc.constraint_schema = kcu.constraint_schema \
              AND tc.constraint_name = kcu.constraint_name \
              AND tc.table_schema = kcu.table_schema \
              AND tc.table_name = kcu.table_name \
             WHERE tc.constraint_type = 'PRIMARY KEY' \
               AND kcu.table_schema = '{}'",
            Self::escape_sql_literal(schema_name)
        );

        if let Some(table_name) = table_filter {
            sql.push_str(&format!(
                " AND kcu.table_name = '{}'",
                Self::escape_sql_literal(table_name)
            ));
        }

        sql.push_str(" ORDER BY kcu.table_schema, kcu.table_name, kcu.ordinal_position");

        let result = self
            .execute_query(&sql, HashMap::new(), None, DEFAULT_TIMEOUT_SECS)
            .await?;

        let mut primary_keys = BTreeMap::<String, BTreeSet<String>>::new();
        for row in result.rows {
            let Some(row) = row.as_object() else {
                continue;
            };

            let table_schema = row
                .get("table_schema")
                .and_then(|value| value.as_str())
                .unwrap_or(schema_name);
            let table_name = row
                .get("table_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let column_name = row
                .get("column_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            if table_name.is_empty() || column_name.is_empty() {
                continue;
            }

            let table_key = Self::table_key(table_schema, table_name, include_schema_in_name);
            primary_keys
                .entry(table_key)
                .or_default()
                .insert(column_name.to_string());
        }

        Ok(primary_keys)
    }
}

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
                if dbx.warehouse_id.is_none()
                    && DatabricksSqlClient::warehouse_id_from_http_path(&dbx.http_path).is_none()
                {
                    errors.push(
                        "warehouseId must be provided or derivable from httpPath".to_string(),
                    );
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

        let result = async {
            let client = DatabricksSqlClient::from_config(config, &credentials)?;
            client
                .execute_query(
                    "SELECT current_catalog() AS current_catalog, current_schema() AS current_schema, current_user() AS current_user",
                    HashMap::new(),
                    Some(1),
                    15,
                )
                .await
        }
        .await;

        let duration_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(query_result) => {
                let mut metadata = HashMap::from([
                    ("workspaceUrl".to_string(), config.workspace_url.clone()),
                    ("httpPath".to_string(), config.http_path.clone()),
                    (
                        "warehouseId".to_string(),
                        config
                            .warehouse_id
                            .clone()
                            .or_else(|| {
                                DatabricksSqlClient::warehouse_id_from_http_path(&config.http_path)
                            })
                            .unwrap_or_default(),
                    ),
                ]);

                if let Some(row) = query_result.rows.first().and_then(|row| row.as_object()) {
                    if let Some(value) = row.get("current_catalog").and_then(|value| value.as_str())
                    {
                        metadata.insert("catalog".to_string(), value.to_string());
                    }
                    if let Some(value) = row.get("current_schema").and_then(|value| value.as_str())
                    {
                        metadata.insert("schema".to_string(), value.to_string());
                    }
                    if let Some(value) = row.get("current_user").and_then(|value| value.as_str()) {
                        metadata.insert("user".to_string(), value.to_string());
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
            Err(error) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(error.to_string()),
                metadata: HashMap::from([
                    ("workspaceUrl".to_string(), config.workspace_url.clone()),
                    ("httpPath".to_string(), config.http_path.clone()),
                ]),
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
        let client = DatabricksSqlClient::from_config(config, &credentials)?;

        let (override_catalog, override_schema, table_filter) = match table_name {
            Some(table_name) => {
                let (catalog, schema, table) =
                    DatabricksSqlClient::parse_table_reference(table_name)?;
                (catalog, schema, Some(table))
            }
            None => (None, None, None),
        };

        let schema_name = override_schema
            .clone()
            .or_else(|| config.schema.clone())
            .unwrap_or_else(|| "default".to_string());
        let info_schema_prefix = override_catalog
            .as_deref()
            .map(DatabricksSqlClient::quote_identifier)
            .transpose()?
            .map(|catalog| format!("{catalog}."))
            .unwrap_or_default();

        let mut sql = format!(
            "SELECT table_schema, table_name, column_name, data_type, is_nullable, ordinal_position, column_default \
             FROM {info_schema_prefix}information_schema.columns \
             WHERE table_schema = '{}'",
            DatabricksSqlClient::escape_sql_literal(&schema_name)
        );

        if let Some(table_name) = &table_filter {
            sql.push_str(&format!(
                " AND table_name = '{}'",
                DatabricksSqlClient::escape_sql_literal(table_name)
            ));
        }

        sql.push_str(" ORDER BY table_schema, table_name, ordinal_position");

        let result = client
            .execute_query(&sql, HashMap::new(), None, DEFAULT_TIMEOUT_SECS)
            .await?;

        let include_schema_in_name = config.schema.is_none() || override_schema.is_some();
        let primary_keys = client
            .fetch_primary_keys(
                &info_schema_prefix,
                &schema_name,
                table_filter.as_deref(),
                include_schema_in_name,
            )
            .await
            .unwrap_or_default();
        let mut tables = BTreeMap::<String, Vec<ColumnDefinition>>::new();

        for row in result.rows {
            let Some(row) = row.as_object() else {
                continue;
            };

            let table_schema = row
                .get("table_schema")
                .and_then(|value| value.as_str())
                .unwrap_or(&schema_name);
            let table = row
                .get("table_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if table.is_empty() {
                continue;
            }

            let table_key =
                DatabricksSqlClient::table_key(table_schema, table, include_schema_in_name);

            let column_name = row
                .get("column_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            if column_name.is_empty() {
                continue;
            }

            let data_type = row
                .get("data_type")
                .and_then(|value| value.as_str())
                .unwrap_or("STRING");
            let nullable = row
                .get("is_nullable")
                .and_then(|value| value.as_str())
                .map(|value| value.eq_ignore_ascii_case("YES"))
                .unwrap_or(true);
            let default_value = row
                .get("column_default")
                .and_then(|value| value.as_str())
                .map(str::to_string);

            let is_primary_key = primary_keys
                .get(&table_key)
                .map(|columns| columns.contains(&column_name))
                .unwrap_or(false);

            tables.entry(table_key).or_default().push(ColumnDefinition {
                name: column_name,
                data_type: DatabricksSqlClient::map_databricks_type(data_type),
                nullable,
                primary_key: is_primary_key,
                default_value,
                semantic_type: None,
                statistics: None,
            });
        }

        Ok(SchemaDefinition {
            name: override_catalog
                .or_else(|| config.catalog.clone())
                .map(|catalog| format!("{catalog}.{schema_name}"))
                .unwrap_or_else(|| schema_name.clone()),
            tables: tables
                .into_iter()
                .map(|(name, columns)| TableDefinition {
                    name,
                    columns,
                    estimated_rows: None,
                })
                .collect(),
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
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let config = Self::extract_config(source)?;
        let client = DatabricksSqlClient::from_config(config, &credentials)?;
        client
            .execute_query(query, parameters, limit, timeout_secs)
            .await
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: true,
            schema_inference: true,
            query_timeout: true,
            streaming: false,
            transactions: false,
            max_batch_size: Some(50_000),
        }
    }
}

#[derive(Debug, Clone)]
struct DatabricksStatementExecution {
    columns: Vec<DatabricksColumnMetadata>,
    rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
struct DatabricksStatementRequest {
    statement: String,
    warehouse_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    disposition: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parameters: Vec<DatabricksStatementParameter>,
}

#[derive(Debug, Serialize)]
struct DatabricksStatementParameter {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(rename = "type")]
    type_name: String,
}

#[derive(Debug, Deserialize)]
struct DatabricksStatementEnvelope {
    statement_id: String,
    status: DatabricksStatementStatus,
    #[serde(default)]
    manifest: Option<DatabricksStatementManifest>,
    #[serde(default)]
    result: Option<DatabricksStatementChunk>,
    #[serde(default)]
    error: Option<DatabricksStatementError>,
}

#[derive(Debug, Deserialize)]
struct DatabricksStatementStatus {
    state: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<DatabricksStatementError>,
}

#[derive(Debug, Deserialize)]
struct DatabricksStatementManifest {
    #[serde(default)]
    schema: Option<DatabricksStatementSchema>,
}

#[derive(Debug, Deserialize)]
struct DatabricksStatementSchema {
    #[serde(default)]
    columns: Vec<DatabricksColumnMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct DatabricksColumnMetadata {
    name: String,
    #[serde(default, rename = "type_name", alias = "type_text", alias = "type")]
    type_name: String,
    #[serde(default)]
    nullable: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DatabricksStatementChunk {
    #[serde(default)]
    data_array: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    next_chunk_internal_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DatabricksStatementError {
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_warehouse_id_from_http_path() {
        let warehouse_id =
            DatabricksSqlClient::warehouse_id_from_http_path("/sql/1.0/warehouses/abc123def456");
        assert_eq!(warehouse_id.as_deref(), Some("abc123def456"));
    }

    #[test]
    fn apply_limit_only_to_select_queries() {
        let (query, applied) = DatabricksSqlClient::apply_limit("SELECT * FROM users", Some(25));
        assert_eq!(query, "SELECT * FROM users LIMIT 25");
        assert!(applied);

        let (query, applied) =
            DatabricksSqlClient::apply_limit("INSERT INTO users VALUES (1)", Some(25));
        assert_eq!(query, "INSERT INTO users VALUES (1)");
        assert!(!applied);
    }

    #[test]
    fn quote_identifier_supports_three_part_names() {
        let quoted = DatabricksSqlClient::quote_identifier("main.bronze.events").unwrap();
        assert_eq!(quoted, "`main`.`bronze`.`events`");
    }

    #[test]
    fn capabilities_enable_query_and_schema_support() {
        let connector = DatabricksConnector::new();
        let caps = connector.capabilities();
        assert!(caps.parameterized_queries);
        assert!(caps.schema_inference);
        assert!(caps.query_timeout);
    }
}
