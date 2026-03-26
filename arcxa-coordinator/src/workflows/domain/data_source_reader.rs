//! Data Source Reader Trait
//!
//! Unified interface for reading data from various sources (CSV, Database, S3).
//!
//! ## Example
//!
//! ```rust,no_run
//! use graphica_coordinator::workflows::domain::{CsvFileReader, DataSource, DataSourceReader};
//! use futures::StreamExt;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let data_source = DataSource::CsvFile {
//!     file_id: "data001".to_string(),
//!     file_path: PathBuf::from("/data/customers.csv"),
//!     encoding: None,
//!     delimiter: None,
//!     has_header: true,
//! };
//! let mut reader = CsvFileReader::new(data_source)?;
//! let mut stream = reader.read().await?;
//!
//! while let Some(row) = stream.next().await {
//!     let row = row?;
//!     println!("Row: {:?}", row);
//! }
//! # Ok(())
//! # }
//! ```

use super::{DataSource, DatabaseConnectionConfig, DatabaseType};
use crate::common::databricks::{
    build_loader_connection_string, workflow_connection_to_databricks,
};
use crate::common::oracle::build_workflow_connection_string as build_oracle_workflow_connection_string;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::{stream::Stream, StreamExt, TryStreamExt};
use graphica_core::catalog::connectors::databricks::DatabricksSqlClient;
use graphica_core::catalog::postgres_tls::connect_postgres_client;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::pin::Pin;
use tokio::fs::File;
use tokio::io::AsyncBufReadExt;
use tokio_postgres::types::ToSql;

/// Row of data read from a source
pub type DataRow = HashMap<String, JsonValue>;

/// Stream of data rows
pub type DataStream = Pin<Box<dyn Stream<Item = Result<DataRow>> + Send>>;

/// Metadata about the data source
#[derive(Debug, Clone)]
pub struct SourceMetadata {
    /// Estimated total rows (if available)
    pub estimated_rows: Option<usize>,

    /// Column names
    pub columns: Vec<String>,

    /// Source identifier
    pub source_identifier: String,

    /// Additional metadata
    pub extra: HashMap<String, String>,
}

/// Trait for reading data from various sources
#[async_trait]
pub trait DataSourceReader: Send + Sync {
    /// Get metadata about the source
    async fn metadata(&self) -> Result<SourceMetadata>;

    /// Read data as a stream of rows
    async fn read(&mut self) -> Result<DataStream>;

    /// Get the data source this reader is reading from
    fn source(&self) -> &DataSource;
}

// === CSV File Reader ===

/// Reader for CSV files
pub struct CsvFileReader {
    source: DataSource,
}

impl CsvFileReader {
    /// Create a new CSV file reader
    pub fn new(source: DataSource) -> Result<Self> {
        match &source {
            DataSource::CsvFile { .. } => Ok(Self { source }),
            _ => Err(anyhow!("CsvFileReader requires DataSource::CsvFile")),
        }
    }
}

#[async_trait]
impl DataSourceReader for CsvFileReader {
    async fn metadata(&self) -> Result<SourceMetadata> {
        if let DataSource::CsvFile {
            file_id,
            file_path,
            has_header,
            ..
        } = &self.source
        {
            // Read first line to get column names
            let file = File::open(file_path)
                .await
                .with_context(|| format!("Failed to open CSV file: {:?}", file_path))?;

            let mut reader = tokio::io::BufReader::new(file);
            let mut first_line = String::new();
            reader.read_line(&mut first_line).await?;

            let columns = if *has_header {
                first_line
                    .trim()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            } else {
                // Generate column names: col_0, col_1, etc.
                let col_count = first_line.split(',').count();
                (0..col_count).map(|i| format!("col_{}", i)).collect()
            };

            Ok(SourceMetadata {
                estimated_rows: None, // Could count lines, but expensive
                columns,
                source_identifier: file_id.clone(),
                extra: HashMap::new(),
            })
        } else {
            Err(anyhow!("Invalid source type for CsvFileReader"))
        }
    }

