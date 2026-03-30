//! Oracle loader implementation backed by ODBC.

pub(crate) mod sql;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use graphica_core::catalog::types::DataSource;
use graphica_core::catalog::{types::SourceConfig, Credentials};
use graphica_core::core::lineage::row_level::{DatabaseType, RowId};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

use crate::common::oracle::{build_catalog_connection_string, sanitize_connection_string};

use super::{DatabaseLoader, LoadMode};

#[cfg(feature = "odbc")]
use odbc_api::parameter::InputParameter;

pub struct OracleLoader {
    connection_string: String,
    connection_info: String,
    batch_size: usize,
}

impl OracleLoader {
    pub async fn new(connection_string: &str, batch_size: usize) -> Result<Self> {
        if connection_string.trim().is_empty() {
            return Err(anyhow!("Oracle connection string cannot be empty"));
        }

        Ok(Self {
            connection_string: connection_string.to_string(),
            connection_info: sanitize_connection_string(connection_string),
            batch_size: batch_size.max(1),
        })
    }
}

pub async fn query_primary_key_columns(
    source: &DataSource,
    table_name: &str,
    credentials: &Credentials,
) -> Result<Vec<String>> {
    let SourceConfig::Oracle(config) = &source.connection.config else {
        return Err(anyhow!(
            "Oracle primary key lookup requires an Oracle datasource"
        ));
    };

    let connection_string = build_catalog_connection_string(config, credentials, &source.metadata)?;
    let table_name = table_name.to_string();

    tokio::task::spawn_blocking(move || {
        query_primary_key_columns_blocking(&connection_string, &table_name)
    })
    .await
    .map_err(|error| anyhow!("Oracle primary key query task join error: {}", error))?
}

pub fn build_output_row_ids(
    table_name: &str,
    rows: &[Map<String, Value>],
    primary_key_columns: &[String],
) -> Vec<Option<RowId>> {
    rows.iter()
        .map(|row| {
            let mut primary_key = std::collections::BTreeMap::new();
            for column in primary_key_columns {
                let value = row.get(column)?;
                if value.is_null() {
                    return None;
                }
                primary_key.insert(column.clone(), row_value_to_string(value));
            }

            if primary_key.is_empty() {
                return None;
            }

            Some(RowId::database(
                DatabaseType::Oracle,
                table_name.to_string(),
                primary_key,
            ))
        })
        .collect()
}

#[cfg(feature = "odbc")]
#[async_trait]
impl DatabaseLoader for OracleLoader {
    async fn load(
        &self,
        table_name: &str,
        records: Vec<Value>,
        mode: LoadMode,
        key_fields: Option<&[String]>,
    ) -> Result<u64> {
        let connection_string = self.connection_string.clone();
        let table_name = table_name.to_string();
        let key_fields = key_fields.map(|fields| fields.to_vec());
        let batch_size = self.batch_size;

        let rows = records
            .into_iter()
            .map(|record| match record {
                Value::Object(map) => Ok(map),
                _ => Err(anyhow!("Oracle loader expects JSON object records")),
            })
            .collect::<Result<Vec<_>>>()?;

        tokio::task::spawn_blocking(move || {
            load_rows_blocking(
                &connection_string,
                &table_name,
                rows,
                mode,
                key_fields.as_deref(),
                batch_size,
            )
        })
        .await
        .map_err(|error| anyhow!("Oracle loader task join error: {}", error))?
    }

    async fn test_connection(&self) -> Result<()> {
        let connection_string = self.connection_string.clone();
        tokio::task::spawn_blocking(move || test_connection_blocking(&connection_string))
            .await
            .map_err(|error| anyhow!("Oracle loader test task join error: {}", error))?
    }

    fn database_type(&self) -> &'static str {
        "oracle"
    }

    fn connection_info(&self) -> String {
        self.connection_info.clone()
    }
}

#[cfg(not(feature = "odbc"))]
#[async_trait]
impl DatabaseLoader for OracleLoader {
    async fn load(
        &self,
        _table_name: &str,
        _records: Vec<Value>,
        _mode: LoadMode,
        _key_fields: Option<&[String]>,
    ) -> Result<u64> {
        Err(anyhow!(
            "Oracle loading requires the 'odbc' feature to be enabled"
        ))
    }

    async fn test_connection(&self) -> Result<()> {
        Err(anyhow!(
            "Oracle loading requires the 'odbc' feature to be enabled"
        ))
    }

