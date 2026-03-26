//! Data Loader - Bridge between DataSourceReader and DatabaseLoader
//!
//! Coordinates reading from any data source and loading into any target database.
//!
//! ## Architecture
//!
//! ```text
//! DataSourceReader (CSV, DB, S3)
//!         ↓
//!    DataLoader (this module)
//!         ↓
//! DatabaseLoader (Postgres, Databricks, legacy DB2/Oracle adapters)
//! ```

use crate::common::databricks::{
    build_loader_connection_string, workflow_connection_to_databricks,
};
use crate::common::oracle::build_workflow_connection_string as build_oracle_workflow_connection_string;
use crate::etl::loaders::database::{DatabaseLoader, DatabaseLoaderFactory, LoadMode};
use crate::workflows::domain::{
    DataRow, DataSource, DataSourceReader, DatabaseConnectionConfig, DatabaseType,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Configuration for data loading operations
#[derive(Debug, Clone)]
pub struct LoadConfig {
    /// Target table name
    pub table_name: String,

    /// Load mode (INSERT, UPSERT, REPLACE)
    pub load_mode: LoadMode,

    /// Key fields for UPSERT operations
    pub key_fields: Option<Vec<String>>,

    /// Batch size for loading (number of rows per batch)
    pub batch_size: usize,

    /// Maximum errors before aborting
    pub max_errors: Option<usize>,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            table_name: String::new(),
            load_mode: LoadMode::Insert,
            key_fields: None,
            batch_size: 50000, // Increased from 10K to 50K for better throughput
            max_errors: Some(100),
        }
    }
}

/// Statistics for a data loading operation
#[derive(Debug, Clone, Default)]
pub struct LoadStats {
    /// Total rows read from source
    pub rows_read: u64,

    /// Total rows successfully loaded
    pub rows_loaded: u64,

    /// Total rows failed
    pub rows_failed: u64,

    /// Duration in milliseconds
    pub duration_ms: i64,

    /// Batches processed
    pub batches_processed: usize,
}

/// Data loader that coordinates reading and loading
pub struct DataLoader {
    /// Database connection config from DataSource
    connection_config: DatabaseConnectionConfig,

    /// Database type
    database_type: DatabaseType,

    /// Load configuration
    config: LoadConfig,

    /// Database loader (created on first load)
    loader: Arc<Mutex<Option<Box<dyn DatabaseLoader>>>>,
}

impl DataLoader {
    /// Create a new data loader
    pub fn new(
        database_type: DatabaseType,
        connection_config: DatabaseConnectionConfig,
        config: LoadConfig,
    ) -> Self {
        Self {
            connection_config,
            database_type,
            config,
            loader: Arc::new(Mutex::new(None)),
        }
    }

