use anyhow::{Context, Result};
use graphica_core::catalog::DataSourceCatalog;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::common::oracle::build_catalog_connection_string;
use crate::etl::loaders::database::oracle::sql::{
    generate_create_table_sql, resolve_owner_and_table, OracleCreateTableColumn,
};
use crate::etl::loaders::database::{DatabaseLoaderFactory, LoadMode as EtlLoadMode};
use crate::mapping::loader::LoadResult as MappingLoadResult;
use crate::mapping::multi_source::types::TargetTableConfig;

use super::common::{
    materialize_target_records, resolve_target_datasource_from_catalog, target_table_key_fields,
};

fn oracle_load_mode_for_table(
    table_config: &TargetTableConfig,
) -> (&'static str, Option<Vec<String>>) {
    let key_fields = target_table_key_fields(table_config);
    let mode = if key_fields.is_some() {
        "upsert"
    } else {
        "insert"
    };
    (mode, key_fields)
}

fn qualified_oracle_table_name(
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    table_name: &str,
) -> String {
    if table_name.contains('.') || session.target_database.schema.trim().is_empty() {
        table_name.to_string()
    } else {
        format!("{}.{}", session.target_database.schema, table_name)
    }
}

#[cfg(feature = "odbc")]
fn ensure_oracle_table_exists(
    connection_string: &str,
    qualified_table_name: &str,
    table_config: &TargetTableConfig,
) -> Result<()> {
    use odbc_api::Environment;

    let env = Environment::new().context("Failed to create Oracle ODBC environment")?;
    let connection = env
        .connect_with_connection_string(connection_string, Default::default())
        .context("Failed to connect to Oracle via ODBC")?;

    let default_owner =
        current_schema(&connection).context("Failed to determine Oracle current schema")?;
    let (owner, table) =
        resolve_owner_and_table(qualified_table_name, None, default_owner.as_deref())?;

    if oracle_table_exists(&connection, &owner, &table)
        .context("Failed to check Oracle table existence")?
    {
        return Ok(());
    }

    connection
        .set_autocommit(false)
        .context("Failed to disable Oracle autocommit for table creation")?;

    let ddl =
        build_oracle_create_table_sql(qualified_table_name, table_config).with_context(|| {
            format!(
                "Failed to generate CREATE TABLE for Oracle target '{}'",
                qualified_table_name
            )
        })?;

    if let Err(error) = execute_sql(&connection, &ddl) {
        let _ = connection.rollback();
        return Err(error).with_context(|| {
            format!(
                "Failed to create Oracle target table '{}' for unified mapping load",
                qualified_table_name
            )
        });
    }

    connection
        .commit()
        .context("Failed to commit Oracle CREATE TABLE transaction")?;

    Ok(())
}

#[cfg(not(feature = "odbc"))]
fn ensure_oracle_table_exists(
    _connection_string: &str,
    _qualified_table_name: &str,
    _table_config: &TargetTableConfig,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Oracle unified mapping load requires the 'odbc' feature to be enabled"
    ))
}

fn build_oracle_create_table_sql(
    qualified_table_name: &str,
    table_config: &TargetTableConfig,
) -> Result<String> {
    let primary_keys = target_table_key_fields(table_config).unwrap_or_default();
    let mut columns = table_config
        .columns
        .values()
        .map(|column| OracleCreateTableColumn {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable && !column.is_primary_key,
        })
        .collect::<Vec<_>>();

    columns.sort_by(|left, right| left.name.cmp(&right.name));

    generate_create_table_sql(qualified_table_name, &columns, &primary_keys)
}