    fn database_type(&self) -> &'static str {
        "oracle"
    }

    fn connection_info(&self) -> String {
        self.connection_info.clone()
    }
}

#[cfg(feature = "odbc")]
fn test_connection_blocking(connection_string: &str) -> Result<()> {
    use odbc_api::{ConnectionOptions, Environment};

    let env = Environment::new().context("Failed to create Oracle ODBC environment")?;
    let connection = env
        .connect_with_connection_string(connection_string, ConnectionOptions::default())
        .context("Failed to connect to Oracle via ODBC")?;

    connection
        .execute("SELECT 1 FROM DUAL", (), None)
        .context("Oracle health check failed")?;

    Ok(())
}

#[cfg(feature = "odbc")]
fn load_rows_blocking(
    connection_string: &str,
    table_name: &str,
    rows: Vec<Map<String, Value>>,
    mode: LoadMode,
    key_fields: Option<&[String]>,
    batch_size: usize,
) -> Result<u64> {
    use odbc_api::parameter::InputParameter;
    use odbc_api::{ConnectionOptions, Environment};

    if rows.is_empty() {
        return Ok(0);
    }

    let env = Environment::new().context("Failed to create Oracle ODBC environment")?;
    let connection = env
        .connect_with_connection_string(connection_string, ConnectionOptions::default())
        .context("Failed to connect to Oracle via ODBC")?;

    connection
        .set_autocommit(false)
        .context("Failed to disable Oracle autocommit")?;
    let load_result = (|| -> Result<u64> {
        let current_schema = query_current_schema(&connection)
            .context("Failed to determine Oracle current schema")?;
        ensure_table_exists(&connection, table_name, current_schema.as_deref())
            .with_context(|| format!("Oracle target table '{}' does not exist", table_name))?;

        let columns = collect_columns(&rows)?;
        let target_column_types =
            query_table_column_types(&connection, table_name, current_schema.as_deref())
                .with_context(|| {
                    format!(
                        "Failed to determine Oracle target column types for '{}'",
                        table_name
                    )
                })?;
        sql::validate_table_name(table_name)?;
        sql::validate_column_names(&columns)?;
        ensure_target_columns_present(table_name, &columns, &target_column_types)?;
        let value_expressions = build_value_expressions(&columns, &target_column_types)?;

        if matches!(mode, LoadMode::Replace) {
            let replace_sql = sql::generate_replace_sql(table_name)?;
            execute_statement(&connection, &replace_sql, Vec::new())
                .context("Failed to clear Oracle target table for REPLACE load")?;
        }

        let effective_mode = if matches!(mode, LoadMode::Upsert | LoadMode::Merge) {
            let keys = key_fields.unwrap_or(&[]);
            if keys.is_empty() {
                return Err(anyhow!("Oracle upsert requires one or more key_fields"));
            }
            LoadMode::Upsert
        } else {
            mode
        };

        let mut loaded = 0u64;
        for batch in rows.chunks(batch_size.max(1)) {
            let (statement, params) = match effective_mode {
                LoadMode::Insert | LoadMode::Append | LoadMode::Replace => (
                    sql::generate_insert_all_sql(
                        table_name,
                        &columns,
                        &value_expressions,
                        batch.len(),
                    )?,
                    build_parameter_batch(batch, &columns, &target_column_types)?,
                ),
                LoadMode::Upsert | LoadMode::Merge => (
                    sql::generate_merge_sql(
                        table_name,
                        &columns,
                        key_fields.unwrap_or(&[]),
                        &value_expressions,
                        batch.len(),
                    )?,
                    build_parameter_batch(batch, &columns, &target_column_types)?,
                ),
            };

            execute_statement(&connection, &statement, params)
                .with_context(|| format!("Oracle load batch failed for table '{}'", table_name))?;
            loaded += batch.len() as u64;
        }

        Ok(loaded)
    })();

    match load_result {
        Ok(loaded) => {
            connection
                .commit()
                .context("Failed to commit Oracle load transaction")?;
            Ok(loaded)
        }
        Err(error) => {
            let rollback_result = connection.rollback();
            if let Err(rollback_error) = rollback_result {
                tracing::warn!(
                    "Oracle rollback failed after load error for table '{}': {:?}",
                    table_name,
                    rollback_error
                );
            }
            Err(error)
        }
    }
}