    async fn read(&mut self) -> Result<DataStream> {
        if let DataSource::CsvFile {
            file_path,
            delimiter,
            has_header,
            ..
        } = &self.source
        {
            let delimiter = delimiter.unwrap_or(',');
            let has_header = *has_header;

            let file = File::open(file_path)
                .await
                .with_context(|| format!("Failed to open CSV file: {:?}", file_path))?;

            let reader = tokio::io::BufReader::new(file);
            let mut lines = reader.lines();

            // Read header if present
            let columns = if has_header {
                if let Some(header_line) = lines.next_line().await? {
                    header_line
                        .split(delimiter)
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>()
                } else {
                    return Err(anyhow!("CSV file is empty"));
                }
            } else {
                Vec::new()
            };

            // Create stream
            let stream = async_stream::stream! {
                let mut line_num = if has_header { 1 } else { 0 };

                while let Some(line) = lines.next_line().await.transpose() {
                    line_num += 1;

                    match line {
                        Ok(line_content) => {
                            if line_content.trim().is_empty() {
                                continue;
                            }

                            let values: Vec<String> = line_content
                                .split(delimiter)
                                .map(|s| s.trim().to_string())
                                .collect();

                            let mut row = HashMap::new();

                            if columns.is_empty() {
                                // No header - use col_0, col_1, etc.
                                for (i, value) in values.into_iter().enumerate() {
                                    row.insert(
                                        format!("col_{}", i),
                                        JsonValue::String(value),
                                    );
                                }
                            } else {
                                // Use header column names
                                for (i, col_name) in columns.iter().enumerate() {
                                    let value = values.get(i).cloned().unwrap_or_default();
                                    row.insert(
                                        col_name.clone(),
                                        JsonValue::String(value),
                                    );
                                }
                            }

                            yield Ok(row);
                        }
                        Err(e) => {
                            yield Err(anyhow!("Error reading line {}: {}", line_num, e));
                        }
                    }
                }
            };

            Ok(Box::pin(stream))
        } else {
            Err(anyhow!("Invalid source type for CsvFileReader"))
        }
    }

    fn source(&self) -> &DataSource {
        &self.source
    }
}

// === Database Query Reader ===

/// Reader for database queries
pub struct DatabaseQueryReader {
    source: DataSource,
}

impl DatabaseQueryReader {
    /// Create a new database query reader
    pub fn new(source: DataSource) -> Result<Self> {
        match &source {
            DataSource::DatabaseQuery { .. } => Ok(Self { source }),
            _ => Err(anyhow!(
                "DatabaseQueryReader requires DataSource::DatabaseQuery"
            )),
        }
    }

    /// Build connection string from source configuration
    fn build_connection_string(source: &DataSource) -> Result<String> {
        if let DataSource::DatabaseQuery {
            database_type,
            connection_config,
            ..
        } = source
        {
            let conn_str = match database_type {
                DatabaseType::Postgres => {
                    let mut conn_str = format!(
                        "host={} port={} dbname={} user={} password={}",
                        connection_config.host,
                        connection_config.port,
                        connection_config.database,
                        connection_config.username,
                        connection_config.password
                    );

                    if let Some(ssl_mode) = &connection_config.ssl_mode {
                        conn_str.push_str(&format!(" sslmode={}", ssl_mode));
                    }

                    conn_str
                }
                DatabaseType::DB2 => format!(
                    "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
                    connection_config.database,
                    connection_config.host,
                    connection_config.port,
                    connection_config.username,
                    connection_config.password
                ),
                DatabaseType::Oracle => build_oracle_workflow_connection_string(connection_config)?,
                DatabaseType::SAPHANA => Self::build_hana_connection_string(connection_config)?,
                DatabaseType::Databricks => {
                    let (config, credentials) =
                        workflow_connection_to_databricks(connection_config)?;
                    build_loader_connection_string(&config, &credentials)
                }
                _ => {
                    return Err(anyhow!(
                        "Database type {:?} not yet supported for query reading",
                        database_type
                    ))
                }
            };
            Ok(conn_str)
        } else {
            Err(anyhow!("Invalid source type for DatabaseQueryReader"))
        }
    }

