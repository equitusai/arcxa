use anyhow::{Context, Result};
use graphica_core::catalog::types::DataSource;
use graphica_core::catalog::Credentials;

use crate::common::oracle::build_catalog_connection_string;
use crate::etl::loaders::database::DatabaseLoaderFactory;

use super::common::{batch_size_for_rows, map_load_mode, rows_to_records};

pub async fn load(
    source: &DataSource,
    table_name: &str,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    mode: &str,
    key_fields: Option<&[String]>,
    credentials: &Credentials,
) -> Result<u64> {
    let config = match &source.connection.config {
        graphica_core::catalog::types::SourceConfig::Oracle(config) => config,
        other => {
            return Err(anyhow::anyhow!(
                "Expected Oracle configuration, found {:?}",
                other
            ))
        }
    };

    let connection_string = build_catalog_connection_string(config, credentials, &source.metadata)
        .context("Failed to build Oracle ODBC connection string")?;

    let loader = DatabaseLoaderFactory::create(
        "oracle",
        &connection_string,
        batch_size_for_rows(rows.len()),
    )
    .await
    .context("Failed to create Oracle loader")?;

    loader
        .load(
            table_name,
            rows_to_records(rows),
            map_load_mode(mode),
            key_fields,
        )
        .await
        .context("Oracle load failed")
}
