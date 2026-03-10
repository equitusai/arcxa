//! Data Source Abstraction
//!
//! Unified interface for reading data from various sources in batch jobs.
//!
//! ## Supported Sources
//!
//! - **CSV Files**: Local filesystem CSV files
//! - **Database Queries**: Extract from Postgres, DB2, Oracle, SAP HANA, MySQL, Databricks
//! - **S3 Objects**: CSV or Parquet files from S3
//!
//! ## Example
//!
//! ```rust
//! use graphica_coordinator::workflows::domain::{DataSource, DatabaseType, DatabaseConnectionConfig};
//! use std::path::PathBuf;
//!
//! // CSV file source
//! let csv_source = DataSource::CsvFile {
//!     file_id: "customers_001".to_string(),
//!     file_path: PathBuf::from("/data/customers.csv"),
//!     encoding: Some("UTF-8".to_string()),
//!     delimiter: Some(','),
//!     has_header: true,
//! };
//!
//! // Database query source
//! let db_source = DataSource::DatabaseQuery {
//!     datasource_id: "legacy_oracle".to_string(),
//!     database_type: DatabaseType::Oracle,
//!     connection_config: DatabaseConnectionConfig {
//!         host: "oracle.example.com".to_string(),
//!         port: 1521,
//!         database: "PROD".to_string(),
//!         username: "etl_user".to_string(),
//!         password: "secret".to_string(),
//!         ssl_mode: None,
//!         extra_params: std::collections::HashMap::new(),
//!     },
//!     query: "SELECT * FROM ORDERS WHERE created_date >= SYSDATE-30".to_string(),
//!     fetch_size: 10000,
//! };
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Data source for batch job execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DataSource {
    /// CSV file on local filesystem
    CsvFile {
        /// Unique identifier for this file
        file_id: String,

        /// Path to CSV file
        file_path: PathBuf,

        /// Character encoding (default: UTF-8)
        #[serde(default)]
        encoding: Option<String>,

        /// Field delimiter (default: comma)
        #[serde(default)]
        delimiter: Option<char>,

        /// Whether CSV has header row (default: true)
        #[serde(default = "default_has_header")]
        has_header: bool,
    },

    /// Database query extraction
    DatabaseQuery {
        /// Unique identifier for this data source
        datasource_id: String,

        /// Type of database (Postgres, DB2, Oracle, SAP HANA, Databricks, etc.)
        database_type: DatabaseType,

        /// Database connection configuration
        connection_config: DatabaseConnectionConfig,

        /// SQL query to execute
        query: String,

        /// Number of rows to fetch per batch (default: 10000)
        #[serde(default = "default_fetch_size")]
        fetch_size: usize,
    },

    /// S3 object (CSV or Parquet)
    S3Object {
        /// S3 bucket name
        bucket: String,

        /// Object key (path within bucket)
        key: String,

        /// AWS region
        region: String,

        /// AWS access key ID (if not using IAM role)
        #[serde(skip_serializing_if = "Option::is_none")]
        access_key_id: Option<String>,

        /// AWS secret access key (if not using IAM role)
        #[serde(skip_serializing_if = "Option::is_none")]
        secret_access_key: Option<String>,

        /// File format (csv or parquet)
        #[serde(default = "default_file_format")]
        file_format: String,
    },
}

fn default_has_header() -> bool {
    true
}

fn default_fetch_size() -> usize {
    10000
}

fn default_file_format() -> String {
    "csv".to_string()
}

/// Database type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    Postgres,
    DB2,
    Oracle,
    SAPHANA,
    MySQL,
    Snowflake,
    Databricks,
}

impl DatabaseType {
    /// Get human-readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::DB2 => "IBM DB2",
            Self::Oracle => "Oracle Database",
            Self::SAPHANA => "SAP HANA",
            Self::MySQL => "MySQL",
            Self::Snowflake => "Snowflake",
            Self::Databricks => "Databricks SQL Warehouse",
        }
    }

    /// Get default port for database type
    pub fn default_port(&self) -> u16 {
        match self {
            Self::Postgres => 5432,
            Self::DB2 => 50000,
            Self::Oracle => 1521,
            Self::SAPHANA => 30015,
            Self::MySQL => 3306,
            Self::Snowflake => 443,
            Self::Databricks => 443,
        }
    }
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseConnectionConfig {
    /// Database host
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password (not serialized in logs)
    #[serde(skip_serializing)]
    pub password: String,

    /// SSL/TLS mode (e.g., "require", "prefer", "disable")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<String>,

    /// Additional connection parameters
    #[serde(default)]
    pub extra_params: HashMap<String, String>,
}

