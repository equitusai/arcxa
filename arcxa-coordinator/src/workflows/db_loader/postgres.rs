use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::types::PostgreSQLConfig;
use graphica_core::catalog::Credentials;

use crate::etl::loaders::database::DatabaseLoaderFactory;

use super::common::{batch_size_for_rows, map_load_mode, rows_to_records};

pub async fn load(
    config: &PostgreSQLConfig,
    credentials: &Credentials,
    table_name: &str,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    mode: &str,
    key_fields: Option<&[String]>,
) -> Result<u64> {
    if credentials.username.is_empty() || credentials.password.is_empty() {
        return Err(anyhow!(
            "PostgreSQL credentials missing (username/password required)"
        ));
    }

    let mut connection_string = format!(
        "host={} port={} dbname={} user={} password={}",
        config.host, config.port, config.database, credentials.username, credentials.password
    );

    if let Some(ssl_mode) = &config.ssl_mode {
        connection_string.push_str(&format!(" sslmode={}", ssl_mode));
    }

    let load_mode = map_load_mode(mode);
    let effective_mode = match load_mode {
        crate::etl::loaders::database::LoadMode::Upsert
        | crate::etl::loaders::database::LoadMode::Merge => {
            if key_fields.is_some() {
                load_mode
            } else {
                tracing::warn!(
                    "PostgreSQL loader requires key_fields for UPSERT/MERGE; falling back to INSERT"
                );
                crate::etl::loaders::database::LoadMode::Insert
            }
        }
        other => other,
    };

    let loader = DatabaseLoaderFactory::create(
        "postgresql",
        &connection_string,
        batch_size_for_rows(rows.len()),
    )
    .await
    .context("Failed to create PostgreSQL loader")?;

    loader
        .load(
            table_name,
            rows_to_records(rows),
            effective_mode,
            key_fields,
        )
        .await
        .context("PostgreSQL load failed")
}
