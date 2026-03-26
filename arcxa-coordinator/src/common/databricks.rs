//! Shared Databricks integration helpers used across workflow and ETL paths.

use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::{
    connector::Credentials, connectors::databricks::DatabricksSqlClient, types::DatabricksConfig,
};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

use crate::etl::traits::LoadMode;
use crate::mapping::multi_source::types::TargetTableConfig;
use crate::workflows::domain::DatabaseConnectionConfig;

const DEFAULT_STATEMENT_BATCH_SIZE: usize = 200;

pub fn workflow_connection_to_databricks(
    connection_config: &DatabaseConnectionConfig,
) -> Result<(DatabricksConfig, Credentials)> {
    let workspace_url = workflow_workspace_url(connection_config)?;
    let http_path = connection_config
        .extra_params
        .get("http_path")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Databricks workflow connection requires extra_params.http_path"))?;

    let catalog = connection_config
        .extra_params
        .get("catalog")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!connection_config.database.trim().is_empty())
                .then(|| connection_config.database.clone())
        });
    let schema = connection_config
        .extra_params
        .get("schema")
        .cloned()
        .filter(|value| !value.trim().is_empty());
    let warehouse_id = connection_config
        .extra_params
        .get("warehouse_id")
        .cloned()
        .filter(|value| !value.trim().is_empty());

    let token = connection_config
        .extra_params
        .get("token")
        .or_else(|| connection_config.extra_params.get("access_token"))
        .cloned()
        .or_else(|| {
            (!connection_config.password.trim().is_empty())
                .then(|| connection_config.password.clone())
        })
        .ok_or_else(|| anyhow!("Databricks workflow connection requires a PAT token"))?;

    let config = DatabricksConfig {
        workspace_url,
        http_path,
        catalog,
        schema,
        warehouse_id,
    };

    let mut credentials = Credentials::new(connection_config.username.clone(), token.clone());
    credentials.additional.insert("token".to_string(), token);

    Ok((config, credentials))
}

pub fn build_loader_connection_string(
    config: &DatabricksConfig,
    credentials: &Credentials,
) -> String {
    let mut parts = Vec::new();
    parts.push(format!("workspace_url={}", config.workspace_url));
    parts.push(format!("http_path={}", config.http_path));

    if let Some(catalog) = config
        .catalog
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("catalog={catalog}"));
    }

    if let Some(schema) = config
        .schema
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("schema={schema}"));
    }

    if let Some(warehouse_id) = config
        .warehouse_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("warehouse_id={warehouse_id}"));
    }

    if !credentials.username.trim().is_empty() {
        parts.push(format!("username={}", credentials.username));
    }

    let token = credentials
        .additional
        .get("token")
        .or_else(|| credentials.additional.get("access_token"))
        .cloned()
        .or_else(|| {
            (!credentials.password.trim().is_empty()).then(|| credentials.password.clone())
        });

    if let Some(token) = token {
        parts.push(format!("token={token}"));
    }

    parts.join(";")
}

