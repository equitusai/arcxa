use anyhow::{Context, Result};
use graphica_core::catalog::types::DatabricksConfig;
use graphica_core::catalog::Credentials;

use crate::common::databricks::build_loader_connection_string;
use crate::etl::loaders::database::DatabaseLoaderFactory;

use super::common::{batch_size_for_rows, map_load_mode, rows_to_records};

pub async fn load(
    config: &DatabricksConfig,
    credentials: &Credentials,
    table_name: &str,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    mode: &str,
    key_fields: Option<&[String]>,
) -> Result<u64> {
    let connection_string = build_loader_connection_string(config, credentials);
    let loader = DatabaseLoaderFactory::create(
        "databricks",
        &connection_string,
        batch_size_for_rows(rows.len()),
    )
    .await
    .context("Failed to create Databricks loader")?;

    loader
        .load(
            table_name,
            rows_to_records(rows),
            map_load_mode(mode),
            key_fields,
        )
        .await
        .context("Databricks load failed")
}