    fn build_hana_connection_string(
        connection_config: &DatabaseConnectionConfig,
    ) -> Result<String> {
        if let Some(raw) = connection_config.extra_params.get("odbc_connection_string") {
            return Ok(Self::apply_credentials_to_connection_string(
                raw,
                connection_config,
            ));
        }

        let driver = connection_config
            .extra_params
            .get("odbc_driver")
            .cloned()
            .or_else(|| std::env::var("GRAPHICA_HANA_ODBC_DRIVER").ok())
            .unwrap_or_else(|| "HDBODBC".to_string());

        let dsn = connection_config
            .extra_params
            .get("odbc_dsn")
            .cloned()
            .or_else(|| std::env::var("GRAPHICA_HANA_ODBC_DSN").ok());

        let mut conn = if let Some(dsn) = dsn {
            format!(
                "DSN={};UID={};PWD={}",
                dsn, connection_config.username, connection_config.password
            )
        } else {
            let mut conn = format!(
                "DRIVER={{{}}};SERVERNODE={}:{};UID={};PWD={};",
                driver,
                connection_config.host,
                connection_config.port,
                connection_config.username,
                connection_config.password
            );

            if !connection_config.database.is_empty() {
                conn.push_str(&format!("DATABASENAME={};", connection_config.database));
            }

            conn
        };

        Self::append_odbc_options(&mut conn, connection_config);
        Ok(conn)
    }

    fn append_odbc_options(conn: &mut String, connection_config: &DatabaseConnectionConfig) {
        if let Some(options) = connection_config.extra_params.get("odbc_options") {
            if !options.is_empty() {
                if !conn.ends_with(';') {
                    conn.push(';');
                }
                conn.push_str(options);
            }
        }
    }

    fn apply_credentials_to_connection_string(
        raw: &str,
        connection_config: &DatabaseConnectionConfig,
    ) -> String {
        let mut conn = raw.to_string();
        let upper = conn.to_uppercase();

        if !upper.contains("UID=") {
            if !conn.ends_with(';') {
                conn.push(';');
            }
            conn.push_str(&format!("UID={}", connection_config.username));
        }

        if !upper.contains("PWD=") {
            if !conn.ends_with(';') {
                conn.push(';');
            }
            conn.push_str(&format!("PWD={}", connection_config.password));
        }

        conn
    }
}

#[async_trait]
impl DataSourceReader for DatabaseQueryReader {
    async fn metadata(&self) -> Result<SourceMetadata> {
        if let DataSource::DatabaseQuery {
            datasource_id,
            query,
            ..
        } = &self.source
        {
            // For database queries, we can't know columns without executing
            // Return minimal metadata
            Ok(SourceMetadata {
                estimated_rows: None,
                columns: vec![], // Will be populated on first read
                source_identifier: datasource_id.clone(),
                extra: HashMap::new(),
            })
        } else {
            Err(anyhow!("Invalid source type for DatabaseQueryReader"))
        }
    }

    async fn read(&mut self) -> Result<DataStream> {
        if let DataSource::DatabaseQuery {
            database_type,
            query,
            fetch_size,
            ..
        } = &self.source
        {
            match database_type {
                DatabaseType::Postgres => self.read_postgres_query(query, *fetch_size).await,
                DatabaseType::DB2 => self.read_odbc_query(query, *fetch_size, "DB2").await,
                DatabaseType::Oracle => self.read_odbc_query(query, *fetch_size, "Oracle").await,
                DatabaseType::SAPHANA => self.read_odbc_query(query, *fetch_size, "SAP HANA").await,
                DatabaseType::Databricks => self.read_databricks_query(query, *fetch_size).await,
                _ => Err(anyhow!(
                    "Database type {:?} not yet supported for query reading",
                    database_type
                )),
            }
        } else {
            Err(anyhow!("Invalid source type for DatabaseQueryReader"))
        }
    }

    fn source(&self) -> &DataSource {
        &self.source
    }
}

impl DatabaseQueryReader {
    fn postgres_row_value_to_json(row: &tokio_postgres::Row, idx: usize) -> JsonValue {
        if let Ok(val) = row.try_get::<_, Option<bool>>(idx) {
            return val.map(JsonValue::Bool).unwrap_or(JsonValue::Null);
        }
        if let Ok(val) = row.try_get::<_, Option<i16>>(idx) {
            return val
                .map(|value| JsonValue::Number((value as i64).into()))
                .unwrap_or(JsonValue::Null);
        }
        if let Ok(val) = row.try_get::<_, Option<i32>>(idx) {
            return val
                .map(|value| JsonValue::Number(value.into()))
                .unwrap_or(JsonValue::Null);
        }
        if let Ok(val) = row.try_get::<_, Option<i64>>(idx) {
            return val
                .map(|value| JsonValue::Number(value.into()))
                .unwrap_or(JsonValue::Null);
        }
        if let Ok(val) = row.try_get::<_, Option<f64>>(idx) {
            return val
                .and_then(serde_json::Number::from_f64)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null);
        }
        if let Ok(val) = row.try_get::<_, Option<String>>(idx) {
            return val.map(JsonValue::String).unwrap_or(JsonValue::Null);
        }

