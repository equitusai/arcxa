use anyhow::Context;
use graphica_core::catalog::connectors::databricks::DatabricksSqlClient;
use graphica_core::catalog::DataSourceCatalog;
use std::collections::HashMap;
use std::sync::Arc;

use crate::common::databricks::ensure_table_via_client;
use crate::mapping::loader::LoadResult as MappingLoadResult;
use crate::mapping::multi_source::types::TargetTableConfig;

use super::common::{
    materialize_target_records, resolve_target_datasource_from_catalog, target_table_key_fields,
};

pub(crate) fn databricks_load_strategy_for_table(
    table_config: &TargetTableConfig,
) -> (crate::etl::traits::LoadMode, Option<Vec<String>>) {
    let key_fields = target_table_key_fields(table_config);
    let mode = if key_fields.is_some() {
        crate::etl::traits::LoadMode::Upsert
    } else {
        crate::etl::traits::LoadMode::Insert
    };

    (mode, key_fields)
}

pub async fn load_unified_session_to_databricks(
    catalog: Arc<dyn DataSourceCatalog>,
    secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    source_rows: &HashMap<String, Vec<crate::mapping::loader::SourceRow>>,
    create_tables: bool,
    validate_data: bool,
    batch_size: usize,
) -> anyhow::Result<MappingLoadResult> {
    if session.target_database.datasource_id.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Unified Databricks load requires target_database.datasource_id"
        ));
    }

    let (target_source, credentials) = resolve_target_datasource_from_catalog(
        catalog,
        secret_store_registry,
        &session.target_database.datasource_id,
    )
    .await?;

    let databricks_config = match &target_source.connection.config {
        graphica_core::catalog::types::SourceConfig::Databricks(config) => config,
        other => {
            return Err(anyhow::anyhow!(
                "Unified Databricks load requires a Databricks target datasource, found {:?}",
                other
            ));
        }
    };

    let client = DatabricksSqlClient::from_config(databricks_config, &credentials)
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to create Databricks SQL client")?;

    let mut rows_processed = 0usize;
    let mut rows_inserted = 0usize;
    let mut errors = Vec::new();

    for (table_name, table_config) in &session.target_database.tables {
        if create_tables {
            ensure_table_via_client(&client, table_config)
                .await
                .with_context(|| format!("Failed to ensure Databricks table '{}'", table_name))?;
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

        let (load_mode, key_fields) = databricks_load_strategy_for_table(table_config);
        let loaded = crate::common::databricks::load_rows_via_client(
            &client,
            table_name,
            records,
            load_mode,
            key_fields.as_deref(),
            Some(batch_size.max(1)),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to load unified table '{}' to Databricks",
                table_name
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
