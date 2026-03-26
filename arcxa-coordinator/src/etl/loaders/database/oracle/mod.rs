//! Oracle loader implementation backed by ODBC.

pub(crate) mod sql;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

use crate::common::oracle::sanitize_connection_string;

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
        sql::validate_table_name(table_name)?;
        sql::validate_column_names(&columns)?;

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
                    sql::generate_insert_all_sql(table_name, &columns, batch.len())?,
                    build_parameter_batch(batch, &columns),
                ),
                LoadMode::Upsert | LoadMode::Merge => (
                    sql::generate_merge_sql(
                        table_name,
                        &columns,
                        key_fields.unwrap_or(&[]),
                        batch.len(),
                    )?,
                    build_parameter_batch(batch, &columns),
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
) -> Vec<Box<dyn InputParameter>> {
    let mut params = Vec::with_capacity(rows.len() * columns.len());
    for row in rows {
        for column in columns {
            params.push(crate::common::odbc::json_value_to_parameter(
                row.get(column),
            ));
        }
    }
    params
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
}