#[cfg(feature = "odbc")]
fn execute_statement(
    connection: &odbc_api::Connection<'_>,
    statement: &str,
    params: Vec<Box<dyn InputParameter>>,
) -> Result<()> {
    connection
        .execute(statement, params.as_slice(), None)
        .map(|_| ())
        .map_err(|error| anyhow!("Oracle statement execution failed: {:?}", error))
}

#[cfg(feature = "odbc")]
fn build_parameter_batch(
    rows: &[Map<String, Value>],
    columns: &[String],
    target_column_types: &HashMap<String, String>,
) -> Result<Vec<Box<dyn InputParameter>>> {
    let mut params = Vec::with_capacity(rows.len() * columns.len());
    for row in rows {
        for column in columns {
            let target_column_type = target_column_types
                .get(&column.to_ascii_uppercase())
                .map(String::as_str);
            params.push(
                json_value_to_oracle_parameter(row.get(column), target_column_type).with_context(
                    || format!("Failed to build Oracle parameter for '{}'", column),
                )?,
            );
        }
    }
    Ok(params)
}

#[cfg(feature = "odbc")]
fn json_value_to_oracle_parameter(
    value: Option<&Value>,
    target_column_type: Option<&str>,
) -> Result<Box<dyn InputParameter>> {
    use odbc_api::IntoParameter;

    let Some(target_column_type) = target_column_type else {
        return Ok(crate::common::odbc::json_value_to_parameter(value));
    };

    if target_column_type.starts_with("TIMESTAMP WITH TIME ZONE") {
        let normalized =
            normalize_oracle_temporal_value(value, TemporalTargetType::TimestampWithTimeZone)?;
        return Ok(Box::new(normalized.into_parameter()));
    }

    if target_column_type.starts_with("TIMESTAMP") {
        let normalized = normalize_oracle_temporal_value(value, TemporalTargetType::Timestamp)?;
        return Ok(Box::new(normalized.into_parameter()));
    }

    if target_column_type == "DATE" {
        let normalized = normalize_oracle_temporal_value(value, TemporalTargetType::Date)?;
        return Ok(Box::new(normalized.into_parameter()));
    }

    Ok(crate::common::odbc::json_value_to_parameter(value))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemporalTargetType {
    Date,
    Timestamp,
    TimestampWithTimeZone,
}

fn normalize_oracle_temporal_value(
    value: Option<&Value>,
    target: TemporalTargetType,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };

    match value {
        Value::Null => Ok(None),
        Value::String(text) => normalize_oracle_temporal_text(text, target),
        other => Err(anyhow!(
            "Oracle {:?} column expected a string-compatible temporal value, found {}",
            target,
            other
        )),
    }
}

fn normalize_oracle_temporal_text(
    text: &str,
    target: TemporalTargetType,
) -> Result<Option<String>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    match target {
        TemporalTargetType::TimestampWithTimeZone => normalize_oracle_timestamp_tz_text(trimmed),
        TemporalTargetType::Timestamp => normalize_oracle_timestamp_text(trimmed),
        TemporalTargetType::Date => normalize_oracle_date_text(trimmed),
    }
}

fn normalize_oracle_timestamp_text(text: &str) -> Result<Option<String>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(
            parsed
                .with_timezone(&Utc)
                .naive_utc()
                .format("%Y-%m-%d %H:%M:%S%.6f")
                .to_string(),
        ));
    }

    if let Some(parsed) = parse_naive_datetime(text) {
        return Ok(Some(parsed.format("%Y-%m-%d %H:%M:%S%.6f").to_string()));
    }

    if let Some(parsed) = parse_naive_date(text) {
        return Ok(Some(
            parsed
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid")
                .format("%Y-%m-%d %H:%M:%S%.6f")
                .to_string(),
        ));
    }

    Err(anyhow!(
        "Unsupported Oracle TIMESTAMP value '{}'; expected RFC3339, YYYY-MM-DD HH:MM:SS[.fraction], or YYYY-MM-DD",
        text
    ))
}

fn normalize_oracle_timestamp_tz_text(text: &str) -> Result<Option<String>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(parsed.format("%Y-%m-%d %H:%M:%S%.6f %:z").to_string()));
    }

    for pattern in [
        "%Y-%m-%d %H:%M:%S%.f %:z",
        "%Y-%m-%dT%H:%M:%S%.f %:z",
        "%Y-%m-%d %H:%M:%S %:z",
        "%Y-%m-%dT%H:%M:%S %:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%:z",
    ] {
        if let Ok(parsed) = DateTime::parse_from_str(text, pattern) {
            return Ok(Some(parsed.format("%Y-%m-%d %H:%M:%S%.6f %:z").to_string()));
        }
    }

    Err(anyhow!(
        "Unsupported Oracle TIMESTAMP WITH TIME ZONE value '{}'; expected an offset-aware timestamp such as RFC3339",
        text
    ))
}