        JsonValue::Null
    }

    fn postgres_row_to_data_row(row: &tokio_postgres::Row) -> DataRow {
        row.columns()
            .iter()
            .enumerate()
            .map(|(idx, column)| {
                (
                    column.name().to_string(),
                    Self::postgres_row_value_to_json(row, idx),
                )
            })
            .collect()
    }

    /// Read from PostgreSQL database
    async fn read_postgres_query(&self, query: &str, fetch_size: usize) -> Result<DataStream> {
        let connection_string = Self::build_connection_string(&self.source)?;
        let ssl_mode = match &self.source {
            DataSource::DatabaseQuery {
                connection_config, ..
            } => connection_config.ssl_mode.as_deref(),
            _ => None,
        };

        let client = connect_postgres_client(&connection_string, ssl_mode)
            .await
            .context("Failed to connect to PostgreSQL")?;

        let query_owned = query.to_string();
        let row_stream = client
            .query_raw(&query_owned, std::iter::empty::<&(dyn ToSql + Sync)>())
            .await
            .context("Failed to execute query")?;
        let chunk_size = fetch_size.max(1);

        let stream = async_stream::stream! {
            let chunked = row_stream.try_chunks(chunk_size);
            futures::pin_mut!(chunked);
            while let Some(chunk_result) = chunked.next().await {
                match chunk_result {
                    Ok(rows) => {
                        for row in rows {
                            yield Ok(Self::postgres_row_to_data_row(&row));
                        }
                    }
                    Err(error) => {
                        yield Err(anyhow!(error).context("Failed to stream PostgreSQL rows"));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// Read from an ODBC-backed database using the shared discovery helper.
    async fn read_odbc_query(
        &self,
        query: &str,
        fetch_size: usize,
        database_label: &str,
    ) -> Result<DataStream> {
        use crate::mapping::discovery::extractors::odbc::execute_odbc_query;

        let connection_string = Self::build_connection_string(&self.source)?;

        // Execute query using ODBC helper (normalize_headers = false to preserve source column casing)
        let query_owned = query.to_string();
        let result = execute_odbc_query(&connection_string, &query_owned, false)
            .await
            .with_context(|| format!("Failed to execute {} query via ODBC", database_label))?;

        // Convert HashMap<String, String> to HashMap<String, JsonValue>
        let stream = async_stream::stream! {
            for row in result {
                let json_row: DataRow = row.into_iter()
                    .map(|(k, v)| (k, JsonValue::String(v)))
                    .collect();
                yield Ok(json_row);
            }
        };

        let _ = fetch_size; // fetch_size is handled by the ODBC driver
        Ok(Box::pin(stream))
    }

    async fn read_databricks_query(&self, query: &str, _fetch_size: usize) -> Result<DataStream> {
        let connection_config = match &self.source {
            DataSource::DatabaseQuery {
                connection_config, ..
            } => connection_config,
            _ => return Err(anyhow!("Invalid source type for Databricks query reader")),
        };

        let (config, credentials) = workflow_connection_to_databricks(connection_config)?;
        let client = DatabricksSqlClient::from_config(&config, &credentials)
            .map_err(|error| anyhow!(error))
            .context("Failed to create Databricks SQL client")?;
        let result = client
            .execute_query(query, HashMap::new(), None, 300)
            .await
            .map_err(|error| anyhow!(error))
            .context("Failed to execute Databricks query")?;

        let stream = async_stream::stream! {
            for row in result.rows {
                match row {
                    JsonValue::Object(map) => {
                        let data_row: DataRow = map.into_iter().collect();
                        yield Ok(data_row);
                    }
                    value => {
                        let mut data_row = HashMap::new();
                        data_row.insert("value".to_string(), value);
                        yield Ok(data_row);
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

// === S3 Object Reader (Stub for Phase 2) ===

/// Reader for S3 objects
pub struct S3ObjectReader {
    source: DataSource,
}

impl S3ObjectReader {
    /// Create a new S3 object reader
    pub fn new(source: DataSource) -> Result<Self> {
        match &source {
            DataSource::S3Object { .. } => Ok(Self { source }),
            _ => Err(anyhow!("S3ObjectReader requires DataSource::S3Object")),
        }
    }
}

#[async_trait]
impl DataSourceReader for S3ObjectReader {
    async fn metadata(&self) -> Result<SourceMetadata> {
        // TODO (Phase 2): Implement S3 metadata retrieval
        Err(anyhow!("S3ObjectReader not yet implemented - Phase 2"))
    }

    async fn read(&mut self) -> Result<DataStream> {
        // TODO (Phase 2): Implement S3 object reading
        Err(anyhow!("S3ObjectReader not yet implemented - Phase 2"))
    }

    fn source(&self) -> &DataSource {
        &self.source
    }
}

// === Factory Function ===

/// Create appropriate reader for a data source
pub fn create_reader(source: DataSource) -> Result<Box<dyn DataSourceReader>> {
    match &source {
        DataSource::CsvFile { .. } => Ok(Box::new(CsvFileReader::new(source)?)),
        DataSource::DatabaseQuery { .. } => Ok(Box::new(DatabaseQueryReader::new(source)?)),
        DataSource::S3Object { .. } => Ok(Box::new(S3ObjectReader::new(source)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::path::PathBuf;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_csv_reader_with_header() {
        // Create test CSV file
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_data.csv");

        let csv_content = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,SF\n";
        fs::write(&test_file, csv_content).await.unwrap();

        let source = DataSource::CsvFile {
            file_id: "test_1".to_string(),
            file_path: test_file.clone(),
            encoding: Some("UTF-8".to_string()),
            delimiter: Some(','),
            has_header: true,
        };

        let mut reader = CsvFileReader::new(source).unwrap();

        // Test metadata
        let metadata = reader.metadata().await.unwrap();
        assert_eq!(metadata.columns, vec!["name", "age", "city"]);

        // Test reading
        let mut stream = reader.read().await.unwrap();
        let mut rows = Vec::new();

        while let Some(result) = stream.next().await {
            rows.push(result.unwrap());
        }

        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0].get("name").unwrap(),
            &JsonValue::String("Alice".to_string())
        );
        assert_eq!(
            rows[1].get("age").unwrap(),
            &JsonValue::String("25".to_string())
        );

        // Cleanup
        fs::remove_file(&test_file).await.ok();
    }

    #[tokio::test]
    async fn test_csv_reader_without_header() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_noheader.csv");

        let csv_content = "Alice,30,NYC\nBob,25,LA\n";
        fs::write(&test_file, csv_content).await.unwrap();

        let source = DataSource::CsvFile {
            file_id: "test_2".to_string(),
            file_path: test_file.clone(),
            encoding: None,
            delimiter: Some(','),
            has_header: false,
        };

        let mut reader = CsvFileReader::new(source).unwrap();
        let mut stream = reader.read().await.unwrap();
        let mut rows = Vec::new();

        while let Some(result) = stream.next().await {
            rows.push(result.unwrap());
        }

        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].get("col_0").unwrap(),
            &JsonValue::String("Alice".to_string())
        );
        assert_eq!(
            rows[0].get("col_1").unwrap(),
            &JsonValue::String("30".to_string())
        );

        fs::remove_file(&test_file).await.ok();
    }

    #[tokio::test]
    async fn test_factory_creates_csv_reader() {
        let source = DataSource::CsvFile {
            file_id: "test".to_string(),
            file_path: PathBuf::from("/tmp/test.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };

        let reader = create_reader(source).unwrap();
        assert!(matches!(reader.source(), DataSource::CsvFile { .. }));
    }

    #[tokio::test]
    async fn test_database_reader_creation() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_db".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: DatabaseConnectionConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "testdb".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT * FROM test".to_string(),
            fetch_size: 1000,
        };

        let reader = DatabaseQueryReader::new(source).unwrap();

        // Metadata should work (returns minimal info)
        let metadata = reader.metadata().await.unwrap();
        assert_eq!(metadata.source_identifier, "test_db");

        // Note: Actual query execution would require a real database connection
        // That's tested in E2E tests, not unit tests
    }

    #[tokio::test]
    async fn test_db2_reader_implemented() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_db2".to_string(),
            database_type: DatabaseType::DB2,
            connection_config: DatabaseConnectionConfig {
                host: "localhost".to_string(),
                port: 50000,
                database: "TESTDB".to_string(),
                username: "db2inst1".to_string(),
                password: "pass".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT * FROM TEST".to_string(),
            fetch_size: 1000,
        };

        let reader = DatabaseQueryReader::new(source).unwrap();

        // Metadata should work (returns minimal info)
        let metadata = reader.metadata().await.unwrap();
        assert_eq!(metadata.source_identifier, "test_db2");

        // Note: Actual query execution would require a real DB2 connection with ODBC drivers
        // That's tested in E2E tests, not unit tests
    }

    #[test]
    fn test_oracle_reader_builds_odbc_connection_string() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_oracle".to_string(),
            database_type: DatabaseType::Oracle,
            connection_config: DatabaseConnectionConfig {
                host: "oracle.example.com".to_string(),
                port: 1521,
                database: "ORCLPDB1".to_string(),
                username: "etl_user".to_string(),
                password: "secret".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT 1 FROM dual".to_string(),
            fetch_size: 1000,
        };

        let connection_string = DatabaseQueryReader::build_connection_string(&source).unwrap();
        assert!(connection_string.contains("DBQ=//oracle.example.com:1521/ORCLPDB1"));
        assert!(connection_string.contains("UID=etl_user"));
        assert!(connection_string.contains("PWD=secret"));
    }

    #[test]
    fn test_oracle_reader_accepts_camel_case_service_name_extra_param() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let mut extra_params = HashMap::new();
        extra_params.insert("serviceName".to_string(), "ORCLPDB1".to_string());

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_oracle".to_string(),
            database_type: DatabaseType::Oracle,
            connection_config: DatabaseConnectionConfig {
                host: "oracle.example.com".to_string(),
                port: 1521,
                database: String::new(),
                username: "etl_user".to_string(),
                password: "secret".to_string(),
                ssl_mode: None,
                extra_params,
            },
            query: "SELECT 1 FROM dual".to_string(),
            fetch_size: 1000,
        };

        let connection_string = DatabaseQueryReader::build_connection_string(&source).unwrap();
        assert!(connection_string.contains("DBQ=//oracle.example.com:1521/ORCLPDB1"));
    }

    #[test]
    fn test_hana_reader_builds_odbc_connection_string() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let mut extra_params = HashMap::new();
        extra_params.insert("odbc_driver".to_string(), "CustomHDBODBC".to_string());

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_hana".to_string(),
            database_type: DatabaseType::SAPHANA,
            connection_config: DatabaseConnectionConfig {
                host: "hana.example.com".to_string(),
                port: 30015,
                database: "HXE".to_string(),
                username: "SYSTEM".to_string(),
                password: "secret".to_string(),
                ssl_mode: None,
                extra_params,
            },
            query: "SELECT 1 FROM DUMMY".to_string(),
            fetch_size: 1000,
        };

        let connection_string = DatabaseQueryReader::build_connection_string(&source).unwrap();
        assert!(connection_string.contains("DRIVER={CustomHDBODBC};"));
        assert!(connection_string.contains("SERVERNODE=hana.example.com:30015;"));
        assert!(connection_string.contains("DATABASENAME=HXE;"));
        assert!(connection_string.contains("UID=SYSTEM"));
        assert!(connection_string.contains("PWD=secret"));
    }

    #[test]
    fn test_databricks_reader_builds_loader_connection_string() {
        use super::super::DatabaseConnectionConfig;
        use super::super::DatabaseType;

        let mut extra_params = HashMap::new();
        extra_params.insert(
            "http_path".to_string(),
            "/sql/1.0/warehouses/abc123".to_string(),
        );
        extra_params.insert("schema".to_string(), "bronze".to_string());

        let source = DataSource::DatabaseQuery {
            datasource_id: "test_databricks".to_string(),
            database_type: DatabaseType::Databricks,
            connection_config: DatabaseConnectionConfig {
                host: "https://adb-123.azuredatabricks.net".to_string(),
                port: 443,
                database: "main".to_string(),
                username: "svc_arcxa".to_string(),
                password: "token-value".to_string(),
                ssl_mode: Some("require".to_string()),
                extra_params,
            },
            query: "SELECT 1".to_string(),
            fetch_size: 1000,
        };

        let connection_string = DatabaseQueryReader::build_connection_string(&source).unwrap();
        assert!(connection_string.contains("workspace_url=https://adb-123.azuredatabricks.net"));
        assert!(connection_string.contains("http_path=/sql/1.0/warehouses/abc123"));
        assert!(connection_string.contains("catalog=main"));
        assert!(connection_string.contains("schema=bronze"));
        assert!(connection_string.contains("token=token-value"));
    }
}