    /// Load data from a source reader into the target database
    pub async fn load_from_reader(
        &self,
        mut reader: Box<dyn DataSourceReader>,
    ) -> Result<LoadStats> {
        let start = std::time::Instant::now();
        let mut stats = LoadStats::default();

        info!(
            "Starting data load: source={:?} -> table={}",
            reader.source(),
            self.config.table_name
        );

        // Ensure database loader is initialized
        self.ensure_loader_initialized().await?;

        // Read data stream from source
        let mut data_stream = reader.read().await?;
        let mut batch: Vec<Value> = Vec::with_capacity(self.config.batch_size);
        let mut error_count = 0;

        // Process rows in batches
        while let Some(row_result) = data_stream.next().await {
            match row_result {
                Ok(row) => {
                    // Convert DataRow (HashMap<String, JsonValue>) to Value
                    let value = self.data_row_to_value(row);
                    batch.push(value);
                    stats.rows_read += 1;

                    // Load batch when full
                    if batch.len() >= self.config.batch_size {
                        match self.load_batch(&batch).await {
                            Ok(loaded) => {
                                stats.rows_loaded += loaded;
                                stats.batches_processed += 1;
                                debug!(
                                    "Loaded batch {} ({} rows)",
                                    stats.batches_processed, loaded
                                );
                            }
                            Err(e) => {
                                warn!("Batch load failed: {}", e);
                                stats.rows_failed += batch.len() as u64;
                                error_count += batch.len();

                                if let Some(max_errors) = self.config.max_errors {
                                    if error_count >= max_errors {
                                        return Err(anyhow::anyhow!(
                                            "Max errors ({}) exceeded, aborting load",
                                            max_errors
                                        ));
                                    }
                                }
                            }
                        }
                        batch.clear();
                    }
                }
                Err(e) => {
                    warn!("Error reading row: {}", e);
                    stats.rows_failed += 1;
                    error_count += 1;

                    if let Some(max_errors) = self.config.max_errors {
                        if error_count >= max_errors {
                            return Err(anyhow::anyhow!(
                                "Max errors ({}) exceeded during read, aborting",
                                max_errors
                            ));
                        }
                    }
                }
            }
        }

        // Load final partial batch
        if !batch.is_empty() {
            match self.load_batch(&batch).await {
                Ok(loaded) => {
                    stats.rows_loaded += loaded;
                    stats.batches_processed += 1;
                    debug!("Loaded final batch ({} rows)", loaded);
                }
                Err(e) => {
                    warn!("Final batch load failed: {}", e);
                    stats.rows_failed += batch.len() as u64;
                }
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as i64;

        info!(
            "Load complete: read={} loaded={} failed={} duration={}ms",
            stats.rows_read, stats.rows_loaded, stats.rows_failed, stats.duration_ms
        );

        Ok(stats)
    }

    /// Ensure the database loader is initialized
    async fn ensure_loader_initialized(&self) -> Result<()> {
        let mut loader_guard = self.loader.lock().await;

        if loader_guard.is_none() {
            // Build connection string from config
            let connection_string = self
                .build_connection_string()
                .context("Failed to build database connection string")?;

            // Create database loader
            let db_type = match self.database_type {
                DatabaseType::Postgres => "postgresql",
                DatabaseType::DB2 => "db2",
                DatabaseType::Oracle => "oracle",
                DatabaseType::SAPHANA => "saphana",
                DatabaseType::MySQL => "mysql",
                DatabaseType::Snowflake => "snowflake",
                DatabaseType::Databricks => "databricks",
            };

            let loader =
                DatabaseLoaderFactory::create(db_type, &connection_string, self.config.batch_size)
                    .await
                    .context("Failed to create database loader")?;

            *loader_guard = Some(loader);
            info!("Database loader initialized: type={}", db_type);
        }

        Ok(())
    }

    /// Load a batch of records
    async fn load_batch(&self, records: &[Value]) -> Result<u64> {
        let loader_guard = self.loader.lock().await;
        let loader = loader_guard
            .as_ref()
            .context("Database loader not initialized")?;

        loader
            .load(
                &self.config.table_name,
                records.to_vec(),
                self.config.load_mode,
                self.config.key_fields.as_deref(),
            )
            .await
    }

    /// Convert DataRow to JSON Value
    fn data_row_to_value(&self, row: DataRow) -> Value {
        let mut obj = serde_json::Map::new();
        for (key, value) in row {
            obj.insert(key, value);
        }
        Value::Object(obj)
    }

    /// Build database connection string from config
    fn build_connection_string(&self) -> Result<String> {
        match self.database_type {
            DatabaseType::Postgres => Ok(format!(
                    "host={} port={} dbname={} user={} password={}",
                    self.connection_config.host,
                    self.connection_config.port,
                    self.connection_config.database,
                    self.connection_config.username,
                    self.connection_config.password
                )),
            DatabaseType::DB2 => Ok(format!(
                    "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
                    self.connection_config.database,
                    self.connection_config.host,
                    self.connection_config.port,
                    self.connection_config.username,
                    self.connection_config.password
                )),
            DatabaseType::MySQL => Ok(format!(
                    "mysql://{}:{}@{}:{}/{}",
                    self.connection_config.username,
                    self.connection_config.password,
                    self.connection_config.host,
                    self.connection_config.port,
                    self.connection_config.database
                )),
            DatabaseType::Databricks => {
                let (config, credentials) =
                    workflow_connection_to_databricks(&self.connection_config)?;
                Ok(build_loader_connection_string(&config, &credentials))
            }
            DatabaseType::Oracle => build_oracle_workflow_connection_string(&self.connection_config),
            DatabaseType::SAPHANA | DatabaseType::Snowflake => Ok(format!(
                "{}://{}:{}@{}:{}/{}",
                self.database_type.to_string().to_lowercase(),
                self.connection_config.username,
                self.connection_config.password,
                self.connection_config.host,
                self.connection_config.port,
                self.connection_config.database
            )),
        }
    }
}

impl DatabaseType {
    fn to_string(&self) -> &'static str {
        match self {
            DatabaseType::Postgres => "postgres",
            DatabaseType::DB2 => "db2",
            DatabaseType::Oracle => "oracle",
            DatabaseType::SAPHANA => "saphana",
            DatabaseType::MySQL => "mysql",
            DatabaseType::Snowflake => "snowflake",
            DatabaseType::Databricks => "databricks",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_config_default() {
        let config = LoadConfig::default();
        assert_eq!(config.load_mode, LoadMode::Insert);
        assert_eq!(config.batch_size, 50000);
        assert_eq!(config.max_errors, Some(100));
    }

    #[test]
    fn test_build_postgres_connection_string() {
        let config = DatabaseConnectionConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            ssl_mode: None,
            extra_params: HashMap::new(),
        };

        let load_config = LoadConfig {
            table_name: "test_table".to_string(),
            ..Default::default()
        };

        let loader = DataLoader::new(DatabaseType::Postgres, config, load_config);
        let conn_str = loader.build_connection_string().unwrap();

        assert!(conn_str.contains("host=localhost"));
        assert!(conn_str.contains("port=5432"));
        assert!(conn_str.contains("dbname=testdb"));
        assert!(conn_str.contains("user=user"));
    }

    #[test]
    fn test_data_row_to_value() {
        let config = DatabaseConnectionConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            ssl_mode: None,
            extra_params: HashMap::new(),
        };

        let load_config = LoadConfig {
            table_name: "test_table".to_string(),
            ..Default::default()
        };

        let loader = DataLoader::new(DatabaseType::Postgres, config, load_config);

        let mut row: DataRow = HashMap::new();
        row.insert("id".to_string(), json!("123"));
        row.insert("name".to_string(), json!("Alice"));

        let value = loader.data_row_to_value(row);

        assert!(value.is_object());
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("id").unwrap(), &json!("123"));
        assert_eq!(obj.get("name").unwrap(), &json!("Alice"));
    }

