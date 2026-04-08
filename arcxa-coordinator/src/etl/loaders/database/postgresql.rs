//! PostgreSQL Loader Implementation
//!
//! High-performance PostgreSQL loading using:
//! - COPY FROM STDIN for maximum throughput (50K-200K rows/sec)
//! - Parameterized INSERT for flexibility
//! - ON CONFLICT for UPSERT operations
//!
//! ## Performance
//!
//! - **COPY mode**: 50K-200K rows/second
//! - **INSERT mode**: 5K-10K rows/second
//! - **UPSERT mode**: 3K-8K rows/second

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::SinkExt;
use graphica_core::catalog::postgres_tls::parse_connection_string_ssl_mode;
use serde_json::Value;
use std::collections::HashMap;

use super::common::{
    flatten_rows_to_params, generate_csv_for_copy, generate_insert_sql, generate_upsert_sql,
    quote_postgres_columns, quote_postgres_table_name,
};
use super::postgres_pool::{
    create_postgres_pool, PostgresConfig, PostgresPool, PostgresPoolConfig,
};
use super::{DatabaseLoader, LoadMode};

/// PostgreSQL database loader with connection pooling
pub struct PostgreSQLLoader {
    /// Database connection pool (allows parallel batch loading)
    pool: PostgresPool,

    /// Connection string (without credentials for logging)
    connection_info: String,

    /// Batch size for operations
    batch_size: usize,
}

impl PostgreSQLLoader {
    /// Create new PostgreSQL loader with connection pooling
    ///
    /// # Arguments
    /// * `connection_string` - PostgreSQL connection string (format: "host=... port=... dbname=... user=... password=...")
    /// * `batch_size` - Batch size for bulk operations
    ///
    /// # Example
    /// ```ignore
    /// let loader = PostgreSQLLoader::new(
    ///     "host=localhost port=5432 dbname=mydb user=postgres password=secret",
    ///     50000
    /// ).await?;
    /// ```
    pub async fn new(connection_string: &str, batch_size: usize) -> Result<Self> {
        // Parse connection string into components
        let postgres_config = Self::parse_connection_string(connection_string)?;

        // Create pool configuration
        let pool_config = PostgresPoolConfig {
            postgres_config: postgres_config.clone(),
            max_size: 10, // 10 connections for parallel batch loading
            ..Default::default()
        };

        // Create connection pool
        let pool = create_postgres_pool(pool_config)
            .await
            .context("Failed to create PostgreSQL connection pool")?;

        // Extract connection info without password
        let connection_info = postgres_config.to_sanitized_string();

        tracing::info!(
            "PostgreSQL loader initialized with pool (max_connections=10, batch_size={}): {}",
            batch_size,
            connection_info
        );

        Ok(Self {
            pool,
            connection_info,
            batch_size,
        })
    }