impl DataSource {
    /// Get unique identifier for this data source
    pub fn get_identifier(&self) -> String {
        match self {
            Self::CsvFile { file_id, .. } => file_id.clone(),
            Self::DatabaseQuery { datasource_id, .. } => datasource_id.clone(),
            Self::S3Object { bucket, key, .. } => format!("s3://{}/{}", bucket, key),
        }
    }

    /// Get human-readable display name
    pub fn display_name(&self) -> String {
        match self {
            Self::CsvFile { file_path, .. } => {
                format!("CSV: {}", file_path.display())
            }
            Self::DatabaseQuery {
                database_type,
                connection_config,
                ..
            } => {
                format!(
                    "{}: {}@{}",
                    database_type.display_name(),
                    connection_config.database,
                    connection_config.host
                )
            }
            Self::S3Object { bucket, key, .. } => {
                format!("S3: s3://{}/{}", bucket, key)
            }
        }
    }

    /// Get source type as string
    pub fn source_type(&self) -> &'static str {
        match self {
            Self::CsvFile { .. } => "CsvFile",
            Self::DatabaseQuery { .. } => "DatabaseQuery",
            Self::S3Object { .. } => "S3Object",
        }
    }

    /// Validate data source configuration
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::CsvFile {
                file_path, file_id, ..
            } => {
                if file_id.is_empty() {
                    return Err("File ID cannot be empty".to_string());
                }
                if !file_path.exists() {
                    return Err(format!("File not found: {:?}", file_path));
                }
                if !file_path.is_file() {
                    return Err(format!("Path is not a file: {:?}", file_path));
                }
                Ok(())
            }
            Self::DatabaseQuery {
                datasource_id,
                query,
                connection_config,
                ..
            } => {
                if datasource_id.is_empty() {
                    return Err("Datasource ID cannot be empty".to_string());
                }
                if query.trim().is_empty() {
                    return Err("Query cannot be empty".to_string());
                }
                if connection_config.host.is_empty() {
                    return Err("Database host cannot be empty".to_string());
                }
                if connection_config.database.is_empty() {
                    return Err("Database name cannot be empty".to_string());
                }
                if connection_config.username.is_empty() {
                    return Err("Database username cannot be empty".to_string());
                }
                Ok(())
            }
            Self::S3Object { bucket, key, .. } => {
                if bucket.is_empty() {
                    return Err("S3 bucket cannot be empty".to_string());
                }
                if key.is_empty() {
                    return Err("S3 key cannot be empty".to_string());
                }
                Ok(())
            }
        }
    }

    /// Estimate data size (if known)
    pub fn estimated_size_bytes(&self) -> Option<u64> {
        match self {
            Self::CsvFile { file_path, .. } => std::fs::metadata(file_path).ok().map(|m| m.len()),
            Self::DatabaseQuery { .. } => None, // Unknown until query executes
            Self::S3Object { .. } => None,      // Would need S3 API call
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_csv_file_serialization() {
        let source = DataSource::CsvFile {
            file_id: "file_001".to_string(),
            file_path: PathBuf::from("/data/customers.csv"),
            encoding: Some("UTF-8".to_string()),
            delimiter: Some(','),
            has_header: true,
        };

        let json = serde_json::to_string(&source).unwrap();
        let deserialized: DataSource = serde_json::from_str(&json).unwrap();

        match deserialized {
            DataSource::CsvFile { file_id, .. } => assert_eq!(file_id, "file_001"),
            _ => panic!("Wrong variant after deserialization"),
        }
    }

    #[test]
    fn test_database_query_serialization() {
        let source = DataSource::DatabaseQuery {
            datasource_id: "legacy_db".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: DatabaseConnectionConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "testdb".to_string(),
                username: "testuser".to_string(),
                password: "secret".to_string(),
                ssl_mode: Some("require".to_string()),
                extra_params: HashMap::new(),
            },
            query: "SELECT * FROM orders".to_string(),
            fetch_size: 10000,
        };

        let json = serde_json::to_string(&source).unwrap();
        assert!(!json.contains("secret")); // Password should be skipped
        assert!(json.contains("legacy_db"));
        assert!(json.contains("Postgres"));

        // Note: Cannot round-trip deserialize because password field is required but skip_serializing
        // This is intentional for security - passwords should never be serialized
    }

    #[test]
    fn test_s3_object_serialization() {
        let source = DataSource::S3Object {
            bucket: "data-lake".to_string(),
            key: "products/2024/products.csv".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };

        let json = serde_json::to_string(&source).unwrap();
        let deserialized: DataSource = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, source);
    }

    #[test]
    fn test_get_identifier() {
        let csv = DataSource::CsvFile {
            file_id: "csv_123".to_string(),
            file_path: PathBuf::from("/data/test.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert_eq!(csv.get_identifier(), "csv_123");

        let db = DataSource::DatabaseQuery {
            datasource_id: "db_456".to_string(),
            database_type: DatabaseType::DB2,
            connection_config: DatabaseConnectionConfig {
                host: "db2.example.com".to_string(),
                port: 50000,
                database: "PROD".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT 1".to_string(),
            fetch_size: 1000,
        };
        assert_eq!(db.get_identifier(), "db_456");

        let s3 = DataSource::S3Object {
            bucket: "my-bucket".to_string(),
            key: "data/file.csv".to_string(),
            region: "us-west-2".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };
        assert_eq!(s3.get_identifier(), "s3://my-bucket/data/file.csv");
    }

    #[test]
    fn test_display_name() {
        let csv = DataSource::CsvFile {
            file_id: "csv_001".to_string(),
            file_path: PathBuf::from("/data/customers.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert!(csv.display_name().contains("customers.csv"));

        let db = DataSource::DatabaseQuery {
            datasource_id: "oracle_prod".to_string(),
            database_type: DatabaseType::Oracle,
            connection_config: DatabaseConnectionConfig {
                host: "oracle.example.com".to_string(),
                port: 1521,
                database: "PROD".to_string(),
                username: "etl".to_string(),
                password: "secret".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT * FROM ORDERS".to_string(),
            fetch_size: 10000,
        };
        let display = db.display_name();
        assert!(display.contains("Oracle"));
        assert!(display.contains("PROD"));
        assert!(display.contains("oracle.example.com"));
    }

    #[test]
    fn test_source_type() {
        let csv = DataSource::CsvFile {
            file_id: "f1".to_string(),
            file_path: PathBuf::from("/tmp/test.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert_eq!(csv.source_type(), "CsvFile");

        let db = DataSource::DatabaseQuery {
            datasource_id: "d1".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: DatabaseConnectionConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT 1".to_string(),
            fetch_size: 100,
        };
        assert_eq!(db.source_type(), "DatabaseQuery");

        let s3 = DataSource::S3Object {
            bucket: "bucket".to_string(),
            key: "key".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };
        assert_eq!(s3.source_type(), "S3Object");
    }

    #[test]
    fn test_validation_csv_file() {
        // Valid file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "id,name").unwrap();
        writeln!(temp_file, "1,Alice").unwrap();

        let valid_csv = DataSource::CsvFile {
            file_id: "valid".to_string(),
            file_path: temp_file.path().to_path_buf(),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert!(valid_csv.validate().is_ok());

        // Empty file_id
        let invalid_csv = DataSource::CsvFile {
            file_id: "".to_string(),
            file_path: temp_file.path().to_path_buf(),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert!(invalid_csv.validate().is_err());

        // Non-existent file
        let missing_csv = DataSource::CsvFile {
            file_id: "missing".to_string(),
            file_path: PathBuf::from("/nonexistent/file.csv"),
            encoding: None,
            delimiter: None,
            has_header: true,
        };
        assert!(missing_csv.validate().is_err());
    }

    #[test]
    fn test_validation_database_query() {
        let valid_db = DataSource::DatabaseQuery {
            datasource_id: "valid_db".to_string(),
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
            query: "SELECT * FROM orders".to_string(),
            fetch_size: 1000,
        };
        assert!(valid_db.validate().is_ok());

        // Empty datasource_id
        let invalid_id = DataSource::DatabaseQuery {
            datasource_id: "".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: valid_db.clone_config(),
            query: "SELECT 1".to_string(),
            fetch_size: 1000,
        };
        assert!(invalid_id.validate().is_err());

        // Empty query
        let invalid_query = DataSource::DatabaseQuery {
            datasource_id: "db1".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: valid_db.clone_config(),
            query: "   ".to_string(),
            fetch_size: 1000,
        };
        assert!(invalid_query.validate().is_err());

        // Empty host
        let invalid_host = DataSource::DatabaseQuery {
            datasource_id: "db1".to_string(),
            database_type: DatabaseType::Postgres,
            connection_config: DatabaseConnectionConfig {
                host: "".to_string(),
                port: 5432,
                database: "test".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: None,
                extra_params: HashMap::new(),
            },
            query: "SELECT 1".to_string(),
            fetch_size: 1000,
        };
        assert!(invalid_host.validate().is_err());
    }

    #[test]
    fn test_validation_s3_object() {
        let valid_s3 = DataSource::S3Object {
            bucket: "my-bucket".to_string(),
            key: "data/file.csv".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };
        assert!(valid_s3.validate().is_ok());

        // Empty bucket
        let invalid_bucket = DataSource::S3Object {
            bucket: "".to_string(),
            key: "file.csv".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };
        assert!(invalid_bucket.validate().is_err());

        // Empty key
        let invalid_key = DataSource::S3Object {
            bucket: "bucket".to_string(),
            key: "".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            file_format: "csv".to_string(),
        };
        assert!(invalid_key.validate().is_err());
    }

    #[test]
    fn test_database_type_display_name() {
        assert_eq!(DatabaseType::Postgres.display_name(), "PostgreSQL");
        assert_eq!(DatabaseType::DB2.display_name(), "IBM DB2");
        assert_eq!(DatabaseType::Oracle.display_name(), "Oracle Database");
        assert_eq!(DatabaseType::SAPHANA.display_name(), "SAP HANA");
        assert_eq!(DatabaseType::MySQL.display_name(), "MySQL");
        assert_eq!(DatabaseType::Snowflake.display_name(), "Snowflake");
        assert_eq!(
            DatabaseType::Databricks.display_name(),
            "Databricks SQL Warehouse"
        );
    }

    #[test]
    fn test_database_type_default_port() {
        assert_eq!(DatabaseType::Postgres.default_port(), 5432);
        assert_eq!(DatabaseType::DB2.default_port(), 50000);
        assert_eq!(DatabaseType::Oracle.default_port(), 1521);
        assert_eq!(DatabaseType::SAPHANA.default_port(), 30015);
        assert_eq!(DatabaseType::MySQL.default_port(), 3306);
        assert_eq!(DatabaseType::Snowflake.default_port(), 443);
        assert_eq!(DatabaseType::Databricks.default_port(), 443);
    }

    #[test]
    fn test_estimated_size_bytes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "id,name").unwrap();
        writeln!(temp_file, "1,Alice").unwrap();
        temp_file.flush().unwrap();

        let csv = DataSource::CsvFile {
            file_id: "test".to_string(),
            file_path: temp_file.path().to_path_buf(),
            encoding: None,
            delimiter: None,
            has_header: true,
        };

        let size = csv.estimated_size_bytes();
        assert!(size.is_some());
        assert!(size.unwrap() > 0);
    }

    // Helper for tests
    impl DataSource {
        fn clone_config(&self) -> DatabaseConnectionConfig {
            match self {
                Self::DatabaseQuery {
                    connection_config, ..
                } => connection_config.clone(),
                _ => panic!("Not a database query"),
            }
        }
    }
}
