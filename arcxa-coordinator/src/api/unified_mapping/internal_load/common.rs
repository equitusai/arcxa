use anyhow::Context;
use graphica_core::catalog::DataSourceCatalog;
use std::collections::HashMap;
use std::sync::Arc;

use crate::common::catalog_credentials::resolve_catalog_credentials;
use crate::mapping::loader::MappingPostgresLoader;
use crate::mapping::multi_source::types::TargetTableConfig;

fn target_table_config<'a>(
    session: &'a crate::mapping::multi_source::UnifiedMappingSession,
    table_name: &str,
) -> anyhow::Result<&'a TargetTableConfig> {
    session
        .target_database
        .tables
        .get(table_name)
        .ok_or_else(|| {
            anyhow::anyhow!("Target table '{}' not found in unified session", table_name)
        })
}

pub fn target_table_key_fields(table_config: &TargetTableConfig) -> Option<Vec<String>> {
    if !table_config.primary_keys.is_empty() {
        return Some(table_config.primary_keys.clone());
    }

    let mut derived = table_config
        .columns
        .values()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();

    if derived.is_empty() {
        None
    } else {
        derived.sort();
        derived.dedup();
        Some(derived)
    }
}

pub fn materialize_target_records(
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    source_rows: &HashMap<String, Vec<crate::mapping::loader::SourceRow>>,
    table_name: &str,
    validate_data: bool,
) -> anyhow::Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    let row_builder = MappingPostgresLoader::with_defaults();
    let table_config = target_table_config(session, table_name)?;

    let max_rows = source_rows.values().map(Vec::len).max().unwrap_or(0);
    let mut records = Vec::new();

    for row_index in 0..max_rows {
        let current_rows = source_rows
            .values()
            .filter_map(|rows| rows.get(row_index).cloned())
            .collect::<Vec<_>>();

        let target_row = row_builder
            .build_target_row(session, &current_rows, table_name)
            .with_context(|| format!("Failed to build target row for table '{}'", table_name))?;

        if validate_data {
            row_builder
                .validate_row(&target_row, table_config)
                .with_context(|| {
                    format!(
                        "Unified row validation failed for target table '{}'",
                        table_name
                    )
                })?;
        }

        let is_empty = target_row
            .values()
            .all(|value| value.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true));
        if is_empty {
            continue;
        }

        let record = target_row
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    value
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        records.push(record);
    }

    Ok(records)
}

pub async fn resolve_target_datasource_from_catalog(
    catalog: Arc<dyn DataSourceCatalog>,
    secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
    datasource_id: &str,
) -> anyhow::Result<(
    graphica_core::catalog::types::DataSource,
    graphica_core::catalog::connector::Credentials,
)> {
    let response = catalog
        .get_source(datasource_id)
        .await
        .with_context(|| format!("Failed to resolve target datasource '{}'", datasource_id))?;
    let credentials = resolve_catalog_credentials(&response.source, secret_store_registry).await?;
    Ok((response.source, credentials))
}