    #[test]
    fn test_build_databricks_connection_string() {
        let mut extra_params = HashMap::new();
        extra_params.insert(
            "http_path".to_string(),
            "/sql/1.0/warehouses/abc123".to_string(),
        );
        extra_params.insert("schema".to_string(), "bronze".to_string());

        let config = DatabaseConnectionConfig {
            host: "https://adb-123.azuredatabricks.net".to_string(),
            port: 443,
            database: "main".to_string(),
            username: "svc_arcxa".to_string(),
            password: "token-value".to_string(),
            ssl_mode: Some("require".to_string()),
            extra_params,
        };

        let load_config = LoadConfig {
            table_name: "events".to_string(),
            ..Default::default()
        };

        let loader = DataLoader::new(DatabaseType::Databricks, config, load_config);
        let conn_str = loader.build_connection_string().unwrap();

        assert!(conn_str.contains("workspace_url=https://adb-123.azuredatabricks.net"));
        assert!(conn_str.contains("http_path=/sql/1.0/warehouses/abc123"));
        assert!(conn_str.contains("catalog=main"));
        assert!(conn_str.contains("schema=bronze"));
        assert!(conn_str.contains("token=token-value"));
    }

    #[test]
    fn test_build_oracle_connection_string_accepts_camel_case_service_name() {
        let mut extra_params = HashMap::new();
        extra_params.insert("serviceName".to_string(), "ORCLPDB1".to_string());

        let config = DatabaseConnectionConfig {
            host: "oracle.example.com".to_string(),
            port: 1521,
            database: String::new(),
            username: "etl_user".to_string(),
            password: "secret".to_string(),
            ssl_mode: None,
            extra_params,
        };

        let load_config = LoadConfig {
            table_name: "CUSTOMERS".to_string(),
            ..Default::default()
        };

        let loader = DataLoader::new(DatabaseType::Oracle, config, load_config);
        let conn_str = loader.build_connection_string().unwrap();

        assert!(conn_str.contains("DBQ=//oracle.example.com:1521/ORCLPDB1"));
        assert!(conn_str.contains("UID=etl_user"));
        assert!(conn_str.contains("PWD=secret"));
    }
}