fn normalize_oracle_date_text(text: &str) -> Result<Option<String>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(
            parsed
                .with_timezone(&Utc)
                .naive_utc()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ));
    }

    if let Some(parsed) = parse_naive_datetime(text) {
        return Ok(Some(parsed.format("%Y-%m-%d %H:%M:%S").to_string()));
    }

    if let Some(parsed) = parse_naive_date(text) {
        return Ok(Some(
            parsed
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid")
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ));
    }

    Err(anyhow!(
        "Unsupported Oracle DATE value '{}'; expected RFC3339, YYYY-MM-DD HH:MM:SS[.fraction], or YYYY-MM-DD",
        text
    ))
}

fn parse_naive_datetime(text: &str) -> Option<NaiveDateTime> {
    [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ]
    .into_iter()
    .find_map(|pattern| NaiveDateTime::parse_from_str(text, pattern).ok())
}

fn parse_naive_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

#[cfg(feature = "odbc")]
fn collect_columns(rows: &[Map<String, Value>]) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();

    for row in rows {
        for key in row.keys() {
            if seen.insert(key.clone()) {
                ordered.push(key.clone());
            }
        }
    }

    if ordered.is_empty() {
        return Err(anyhow!("Oracle load requires at least one column"));
    }

    Ok(ordered)
}

#[cfg(feature = "odbc")]
fn ensure_target_columns_present(
    table_name: &str,
    columns: &[String],
    target_column_types: &HashMap<String, String>,
) -> Result<()> {
    let missing_columns = columns
        .iter()
        .filter(|column| !target_column_types.contains_key(&column.to_ascii_uppercase()))
        .cloned()
        .collect::<Vec<_>>();

    if missing_columns.is_empty() {
        return Ok(());
    }

    Err(anyhow!(
        "Oracle target table '{}' is missing load columns: {}",
        table_name,
        missing_columns.join(", ")
    ))
}

#[cfg(feature = "odbc")]
fn build_value_expressions(
    columns: &[String],
    target_column_types: &HashMap<String, String>,
) -> Result<Vec<String>> {
    columns
        .iter()
        .map(|column| {
            let normalized_column = column.to_ascii_uppercase();
            let data_type = target_column_types.get(&normalized_column).ok_or_else(|| {
                anyhow!(
                    "Oracle target column '{}' was not found in target schema",
                    column
                )
            })?;

            let expression = if data_type.starts_with("TIMESTAMP WITH TIME ZONE") {
                "TO_TIMESTAMP_TZ(?, 'YYYY-MM-DD HH24:MI:SS.FF6 TZH:TZM')"
            } else if data_type.starts_with("TIMESTAMP") {
                "TO_TIMESTAMP(?, 'YYYY-MM-DD HH24:MI:SS.FF6')"
            } else if data_type == "DATE" {
                "TO_DATE(?, 'YYYY-MM-DD HH24:MI:SS')"
            } else {
                "?"
            };

            Ok(expression.to_string())
        })
        .collect()
}

#[cfg(feature = "odbc")]
fn query_current_schema(connection: &odbc_api::Connection<'_>) -> Result<Option<String>> {
    use odbc_api::{ColumnDescription, Cursor, ResultSetMetadata};

    let mut cursor = match connection
        .execute(
            "SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AS CURRENT_SCHEMA FROM DUAL",
            (),
            None,
        )
        .context("Failed to query Oracle current schema")?
    {
        Some(cursor) => cursor,
        None => return Ok(None),
    };

    let mut description = ColumnDescription::default();
    let _ = cursor.describe_col(1, &mut description);

    if let Some(mut row) = cursor
        .next_row()
        .context("Failed to read current schema row")?
    {
        let mut buffer = Vec::new();
        let not_null = row
            .get_text(1, &mut buffer)
            .context("Failed to read current schema value")?;
        if not_null {
            return Ok(Some(String::from_utf8_lossy(&buffer).trim().to_string()));
        }
    }

    Ok(None)
}

