//! Databricks loader implementation backed by the shared SQL Statement API client.

use anyhow::{Context, Result};
use async_trait::async_trait;
use graphica_core::catalog::connectors::databricks::DatabricksSqlClient;
use serde_json::Value;

use crate::common::databricks::{
    load_rows_via_client, parse_loader_connection_string, sanitize_loader_connection_string,
};

use super::{DatabaseLoader, LoadMode};

pub struct DatabricksLoader {
    client: DatabricksSqlClient,
    connection_info: String,
    batch_size: usize,
}

impl DatabricksLoader {
    pub async fn new(connection_string: &str, batch_size: usize) -> Result<Self> {
        let (config, credentials) = parse_loader_connection_string(connection_string)?;
        let client = DatabricksSqlClient::from_config(&config, &credentials)
            .map_err(|error| anyhow::anyhow!(error))
            .context("Failed to create Databricks SQL client")?;

        Ok(Self {
            client,
            connection_info: sanitize_loader_connection_string(connection_string),
            batch_size: batch_size.max(1),
        })
    }
}

#[async_trait]
impl DatabaseLoader for DatabricksLoader {
    async fn load(
        &self,
        table_name: &str,
        records: Vec<Value>,
        mode: LoadMode,
        key_fields: Option<&[String]>,
    ) -> Result<u64> {
        let rows = records
            .into_iter()
            .map(|record| match record {
                Value::Object(map) => Ok(map),
                _ => Err(anyhow::anyhow!(
                    "Databricks loader expects records to be JSON objects"
                )),
            })
            .collect::<Result<Vec<_>>>()?;

        load_rows_via_client(
            &self.client,
            table_name,
            rows,
            mode,
            key_fields,
            Some(self.batch_size),
        )
        .await
    }

    async fn test_connection(&self) -> Result<()> {
        self.client
            .execute_query(
                "SELECT current_catalog() AS current_catalog",
                std::collections::HashMap::new(),
                Some(1),
                15,
            )
            .await
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!(error))
    }

    fn database_type(&self) -> &'static str {
        "databricks"
    }

    fn connection_info(&self) -> String {
        self.connection_info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_connection_info() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let loader = runtime
            .block_on(DatabricksLoader::new(
                "workspace_url=https://adb-123.azuredatabricks.net;http_path=/sql/1.0/warehouses/abc123;catalog=main;schema=bronze;token=secret-token",
                500,
            ))
            .unwrap();

        assert_eq!(loader.database_type(), "databricks");
        assert!(!loader.connection_info().contains("secret-token"));
    }
}