#[cfg(feature = "odbc")]
fn current_schema(connection: &odbc_api::Connection<'_>) -> Result<Option<String>> {
    use odbc_api::{ColumnDescription, Cursor, ResultSetMetadata};

    let mut cursor = match connection
        .execute(
            "SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM DUAL",
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
        .context("Failed to read Oracle current schema row")?
    {
        let mut buffer = Vec::new();
        if row
            .get_text(1, &mut buffer)
            .context("Failed to read Oracle current schema value")?
        {
            return Ok(Some(String::from_utf8_lossy(&buffer).trim().to_string()));
        }
    }

    Ok(None)
}

#[cfg(feature = "odbc")]
fn oracle_table_exists(
    connection: &odbc_api::Connection<'_>,
    owner: &str,
    table: &str,
) -> Result<bool> {
    use odbc_api::{Cursor, IntoParameter};

    let sql = "SELECT COUNT(*) FROM ALL_TABLES WHERE OWNER = ? AND TABLE_NAME = ?";
    let params = vec![
        Box::new(owner.to_string().into_parameter())
            as Box<dyn odbc_api::parameter::InputParameter>,
        Box::new(table.to_string().into_parameter())
            as Box<dyn odbc_api::parameter::InputParameter>,
    ];

    let mut cursor = match connection
        .execute(sql, params.as_slice(), None)
        .context("Failed to execute Oracle table existence query")?
    {
        Some(cursor) => cursor,
        None => return Ok(false),
    };

    if let Some(mut row) = cursor
        .next_row()
        .context("Failed to read Oracle table existence row")?
    {
        let mut buffer = Vec::new();
        if row
            .get_text(1, &mut buffer)
            .context("Failed to read Oracle table existence count")?
        {
            let count = String::from_utf8_lossy(&buffer)
                .trim()
                .parse::<i64>()
                .context("Failed to parse Oracle table existence count")?;
            return Ok(count > 0);
        }
    }

    Ok(false)
}

#[cfg(feature = "odbc")]
fn execute_sql(connection: &odbc_api::Connection<'_>, sql: &str) -> Result<()> {
    connection
        .execute(sql, (), None)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("Oracle statement execution failed: {:?}", error))
}

pub async fn load_unified_session_to_oracle(
    catalog: Arc<dyn DataSourceCatalog>,
    secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    source_rows: &HashMap<String, Vec<crate::mapping::loader::SourceRow>>,
    create_tables: bool,
    validate_data: bool,
    batch_size: usize,
) -> Result<MappingLoadResult> {
    if session.target_database.datasource_id.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Unified Oracle load requires target_database.datasource_id"
        ));
    }

    let (target_source, credentials) = resolve_target_datasource_from_catalog(
        catalog,
        secret_store_registry,
        &session.target_database.datasource_id,
    )
    .await?;

    let oracle_config = match &target_source.connection.config {
        graphica_core::catalog::types::SourceConfig::Oracle(config) => config,
        other => {
            return Err(anyhow::anyhow!(
                "Unified Oracle load requires an Oracle target datasource, found {:?}",
                other
            ));
        }
    };

    let connection_string =
        build_catalog_connection_string(oracle_config, &credentials, &target_source.metadata)
            .context("Failed to build Oracle ODBC connection string for unified load")?;

    let loader = DatabaseLoaderFactory::create("oracle", &connection_string, batch_size.max(1))
        .await
        .context("Failed to create Oracle loader for unified session")?;

    let mut rows_processed = 0usize;
    let mut rows_inserted = 0usize;
    let mut errors = Vec::new();

    for (table_name, table_config) in &session.target_database.tables {
        let qualified_table_name = qualified_oracle_table_name(session, table_name);

        if create_tables {
            ensure_oracle_table_exists(&connection_string, &qualified_table_name, table_config)
                .with_context(|| {
                    format!(
                        "Failed to ensure Oracle target table '{}' exists",
                        qualified_table_name
                    )
                })?;
        }

        let records =
            match materialize_target_records(session, source_rows, table_name, validate_data) {
                Ok(records) => records,
                Err(error) => {
                    errors.push(error.to_string());
                    continue;
                }
            };

        rows_processed += records.len();
        if records.is_empty() {
            continue;
        }

        let (mode, key_fields) = oracle_load_mode_for_table(table_config);
        let load_mode = match mode {
            "upsert" => EtlLoadMode::Upsert,
            _ => EtlLoadMode::Insert,
        };

        let loaded = loader
            .load(
                &qualified_table_name,
                records.into_iter().map(Value::Object).collect(),
                load_mode,
                key_fields.as_deref(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to load unified table '{}' to Oracle",
                    qualified_table_name
                )
            })?;

        rows_inserted += loaded as usize;
    }

    Ok(MappingLoadResult {
        rows_processed,
        rows_inserted,
        rows_skipped: rows_processed.saturating_sub(rows_inserted),
        errors,
        lineage_graph_uri: format!("http://graphica.io/load/{}", session.id),
    })
}