#[cfg(feature = "odbc")]
fn ensure_table_exists(
    connection: &odbc_api::Connection<'_>,
    table_name: &str,
    current_schema: Option<&str>,
) -> Result<()> {
    use odbc_api::{Cursor, IntoParameter};

    let (owner, table) = sql::resolve_owner_and_table(table_name, current_schema, current_schema)?;
    let query = "SELECT 1 FROM ALL_TABLES WHERE OWNER = ? AND TABLE_NAME = ?";
    let params = vec![
        Box::new(owner.into_parameter()) as Box<dyn InputParameter>,
        Box::new(table.into_parameter()) as Box<dyn InputParameter>,
    ];

    let mut cursor = match connection
        .execute(query, params.as_slice(), None)
        .context("Failed to query Oracle table existence")?
    {
        Some(cursor) => cursor,
        None => {
            return Err(anyhow!(
                "Oracle table existence query returned no result set"
            ))
        }
    };

    if cursor
        .next_row()
        .context("Failed to read Oracle table existence row")?
        .is_none()
    {
        return Err(anyhow!("Table not found"));
    }

    Ok(())
}

#[cfg(feature = "odbc")]
fn query_table_column_types(
    connection: &odbc_api::Connection<'_>,
    table_name: &str,
    current_schema: Option<&str>,
) -> Result<HashMap<String, String>> {
    use odbc_api::{Cursor, IntoParameter};

    let (owner, table) = sql::resolve_owner_and_table(table_name, current_schema, current_schema)?;
    let query =
        "SELECT COLUMN_NAME, DATA_TYPE FROM ALL_TAB_COLUMNS WHERE OWNER = ? AND TABLE_NAME = ? ORDER BY COLUMN_ID";
    let params = vec![
        Box::new(owner.into_parameter()) as Box<dyn InputParameter>,
        Box::new(table.into_parameter()) as Box<dyn InputParameter>,
    ];

    let mut cursor = match connection
        .execute(query, params.as_slice(), None)
        .context("Failed to query Oracle target column types")?
    {
        Some(cursor) => cursor,
        None => {
            return Err(anyhow!(
                "Oracle target column type query returned no result set"
            ))
        }
    };

    let mut column_types = HashMap::new();
    while let Some(mut row) = cursor
        .next_row()
        .context("Failed to read Oracle target column type row")?
    {
        let mut column_buffer = Vec::new();
        let mut type_buffer = Vec::new();

        let has_column = row
            .get_text(1, &mut column_buffer)
            .context("Failed to read Oracle target column name")?;
        let has_type = row
            .get_text(2, &mut type_buffer)
            .context("Failed to read Oracle target column type")?;

        if has_column && has_type {
            let column_name = String::from_utf8_lossy(&column_buffer)
                .trim()
                .to_ascii_uppercase();
            let data_type = String::from_utf8_lossy(&type_buffer)
                .trim()
                .to_ascii_uppercase();
            column_types.insert(column_name, data_type);
        }
    }

    Ok(column_types)
}

#[cfg(feature = "odbc")]
fn query_primary_key_columns_blocking(
    connection_string: &str,
    table_name: &str,
) -> Result<Vec<String>> {
    use odbc_api::{ConnectionOptions, Cursor, IntoParameter};

    let env = odbc_api::Environment::new().context("Failed to create Oracle ODBC environment")?;
    let connection = env
        .connect_with_connection_string(connection_string, ConnectionOptions::default())
        .context("Failed to connect to Oracle via ODBC")?;

    let current_schema =
        query_current_schema(&connection).context("Failed to determine Oracle current schema")?;
    let (owner, table) = sql::resolve_owner_and_table(
        table_name,
        current_schema.as_deref(),
        current_schema.as_deref(),
    )?;
    let query = r#"
        SELECT cols.COLUMN_NAME
        FROM ALL_CONSTRAINTS cons
        JOIN ALL_CONS_COLUMNS cols
          ON cons.OWNER = cols.OWNER
         AND cons.CONSTRAINT_NAME = cols.CONSTRAINT_NAME
        WHERE cons.CONSTRAINT_TYPE = 'P'
          AND cons.OWNER = ?
          AND cons.TABLE_NAME = ?
        ORDER BY cols.POSITION
    "#;
    let params = vec![
        Box::new(owner.into_parameter()) as Box<dyn InputParameter>,
        Box::new(table.into_parameter()) as Box<dyn InputParameter>,
    ];

    let mut cursor = match connection
        .execute(query, params.as_slice(), None)
        .context("Failed to query Oracle primary key columns")?
    {
        Some(cursor) => cursor,
        None => return Ok(Vec::new()),
    };

    let mut primary_key_columns = Vec::new();
    while let Some(mut row) = cursor
        .next_row()
        .context("Failed to read Oracle primary key column row")?
    {
        let mut column_buffer = Vec::new();
        let has_column = row
            .get_text(1, &mut column_buffer)
            .context("Failed to read Oracle primary key column name")?;
        if has_column {
            primary_key_columns.push(String::from_utf8_lossy(&column_buffer).trim().to_string());
        }
    }

    Ok(primary_key_columns)
}