pub fn parse_loader_connection_string(
    connection_string: &str,
) -> Result<(DatabricksConfig, Credentials)> {
    let mut values = HashMap::new();

    for part in connection_string.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid Databricks connection string segment: {trimmed}"))?;
        values.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let workspace_url = values
        .remove("workspace_url")
        .or_else(|| values.remove("workspaceurl"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Databricks connection string missing workspace_url"))?;
    let http_path = values
        .remove("http_path")
        .or_else(|| values.remove("httppath"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Databricks connection string missing http_path"))?;

    let catalog = values
        .remove("catalog")
        .or_else(|| values.remove("database"))
        .filter(|value| !value.trim().is_empty());
    let schema = values
        .remove("schema")
        .filter(|value| !value.trim().is_empty());
    let warehouse_id = values
        .remove("warehouse_id")
        .or_else(|| values.remove("warehouseid"))
        .filter(|value| !value.trim().is_empty());
    let username = values.remove("username").unwrap_or_default();
    let token = values
        .remove("token")
        .or_else(|| values.remove("access_token"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Databricks connection string missing token"))?;

    let config = DatabricksConfig {
        workspace_url,
        http_path,
        catalog,
        schema,
        warehouse_id,
    };

    let mut credentials = Credentials::new(username, token.clone());
    credentials.additional.insert("token".to_string(), token);

    Ok((config, credentials))
}

pub fn sanitize_loader_connection_string(connection_string: &str) -> String {
    connection_string
        .split(';')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }

            let key = trimmed
                .split_once('=')
                .map(|(key, _)| key.trim().to_ascii_lowercase())
                .unwrap_or_default();
            if matches!(key.as_str(), "token" | "access_token") {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub async fn load_rows_via_client(
    client: &DatabricksSqlClient,
    table_name: &str,
    rows: Vec<Map<String, Value>>,
    mode: LoadMode,
    key_fields: Option<&[String]>,
    statement_batch_size: Option<usize>,
) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }

    let quoted_table =
        DatabricksSqlClient::quote_identifier(table_name).map_err(|error| anyhow!(error))?;
    let columns = collect_columns(&rows)?;
    let quoted_columns = columns
        .iter()
        .map(|column| DatabricksSqlClient::quote_identifier(column).map_err(|error| anyhow!(error)))
        .collect::<Result<Vec<_>>>()?;

    if matches!(mode, LoadMode::Replace) {
        client
            .execute_command(
                &format!("TRUNCATE TABLE {}", quoted_table),
                HashMap::new(),
                60,
            )
            .await
            .map_err(|error| anyhow!(error))
            .context("Databricks replace pre-truncate failed")?;
    }

    let batch_size = statement_batch_size
        .unwrap_or(DEFAULT_STATEMENT_BATCH_SIZE)
        .max(1)
        .min(DEFAULT_STATEMENT_BATCH_SIZE)
        .min(rows.len());

    let mut loaded = 0u64;
    for batch in rows.chunks(batch_size) {
        let (statement, parameters) = match mode {
            LoadMode::Insert | LoadMode::Append | LoadMode::Replace => {
                build_insert_statement(&quoted_table, &columns, &quoted_columns, batch)?
            }
            LoadMode::Upsert | LoadMode::Merge => {
                build_merge_statement(&quoted_table, &columns, &quoted_columns, batch, key_fields)?
            }
        };

        client
            .execute_command(&statement, parameters, 120)
            .await
            .map_err(|error| anyhow!(error))
            .with_context(|| format!("Databricks {} load failed for table {}", mode, table_name))?;

        loaded += batch.len() as u64;
    }

    Ok(loaded)
}

pub async fn ensure_table_via_client(
    client: &DatabricksSqlClient,
    table_config: &TargetTableConfig,
) -> Result<()> {
    if table_config.columns.is_empty() {
        return Err(anyhow!(
            "Cannot create Databricks table '{}' without columns",
            table_config.name
        ));
    }

    let quoted_table = DatabricksSqlClient::quote_identifier(&table_config.name)
        .map_err(|error| anyhow!(error))?;

    let mut column_defs = Vec::with_capacity(table_config.columns.len());
    for column in table_config.columns.values() {
        let quoted_column =
            DatabricksSqlClient::quote_identifier(&column.name).map_err(|error| anyhow!(error))?;
        let mapped_type = map_target_column_type(&column.data_type);
        let nullable_suffix = if column.nullable { "" } else { " NOT NULL" };
        column_defs.push(format!("{quoted_column} {mapped_type}{nullable_suffix}"));
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        quoted_table,
        column_defs.join(", ")
    );

    client
        .execute_query(&sql, HashMap::new(), None, 60)
        .await
        .map(|_| ())
        .map_err(|error| anyhow!(error))
}

pub fn build_insert_statement(
    quoted_table: &str,
    columns: &[String],
    quoted_columns: &[String],
    rows: &[Map<String, Value>],
) -> Result<(String, HashMap<String, Value>)> {
    let mut parameters = HashMap::new();
    let mut row_sql = Vec::with_capacity(rows.len());

    for (row_index, row) in rows.iter().enumerate() {
        let mut value_sql = Vec::with_capacity(columns.len());
        for column in columns {
            let placeholder = format!("r{}_{}", row_index, column);
            value_sql.push(render_value_placeholder(
                row.get(column),
                &placeholder,
                &mut parameters,
            ));
        }
        row_sql.push(format!("({})", value_sql.join(", ")));
    }

    Ok((
        format!(
            "INSERT INTO {} ({}) VALUES {}",
            quoted_table,
            quoted_columns.join(", "),
            row_sql.join(", ")
        ),
        parameters,
    ))
}

pub fn build_merge_statement(
    quoted_table: &str,
    columns: &[String],
    quoted_columns: &[String],
    rows: &[Map<String, Value>],
    key_fields: Option<&[String]>,
) -> Result<(String, HashMap<String, Value>)> {
    let key_fields = key_fields.map(|fields| fields.to_vec()).unwrap_or_default();
    if key_fields.is_empty() {
        return Err(anyhow!("Databricks upsert requires one or more key fields"));
    }

    let validated_keys = key_fields
        .iter()
        .map(|field| {
            DatabricksSqlClient::sanitize_identifier_segment(field).map_err(|error| anyhow!(error))
        })
        .collect::<Result<Vec<_>>>()?;

    for key in &validated_keys {
        if !columns.contains(key) {
            return Err(anyhow!(
                "Databricks upsert key field '{}' not present in load rows",
                key
            ));
        }
    }

    let mut parameters = HashMap::new();
    let mut selects = Vec::with_capacity(rows.len());
    for (row_index, row) in rows.iter().enumerate() {
        let mut projections = Vec::with_capacity(columns.len());
        for (column_index, column) in columns.iter().enumerate() {
            let alias = &quoted_columns[column_index];
            let placeholder = format!("r{}_{}", row_index, column);
            let value_sql =
                render_value_placeholder(row.get(column), &placeholder, &mut parameters);
            projections.push(format!("{value_sql} AS {alias}"));
        }
        selects.push(format!("SELECT {}", projections.join(", ")));
    }

    let on_clause = validated_keys
        .iter()
        .map(|field| {
            let quoted =
                DatabricksSqlClient::quote_identifier(field).map_err(|error| anyhow!(error))?;
            Ok(format!("target.{quoted} = source.{quoted}"))
        })
        .collect::<Result<Vec<_>>>()?
        .join(" AND ");

    let update_columns = columns
        .iter()
        .filter(|column| !validated_keys.contains(*column))
        .map(|column| {
            let quoted =
                DatabricksSqlClient::quote_identifier(column).map_err(|error| anyhow!(error))?;
            Ok(format!("target.{quoted} = source.{quoted}"))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut statement = format!(
        "MERGE INTO {quoted_table} AS target USING ({}) AS source ON {}",
        selects.join(" UNION ALL "),
        on_clause
    );

    if !update_columns.is_empty() {
        statement.push_str(&format!(
            " WHEN MATCHED THEN UPDATE SET {}",
            update_columns.join(", ")
        ));
    }

    statement.push_str(&format!(
        " WHEN NOT MATCHED THEN INSERT ({}) VALUES ({})",
        quoted_columns.join(", "),
        quoted_columns
            .iter()
            .map(|column| format!("source.{column}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    Ok((statement, parameters))
}

fn collect_columns(rows: &[Map<String, Value>]) -> Result<Vec<String>> {
    let mut ordered = BTreeSet::new();
    for row in rows {
        for key in row.keys() {
            let normalized = DatabricksSqlClient::sanitize_identifier_segment(key)
                .map_err(|error| anyhow!(error))?;
            ordered.insert(normalized);
        }
    }

    if ordered.is_empty() {
        return Err(anyhow!("Databricks load requires at least one column"));
    }

    Ok(ordered.into_iter().collect())
}

fn render_value_placeholder(
    value: Option<&Value>,
    placeholder: &str,
    parameters: &mut HashMap<String, Value>,
) -> String {
    match value {
        None | Some(Value::Null) => "NULL".to_string(),
        Some(value) => {
            parameters.insert(placeholder.to_string(), value.clone());
            format!(":{placeholder}")
        }
    }
}

fn map_target_column_type(data_type: &str) -> &'static str {
    let normalized = data_type.trim().to_ascii_uppercase();

    if normalized.contains("BIGINT") {
        "BIGINT"
    } else if normalized.contains("INT") || normalized.contains("SERIAL") {
        "INT"
    } else if normalized.contains("DOUBLE") || normalized.contains("FLOAT") {
        "DOUBLE"
    } else if normalized.contains("DECIMAL") || normalized.contains("NUMERIC") {
        "DECIMAL(38, 18)"
    } else if normalized.contains("BOOLEAN") || normalized.contains("BOOL") {
        "BOOLEAN"
    } else if normalized.contains("DATE") && !normalized.contains("TIME") {
        "DATE"
    } else if normalized.contains("TIMESTAMP") || normalized.contains("DATETIME") {
        "TIMESTAMP"
    } else {
        "STRING"
    }
}

fn workflow_workspace_url(connection_config: &DatabaseConnectionConfig) -> Result<String> {
    if let Some(workspace_url) = connection_config
        .extra_params
        .get("workspace_url")
        .cloned()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(workspace_url.trim_end_matches('/').to_string());
    }

    let host = connection_config.host.trim();
    if host.is_empty() {
        return Err(anyhow!(
            "Databricks workflow connection requires a host or workspace_url"
        ));
    }

    if host.starts_with("https://") || host.starts_with("http://") {
        Ok(host.trim_end_matches('/').to_string())
    } else {
        Ok(format!("https://{}", host.trim_end_matches('/')))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_connection_maps_to_databricks_config() {
        let mut extra_params = HashMap::new();
        extra_params.insert(
            "http_path".to_string(),
            "/sql/1.0/warehouses/abc123".to_string(),
        );
        extra_params.insert("schema".to_string(), "bronze".to_string());

        let connection = DatabaseConnectionConfig {
            host: "https://adb-123.azuredatabricks.net".to_string(),
            port: 443,
            database: "main".to_string(),
            username: "svc_arcxa".to_string(),
            password: "token-value".to_string(),
            ssl_mode: Some("require".to_string()),
            extra_params,
        };

        let (config, credentials) = workflow_connection_to_databricks(&connection).unwrap();
        assert_eq!(config.workspace_url, "https://adb-123.azuredatabricks.net");
        assert_eq!(config.http_path, "/sql/1.0/warehouses/abc123");
        assert_eq!(config.catalog.as_deref(), Some("main"));
        assert_eq!(config.schema.as_deref(), Some("bronze"));
        assert_eq!(
            credentials.additional.get("token"),
            Some(&"token-value".to_string())
        );
    }

    #[test]
    fn loader_connection_string_round_trip_preserves_databricks_config() {
        let config = DatabricksConfig {
            workspace_url: "https://adb-123.azuredatabricks.net".to_string(),
            http_path: "/sql/1.0/warehouses/abc123".to_string(),
            catalog: Some("main".to_string()),
            schema: Some("silver".to_string()),
            warehouse_id: Some("abc123".to_string()),
        };
        let mut credentials = Credentials::new("svc_arcxa".to_string(), "token-value".to_string());
        credentials
            .additional
            .insert("token".to_string(), "token-value".to_string());

        let connection_string = build_loader_connection_string(&config, &credentials);
        let (parsed_config, parsed_credentials) =
            parse_loader_connection_string(&connection_string).unwrap();

        assert_eq!(parsed_config.workspace_url, config.workspace_url);
        assert_eq!(parsed_config.http_path, config.http_path);
        assert_eq!(parsed_config.catalog, config.catalog);
        assert_eq!(parsed_config.schema, config.schema);
        assert_eq!(parsed_config.warehouse_id, config.warehouse_id);
        assert_eq!(parsed_credentials.username, "svc_arcxa");
        assert_eq!(
            parsed_credentials.additional.get("token"),
            Some(&"token-value".to_string())
        );
    }

    #[test]
    fn build_merge_statement_requires_key_fields() {
        let rows = vec![Map::from_iter([(
            "id".to_string(),
            Value::String("1".to_string()),
        )])];

        let error = build_merge_statement(
            "`main`.`silver`.`users`",
            &[String::from("id")],
            &[String::from("`id`")],
            &rows,
            None,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("requires one or more key fields"));
    }

    #[test]
    fn maps_target_column_types_for_table_creation() {
        assert_eq!(map_target_column_type("VARCHAR(255)"), "STRING");
        assert_eq!(map_target_column_type("INTEGER"), "INT");
        assert_eq!(map_target_column_type("BIGINT"), "BIGINT");
        assert_eq!(map_target_column_type("BOOLEAN"), "BOOLEAN");
        assert_eq!(map_target_column_type("TIMESTAMP"), "TIMESTAMP");
        assert_eq!(map_target_column_type("DECIMAL(18,2)"), "DECIMAL(38, 18)");
    }
}