    /// Parse PostgreSQL connection string into PostgresConfig
    fn parse_connection_string(conn_str: &str) -> Result<PostgresConfig> {
        let mut config = PostgresConfig::default();

        for part in conn_str.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "host" => config.host = value.to_string(),
                    "port" => config.port = value.parse().context("Invalid port number")?,
                    "dbname" => config.database = value.to_string(),
                    "user" => config.username = value.to_string(),
                    "password" => config.password = value.to_string(),
                    _ => {} // Ignore other parameters
                }
            }
        }

        config.ssl_mode = parse_connection_string_ssl_mode(conn_str);

        Ok(config)
    }

    /// Sanitize connection string for logging (remove password)
    fn sanitize_connection_string(conn_str: &str) -> String {
        conn_str
            .split_whitespace()
            .filter(|part| !part.starts_with("password="))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Load using COPY FROM STDIN (fastest method)
    async fn load_with_copy(
        &self,
        table_name: &str,
        columns: &[String],
        rows: &[HashMap<String, Option<String>>],
    ) -> Result<u64> {
        let quoted_table = quote_postgres_table_name(table_name)?;
        let quoted_columns = quote_postgres_columns(columns)?;

        // Build COPY command
        let copy_sql = format!(
            "COPY {} ({}) FROM STDIN WITH (FORMAT CSV, DELIMITER ',', QUOTE '\"', NULL '')",
            quoted_table,
            quoted_columns.join(", ")
        );

        // Generate CSV data
        let csv_data = generate_csv_for_copy(columns, rows, ',', '"', "")?;

        // Get connection from pool
        let client = self
            .pool
            .get()
            .await
            .context("Failed to get connection from pool")?;
        let sink = client
            .copy_in(&copy_sql)
            .await
            .context("Failed to start COPY operation")?;

        // Create a stream with our CSV data
        use futures::stream;
        let mut data_stream = stream::iter(vec![Ok(Bytes::from(csv_data))]);

        // Pin the sink and send all data
        tokio::pin!(sink);

        // Send all data from stream to sink
        sink.as_mut()
            .send_all(&mut data_stream)
            .await
            .context("Failed to send CSV data")?;

        // Close the sink - this completes the COPY operation
        sink.as_mut()
            .close()
            .await
            .context("Failed to close COPY sink")?;

        tracing::debug!(
            "COPY loaded {} rows into {} (batch size: {})",
            rows.len(),
            table_name,
            self.batch_size
        );

        Ok(rows.len() as u64)
    }

    /// Load using INSERT statements
    async fn load_with_insert(
        &self,
        table_name: &str,
        columns: &[String],
        rows: &[HashMap<String, Option<String>>],
    ) -> Result<u64> {
        let sql = generate_insert_sql(table_name, columns, rows.len())?;

        // Flatten row values into parameter list
        let param_values = flatten_rows_to_params(columns, rows);

        // Build parameter references for execute
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        for value in &param_values {
            params.push(value);
        }

        let client = self
            .pool
            .get()
            .await
            .context("Failed to get connection from pool")?;
        let inserted = client
            .execute(&sql, &params)
            .await
            .context("Failed to execute INSERT")?;

        tracing::debug!("INSERT loaded {} rows into {}", inserted, table_name);

        Ok(inserted)
    }

    /// Load using UPSERT (INSERT ... ON CONFLICT UPDATE)
    async fn load_with_upsert(
        &self,
        table_name: &str,
        columns: &[String],
        rows: &[HashMap<String, Option<String>>],
        key_fields: &[String],
    ) -> Result<u64> {
        let sql = generate_upsert_sql(table_name, columns, key_fields, rows.len())?;

        // Flatten row values into parameter list
        let param_values = flatten_rows_to_params(columns, rows);

        // Build parameter references for execute
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        for value in &param_values {
            params.push(value);
        }

        let client = self
            .pool
            .get()
            .await
            .context("Failed to get connection from pool")?;
        let upserted = client
            .execute(&sql, &params)
            .await
            .context("Failed to execute UPSERT")?;

        tracing::debug!(
            "UPSERT loaded {} rows into {} (keys: {:?})",
            upserted,
            table_name,
            key_fields
        );

        Ok(upserted)
    }

    /// Load using REPLACE (TRUNCATE + INSERT)
    async fn load_with_replace(
        &self,
        table_name: &str,
        columns: &[String],
        rows: &[HashMap<String, Option<String>>],
    ) -> Result<u64> {
        let quoted_table = quote_postgres_table_name(table_name)?;
        let quoted_columns = quote_postgres_columns(columns)?;

        // Get connection from pool
        let mut client = self
            .pool
            .get()
            .await
            .context("Failed to get connection from pool")?;

        // Start transaction
        let transaction = client
            .transaction()
            .await
            .context("Failed to start transaction")?;

        // Truncate table
        let truncate_sql = format!("TRUNCATE TABLE {}", quoted_table);
        transaction
            .execute(&truncate_sql, &[])
            .await
            .context("Failed to truncate table")?;

        tracing::debug!("Truncated table {} for REPLACE mode", table_name);

        // Use COPY for bulk insert after truncate
        let copy_sql = format!(
            "COPY {} ({}) FROM STDIN WITH (FORMAT CSV, DELIMITER ',', QUOTE '\"', NULL '')",
            quoted_table,
            quoted_columns.join(", ")
        );

        let csv_data = generate_csv_for_copy(columns, rows, ',', '"', "")?;

        let sink = transaction
            .copy_in(&copy_sql)
            .await
            .context("Failed to start COPY operation")?;

        use futures::stream;
        let mut data_stream = stream::iter(vec![Ok(Bytes::from(csv_data))]);

        tokio::pin!(sink);
        sink.as_mut().send_all(&mut data_stream).await?;
        sink.as_mut().close().await?;

        // Commit transaction
        transaction
            .commit()
            .await
            .context("Failed to commit REPLACE transaction")?;

        tracing::debug!("REPLACE loaded {} rows into {}", rows.len(), table_name);

        Ok(rows.len() as u64)
    }
}

#[async_trait]
impl DatabaseLoader for PostgreSQLLoader {
    async fn load(
        &self,
        table_name: &str,
        records: Vec<Value>,
        mode: LoadMode,
        key_fields: Option<&[String]>,
    ) -> Result<u64> {
        if records.is_empty() {
            return Ok(0);
        }

        // Convert JSON records to row format
        let (columns, rows) = super::records_to_rows(&records)?;

        // Process in batches
        let mut total_loaded = 0u64;
        for batch in rows.chunks(self.batch_size) {
            let batch_loaded = match mode {
                LoadMode::Insert | LoadMode::Append => {
                    // Use COPY for INSERT/APPEND (fastest)
                    // Both Insert and Append just add rows
                    self.load_with_copy(table_name, &columns, batch).await?
                }
                LoadMode::Upsert | LoadMode::Merge => {
                    // Upsert and Merge both require key fields
                    let keys = key_fields.ok_or_else(|| {
                        anyhow::anyhow!("key_fields required for UPSERT/MERGE mode")
                    })?;
                    self.load_with_upsert(table_name, &columns, batch, keys)
                        .await?
                }
                LoadMode::Replace => {
                    // Only truncate on first batch
                    if total_loaded == 0 {
                        self.load_with_replace(table_name, &columns, batch).await?
                    } else {
                        self.load_with_copy(table_name, &columns, batch).await?
                    }
                }
            };

            total_loaded += batch_loaded;
        }

        tracing::info!(
            "PostgreSQL: Loaded {} rows into {} using {} mode",
            total_loaded,
            table_name,
            mode
        );

        Ok(total_loaded)
    }

    async fn test_connection(&self) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .context("Failed to get connection from pool")?;
        client
            .query_one("SELECT 1", &[])
            .await
            .context("PostgreSQL connection test failed")?;
        Ok(())
    }

    fn database_type(&self) -> &'static str {
        "PostgreSQL"
    }

    fn connection_info(&self) -> String {
        self.connection_info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_connection_string() {
        let conn_str = "host=localhost dbname=mydb user=postgres password=secret";
        let sanitized = PostgreSQLLoader::sanitize_connection_string(conn_str);

        assert!(sanitized.contains("host=localhost"));
        assert!(sanitized.contains("dbname=mydb"));
        assert!(!sanitized.contains("password"));
        assert!(!sanitized.contains("secret"));
    }
}