#[cfg(not(feature = "odbc"))]
fn query_primary_key_columns_blocking(
    _connection_string: &str,
    _table_name: &str,
) -> Result<Vec<String>> {
    Err(anyhow!(
        "Oracle primary key lookup requires the odbc feature to be enabled"
    ))
}

fn row_value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sanitizes_connection_info() {
        let loader = OracleLoader::new(
            "DRIVER={Oracle in OraClient19Home1};DBQ=//oracle.example.com:1521/ORCL;UID=svc_arcxa;PWD=secret;",
            500,
        )
        .await
        .unwrap();

        assert_eq!(loader.database_type(), "oracle");
        assert!(loader.connection_info().contains("PWD=***"));
        assert!(!loader.connection_info().contains("secret"));
    }

    #[test]
    fn builds_oracle_value_expressions_for_temporal_columns() {
        let columns = vec![
            "CUSTOMER_CODE".to_string(),
            "UPDATED_AT".to_string(),
            "CREATED_ON".to_string(),
        ];
        let target_column_types = HashMap::from([
            ("CUSTOMER_CODE".to_string(), "VARCHAR2".to_string()),
            ("UPDATED_AT".to_string(), "TIMESTAMP".to_string()),
            ("CREATED_ON".to_string(), "DATE".to_string()),
        ]);

        let expressions = build_value_expressions(&columns, &target_column_types).unwrap();

        assert_eq!(
            expressions,
            vec![
                "?".to_string(),
                "TO_TIMESTAMP(?, 'YYYY-MM-DD HH24:MI:SS.FF6')".to_string(),
                "TO_DATE(?, 'YYYY-MM-DD HH24:MI:SS')".to_string(),
            ]
        );
    }

    #[test]
    fn builds_output_row_ids_from_oracle_primary_key_columns() {
        let rows = vec![
            Map::from_iter([
                (
                    "CUSTOMER_CODE".to_string(),
                    Value::String("CUST001".to_string()),
                ),
                (
                    "EMAIL".to_string(),
                    Value::String("alice@example.com".to_string()),
                ),
            ]),
            Map::from_iter([
                (
                    "CUSTOMER_CODE".to_string(),
                    Value::String("CUST002".to_string()),
                ),
                (
                    "EMAIL".to_string(),
                    Value::String("bob@example.com".to_string()),
                ),
            ]),
        ];

        let row_ids = build_output_row_ids("CUSTOMER_DIM", &rows, &["CUSTOMER_CODE".to_string()]);

        assert_eq!(row_ids.len(), 2);
        assert_eq!(
            row_ids[0].as_ref().map(RowId::to_key),
            Some("oracle:CUSTOMER_DIM:CUSTOMER_CODE=CUST001".to_string())
        );
        assert_eq!(
            row_ids[1].as_ref().map(RowId::to_key),
            Some("oracle:CUSTOMER_DIM:CUSTOMER_CODE=CUST002".to_string())
        );
    }

    #[test]
    fn normalizes_iso_timestamp_text_for_oracle_binding() {
        let normalized =
            normalize_oracle_temporal_text("2026-03-20T08:00:00", TemporalTargetType::Timestamp)
                .unwrap();

        assert_eq!(normalized, Some("2026-03-20 08:00:00.000000".to_string()));
    }

    #[test]
    fn normalizes_date_only_text_for_oracle_date_binding() {
        let normalized =
            normalize_oracle_temporal_text("2026-03-20", TemporalTargetType::Date).unwrap();

        assert_eq!(normalized, Some("2026-03-20 00:00:00".to_string()));
    }

    #[test]
    fn rejects_invalid_timestamp_text_for_oracle_binding() {
        let error = normalize_oracle_temporal_text("March 20, 2026", TemporalTargetType::Timestamp)
            .expect_err("invalid Oracle timestamp input should be rejected");

        assert!(error
            .to_string()
            .contains("Unsupported Oracle TIMESTAMP value"));
    }
}
