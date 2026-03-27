//! Data Source Connector Implementations
//!
//! Concrete implementations for each supported data source type.

pub mod csv;
pub mod database_stats; // Phase 1: Database-agnostic statistics extraction
pub mod databricks;
#[cfg(feature = "odbc")]
pub mod db2;
pub mod enhanced_inference;
pub mod mysql;
#[cfg(feature = "odbc")]
pub mod oracle;
pub mod postgresql;
pub mod rdf_ntriples;
pub mod s3_parquet;
#[cfg(feature = "odbc")]
pub mod saphana;
pub mod snowflake;

use super::connector::{ConnectorCapabilities, DataSourceConnector};
use super::types::SourceConfig;
use crate::errors::GraphicaError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Connector metadata for API queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorMetadata {
    /// Unique identifier (e.g., "postgresql", "snowflake")
    pub id: String,

    /// Display name (e.g., "PostgreSQL", "Snowflake")
    pub name: String,

    /// Connector version
    pub version: String,

    /// Description
    pub description: String,

    /// Source type this connector handles
    pub source_type: String,

    /// Connector capabilities
    pub capabilities: ConnectorCapabilities,

    /// Required credential fields
    pub required_credentials: Vec<CredentialField>,

    /// Optional configuration fields
    pub optional_config: Vec<ConfigField>,

    /// When this connector was registered
    pub registered_at: DateTime<Utc>,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Whether this connector is enabled
    pub enabled: bool,
}

/// Credential field metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialField {
    pub name: String,
    pub description: String,
    pub field_type: FieldType,
    pub required: bool,
    pub sensitive: bool,
}

/// Configuration field metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    pub name: String,
    pub description: String,
    pub field_type: FieldType,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
}

/// Field type for configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Boolean,
    Url,
    Hostname,
    Port,
    FilePath,
}

/// Connector entry in registry
struct ConnectorEntry {
    metadata: ConnectorMetadata,
    connector: Arc<dyn DataSourceConnector>,
    usage_count: u64,
}

/// Connector registry
///
/// Maps source types to connector implementations with rich metadata.
pub struct ConnectorRegistry {
    connectors: HashMap<String, ConnectorEntry>,
}

impl ConnectorRegistry {
    /// Create new registry with default connectors
    pub fn new() -> Self {
        let mut registry = Self {
            connectors: HashMap::new(),
        };

        // Register default connectors with metadata
        registry.register_with_metadata(
            Arc::new(postgresql::PostgreSQLConnector::new()),
            Self::postgresql_metadata(),
        );
        registry.register_with_metadata(
            Arc::new(mysql::MySQLConnector::new()),
            Self::mysql_metadata(),
        );
        #[cfg(feature = "odbc")]
        registry.register_with_metadata(
            Arc::new(oracle::OracleConnector::new()),
            Self::oracle_metadata(),
        );
        #[cfg(feature = "odbc")]
        registry.register_with_metadata(Arc::new(db2::DB2Connector::new()), Self::db2_metadata());
        #[cfg(feature = "odbc")]
        registry.register_with_metadata(
            Arc::new(saphana::SAPHANAConnector::new()),
            Self::saphana_metadata(),
        );
        registry.register_with_metadata(
            Arc::new(snowflake::SnowflakeConnector::new()),
            Self::snowflake_metadata(),
        );
        registry.register_with_metadata(
            Arc::new(databricks::DatabricksConnector::new()),
            Self::databricks_metadata(),
        );
        registry.register_with_metadata(
            Arc::new(s3_parquet::S3ParquetConnector::new()),
            Self::s3_parquet_metadata(),
        );
        registry.register_with_metadata(Arc::new(csv::CsvConnector::new()), Self::csv_metadata());
        registry.register_with_metadata(
            Arc::new(rdf_ntriples::RDFNTriplesConnector::new()),
            Self::rdf_ntriples_metadata(),
        );

        registry
    }

    /// Register a connector (legacy method)
    pub fn register(&mut self, connector: Arc<dyn DataSourceConnector>) {
        let metadata = ConnectorMetadata {
            id: connector.source_type().to_lowercase(),
            name: connector.name().to_string(),
            version: connector.version().to_string(),
            description: format!("{} connector", connector.name()),
            source_type: connector.source_type().to_string(),
            capabilities: connector.capabilities(),
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "Database username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "Database password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![],
            registered_at: Utc::now(),
            tags: vec!["database".to_string()],
            enabled: true,
        };

        self.register_with_metadata(connector, metadata);
    }

    /// Register a connector with metadata
    pub fn register_with_metadata(
        &mut self,
        connector: Arc<dyn DataSourceConnector>,
        metadata: ConnectorMetadata,
    ) {
        let entry = ConnectorEntry {
            metadata,
            connector,
            usage_count: 0,
        };

        self.connectors
            .insert(entry.metadata.source_type.clone(), entry);
    }

    /// Get connector for source type
    pub fn get(&self, source_type: &str) -> Result<Arc<dyn DataSourceConnector>, GraphicaError> {
        self.connectors
            .get(source_type)
            .map(|entry| entry.connector.clone())
            .ok_or_else(|| {
                GraphicaError::NotFound(format!("No connector for source type: {}", source_type))
            })
    }

    /// Get connector metadata
    pub fn get_metadata(&self, source_type: &str) -> Option<&ConnectorMetadata> {
        self.connectors
            .get(source_type)
            .map(|entry| &entry.metadata)
    }

    /// List all registered connectors with metadata
    pub fn list_connectors(&self) -> Vec<&ConnectorMetadata> {
        self.connectors
            .values()
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// List enabled connectors only
    pub fn list_enabled_connectors(&self) -> Vec<&ConnectorMetadata> {
        self.connectors
            .values()
            .filter(|entry| entry.metadata.enabled)
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// List all registered source types
    pub fn list_types(&self) -> Vec<String> {
        self.connectors.keys().cloned().collect()
    }

    /// Check if source type is supported
    pub fn supports(&self, source_type: &str) -> bool {
        self.connectors.contains_key(source_type)
            && self
                .connectors
                .get(source_type)
                .map(|e| e.metadata.enabled)
                .unwrap_or(false)
    }

    /// Enable a connector
    pub fn enable_connector(&mut self, source_type: &str) -> Result<(), GraphicaError> {
        let entry = self.connectors.get_mut(source_type).ok_or_else(|| {
            GraphicaError::NotFound(format!("Connector {} not found", source_type))
        })?;
        entry.metadata.enabled = true;
        Ok(())
    }

    /// Disable a connector
    pub fn disable_connector(&mut self, source_type: &str) -> Result<(), GraphicaError> {
        let entry = self.connectors.get_mut(source_type).ok_or_else(|| {
            GraphicaError::NotFound(format!("Connector {} not found", source_type))
        })?;
        entry.metadata.enabled = false;
        Ok(())
    }

    /// Get connector for a source config
    ///
    /// Determines the connector type from the config variant.
    pub fn get_connector(&self, config: &SourceConfig) -> Option<Arc<dyn DataSourceConnector>> {
        let source_type = match config {
            SourceConfig::PostgreSQL(_) => "PostgreSQL",
            SourceConfig::MySQL(_) => "MySQL",
            SourceConfig::Oracle(_) => "Oracle",
            SourceConfig::DB2(_) => "DB2",
            SourceConfig::SAPHANA(_) => "SAPHANA",
            SourceConfig::Snowflake(_) => "Snowflake",
            SourceConfig::Databricks(_) => "Databricks",
            SourceConfig::S3Parquet(_) => "S3Parquet",
            SourceConfig::CsvFile(_) => "CsvFile",
            SourceConfig::RDFNTriples(_) => "RDFNTriples",
        };

        self.connectors
            .get(source_type)
            .map(|entry| entry.connector.clone())
    }

    /// Get registry statistics
    pub fn get_statistics(&self) -> RegistryStatistics {
        let total_count = self.connectors.len();
        let enabled_count = self
            .connectors
            .values()
            .filter(|e| e.metadata.enabled)
            .count();

        let mut by_category = HashMap::new();
        for entry in self.connectors.values() {
            for tag in &entry.metadata.tags {
                *by_category.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        let total_usage = self.connectors.values().map(|e| e.usage_count).sum();

        RegistryStatistics {
            total_count,
            enabled_count,
            disabled_count: total_count - enabled_count,
            by_category,
            total_usage,
        }
    }

    // Metadata factory methods for each connector

    fn mysql_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "mysql".to_string(),
            name: "MySQL".to_string(),
            version: "1.0.0".to_string(),
            description:
                "MySQL connector scaffold; query execution and schema inference are not yet implemented"
                    .to_string(),
            source_type: "MySQL".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: false,
                query_timeout: false,
                streaming: false,
                transactions: false,
                max_batch_size: Some(50000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "MySQL username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "MySQL password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "host".to_string(),
                    description: "MySQL server hostname or IP address".to_string(),
                    field_type: FieldType::Hostname,
                    default_value: Some("localhost".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "database".to_string(),
                    description: "Database name to connect to".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^[a-zA-Z_][a-zA-Z0-9_]*$".to_string()),
                },
                ConfigField {
                    name: "port".to_string(),
                    description: "MySQL port".to_string(),
                    field_type: FieldType::Port,
                    default_value: Some("3306".to_string()),
                    validation_regex: Some(r"^\d+$".to_string()),
                },
                ConfigField {
                    name: "ssl_mode".to_string(),
                    description: "SSL connection mode".to_string(),
                    field_type: FieldType::String,
                    default_value: Some("PREFERRED".to_string()),
                    validation_regex: Some(
                        "^(DISABLED|PREFERRED|REQUIRED|VERIFY_CA|VERIFY_IDENTITY)$".to_string(),
                    ),
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "relational".to_string(),
                "sql".to_string(),
            ],
            enabled: true,
        }
    }

    fn postgresql_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "postgresql".to_string(),
            name: "PostgreSQL".to_string(),
            version: "1.0.0".to_string(),
            description: "PostgreSQL database connector with advanced schema inference".to_string(),
            source_type: "PostgreSQL".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: true,
                schema_inference: true,
                query_timeout: true,
                streaming: false,
                transactions: true,
                max_batch_size: Some(10000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "PostgreSQL username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "PostgreSQL password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "host".to_string(),
                    description: "PostgreSQL server hostname or IP address".to_string(),
                    field_type: FieldType::Hostname,
                    default_value: Some("localhost".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "database".to_string(),
                    description: "Database name to connect to".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^[a-zA-Z_][a-zA-Z0-9_]*$".to_string()),
                },
                ConfigField {
                    name: "port".to_string(),
                    description: "PostgreSQL port".to_string(),
                    field_type: FieldType::Port,
                    default_value: Some("5432".to_string()),
                    validation_regex: Some(r"^\d+$".to_string()),
                },
                ConfigField {
                    name: "schema".to_string(),
                    description: "Default schema (optional, defaults to 'public')".to_string(),
                    field_type: FieldType::String,
                    default_value: Some("public".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "ssl_mode".to_string(),
                    description: "SSL connection mode".to_string(),
                    field_type: FieldType::String,
                    default_value: Some("prefer".to_string()),
                    validation_regex: Some(
                        "^(disable|allow|prefer|require|verify-ca|verify-full)$".to_string(),
                    ),
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "relational".to_string(),
                "sql".to_string(),
            ],
            enabled: true,
        }
    }

    fn oracle_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "oracle".to_string(),
            name: "Oracle".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Oracle database connector with support for both service name and SID connections"
                    .to_string(),
            source_type: "Oracle".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: false,
                streaming: false,
                transactions: true,
                max_batch_size: Some(10000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "Oracle username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "Oracle password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "host".to_string(),
                    description: "Oracle server hostname or IP address".to_string(),
                    field_type: FieldType::Hostname,
                    default_value: Some("localhost".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "port".to_string(),
                    description: "Oracle listener port".to_string(),
                    field_type: FieldType::Port,
                    default_value: Some("1521".to_string()),
                    validation_regex: Some(r"^\d+$".to_string()),
                },
                ConfigField {
                    name: "serviceName".to_string(),
                    description: "Oracle service name (required if SID not provided)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "sid".to_string(),
                    description: "Oracle SID (required if serviceName not provided)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "schema".to_string(),
                    description: "Default schema (optional)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "metadata.odbc_driver".to_string(),
                    description:
                        "Optional Oracle ODBC driver override (for example, Oracle 19/21 Instant Client driver name)"
                            .to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "metadata.odbc_dsn".to_string(),
                    description: "Optional Oracle ODBC DSN override".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "metadata.odbc_connection_string".to_string(),
                    description:
                        "Optional raw Oracle ODBC connection string override. When provided, it takes precedence over host/serviceName/sid assembly."
                            .to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "metadata.odbc_options".to_string(),
                    description:
                        "Optional extra Oracle ODBC connection-string segments appended to the resolved connection."
                            .to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "relational".to_string(),
                "enterprise".to_string(),
            ],
            enabled: true,
        }
    }

    fn db2_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "db2".to_string(),
            name: "IBM DB2".to_string(),
            version: "1.0.0".to_string(),
            description: "IBM DB2 database connector with statistics extraction".to_string(),
            source_type: "DB2".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: false,
                streaming: false,
                transactions: true,
                max_batch_size: Some(10000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "DB2 username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "DB2 password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "host".to_string(),
                    description: "DB2 server hostname or IP address".to_string(),
                    field_type: FieldType::Hostname,
                    default_value: Some("localhost".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "database".to_string(),
                    description: "Database name to connect to".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^[a-zA-Z_][a-zA-Z0-9_]*$".to_string()),
                },
                ConfigField {
                    name: "port".to_string(),
                    description: "DB2 listener port".to_string(),
                    field_type: FieldType::Port,
                    default_value: Some("50000".to_string()),
                    validation_regex: Some(r"^\d+$".to_string()),
                },
                ConfigField {
                    name: "schema".to_string(),
                    description: "Default schema (optional)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "mainframe".to_string(),
                "enterprise".to_string(),
            ],
            enabled: true,
        }
    }

    fn saphana_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "saphana".to_string(),
            name: "SAP HANA".to_string(),
            version: "1.0.0".to_string(),
            description:
                "SAP HANA connector with connection testing support; query execution and schema inference are not yet implemented"
                    .to_string(),
            source_type: "SAPHANA".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: false,
                query_timeout: false,
                streaming: false,
                transactions: false,
                max_batch_size: Some(10000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "SAP HANA username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "SAP HANA password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "host".to_string(),
                    description: "SAP HANA server hostname or IP address".to_string(),
                    field_type: FieldType::Hostname,
                    default_value: Some("localhost".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "database".to_string(),
                    description: "Database name (tenant database for MDC)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "port".to_string(),
                    description: "SAP HANA SQL port".to_string(),
                    field_type: FieldType::Port,
                    default_value: Some("30015".to_string()),
                    validation_regex: Some(r"^\d+$".to_string()),
                },
                ConfigField {
                    name: "schema".to_string(),
                    description: "Default schema (optional)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "instance_number".to_string(),
                    description: "SAP HANA instance number (00-99, optional)".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^[0-9]{2}$".to_string()),
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "in-memory".to_string(),
                "enterprise".to_string(),
            ],
            enabled: true,
        }
    }

    fn snowflake_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "snowflake".to_string(),
            name: "Snowflake".to_string(),
            version: "1.0.0".to_string(),
            description: "Snowflake cloud data warehouse connector with clustering and search optimization support".to_string(),
            source_type: "Snowflake".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: true,
                streaming: false,
                transactions: true,
                max_batch_size: Some(100000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "username".to_string(),
                    description: "Snowflake username".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: false,
                },
                CredentialField {
                    name: "password".to_string(),
                    description: "Snowflake password".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "account".to_string(),
                    description: "Snowflake account identifier".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^[a-zA-Z0-9_-]+$".to_string()),
                },
                ConfigField {
                    name: "warehouse".to_string(),
                    description: "Snowflake warehouse name".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
            ],
            registered_at: Utc::now(),
            tags: vec!["database".to_string(), "cloud".to_string(), "data-warehouse".to_string()],
            enabled: true,
        }
    }

    fn s3_parquet_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "s3_parquet".to_string(),
            name: "S3 Parquet".to_string(),
            version: "1.0.0".to_string(),
            description: "AWS S3 Parquet file connector".to_string(),
            source_type: "S3Parquet".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: true,
                streaming: true,
                transactions: false,
                max_batch_size: Some(50000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "access_key_id".to_string(),
                    description: "AWS Access Key ID".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
                CredentialField {
                    name: "secret_access_key".to_string(),
                    description: "AWS Secret Access Key".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![ConfigField {
                name: "region".to_string(),
                description: "AWS region".to_string(),
                field_type: FieldType::String,
                default_value: Some("us-east-1".to_string()),
                validation_regex: Some(r"^[a-z]{2}-[a-z]+-\d+$".to_string()),
            }],
            registered_at: Utc::now(),
            tags: vec![
                "file".to_string(),
                "cloud".to_string(),
                "columnar".to_string(),
            ],
            enabled: true,
        }
    }

    fn csv_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "csv".to_string(),
            name: "CSV File".to_string(),
            version: "1.0.0".to_string(),
            description: "CSV file connector".to_string(),
            source_type: "CsvFile".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: false,
                streaming: false,
                transactions: false,
                max_batch_size: Some(100000),
            },
            required_credentials: vec![],
            optional_config: vec![
                ConfigField {
                    name: "delimiter".to_string(),
                    description: "Field delimiter character".to_string(),
                    field_type: FieldType::String,
                    default_value: Some(",".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "has_header".to_string(),
                    description: "Whether the file has a header row".to_string(),
                    field_type: FieldType::Boolean,
                    default_value: Some("true".to_string()),
                    validation_regex: None,
                },
            ],
            registered_at: Utc::now(),
            tags: vec!["file".to_string(), "local".to_string()],
            enabled: true,
        }
    }

    fn rdf_ntriples_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "rdf_ntriples".to_string(),
            name: "RDF N-Triples".to_string(),
            version: "1.0.0".to_string(),
            description:
                "RDF N-Triples connector for importing semantic data into the governance RDF store"
                    .to_string(),
            source_type: "RDFNTriples".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: false,
                schema_inference: true,
                query_timeout: true,
                streaming: true,
                transactions: false,
                max_batch_size: Some(1000000),
            },
            required_credentials: vec![],
            optional_config: vec![
                ConfigField {
                    name: "source".to_string(),
                    description: "File path or URL to N-Triples data".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: Some(r"^(https?://|/|\./).*\.nt$".to_string()),
                },
                ConfigField {
                    name: "base_uri".to_string(),
                    description: "Base URI for resolving relative URIs (optional)".to_string(),
                    field_type: FieldType::Url,
                    default_value: None,
                    validation_regex: Some(r"^https?://.*$".to_string()),
                },
                ConfigField {
                    name: "target_graph".to_string(),
                    description: "Graph URI to import into (defaults to default graph)".to_string(),
                    field_type: FieldType::Url,
                    default_value: None,
                    validation_regex: Some(r"^https?://.*$".to_string()),
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "rdf".to_string(),
                "semantic".to_string(),
                "ontology".to_string(),
            ],
            enabled: true,
        }
    }

    fn databricks_metadata() -> ConnectorMetadata {
        ConnectorMetadata {
            id: "databricks".to_string(),
            name: "Databricks".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Databricks SQL Warehouse connector with connection testing, schema inference, and Statement API query execution"
                    .to_string(),
            source_type: "Databricks".to_string(),
            capabilities: ConnectorCapabilities {
                parameterized_queries: true,
                schema_inference: true,
                query_timeout: true,
                streaming: false,
                transactions: false,
                max_batch_size: Some(50000),
            },
            required_credentials: vec![
                CredentialField {
                    name: "token".to_string(),
                    description: "Databricks PAT token".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    sensitive: true,
                },
            ],
            optional_config: vec![
                ConfigField {
                    name: "workspaceUrl".to_string(),
                    description: "Databricks workspace URL".to_string(),
                    field_type: FieldType::Url,
                    default_value: None,
                    validation_regex: Some(r"^https://.+$".to_string()),
                },
                ConfigField {
                    name: "httpPath".to_string(),
                    description: "Databricks SQL endpoint HTTP path".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "warehouseId".to_string(),
                    description: "Optional Databricks SQL warehouse ID override".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                    validation_regex: None,
                },
                ConfigField {
                    name: "catalog".to_string(),
                    description: "Default Databricks catalog".to_string(),
                    field_type: FieldType::String,
                    default_value: Some("main".to_string()),
                    validation_regex: None,
                },
                ConfigField {
                    name: "schema".to_string(),
                    description: "Default Databricks schema".to_string(),
                    field_type: FieldType::String,
                    default_value: Some("default".to_string()),
                    validation_regex: None,
                },
            ],
            registered_at: Utc::now(),
            tags: vec![
                "database".to_string(),
                "cloud".to_string(),
                "lakehouse".to_string(),
            ],
            enabled: true,
        }
    }
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStatistics {
    pub total_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub by_category: HashMap<String, usize>,
    pub total_usage: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_connector_types() -> Vec<&'static str> {
        let mut types = vec![
            "PostgreSQL",
            "MySQL",
            "Snowflake",
            "Databricks",
            "S3Parquet",
            "CsvFile",
            "RDFNTriples",
        ];

        if cfg!(feature = "odbc") {
            types.extend(["Oracle", "DB2", "SAPHANA"]);
        }

        types
    }

    #[test]
    fn test_registry_creation() {
        let registry = ConnectorRegistry::new();
        for source_type in expected_connector_types() {
            assert!(registry.supports(source_type));
        }

        if !cfg!(feature = "odbc") {
            assert!(!registry.supports("Oracle"));
            assert!(!registry.supports("DB2"));
            assert!(!registry.supports("SAPHANA"));
        }
    }

    #[test]
    fn test_registry_get() {
        let registry = ConnectorRegistry::new();
        let connector = registry.get("PostgreSQL").unwrap();
        assert_eq!(connector.source_type(), "PostgreSQL");
    }

    #[test]
    fn test_registry_list() {
        let registry = ConnectorRegistry::new();
        let types = registry.list_types();
        assert_eq!(types.len(), expected_connector_types().len());
    }

    #[test]
    fn test_registry_not_found() {
        let registry = ConnectorRegistry::new();
        let result = registry.get("UnknownDB");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_connectors_with_metadata() {
        let registry = ConnectorRegistry::new();
        let connectors = registry.list_connectors();

        assert_eq!(connectors.len(), expected_connector_types().len());

        // Verify PostgreSQL metadata
        let pg = connectors.iter().find(|c| c.id == "postgresql").unwrap();
        assert_eq!(pg.name, "PostgreSQL");
        assert!(pg.description.contains("schema inference"));
        assert_eq!(pg.tags.len(), 3);
        assert!(pg.tags.contains(&"database".to_string()));
        assert!(pg.capabilities.transactions);
        assert_eq!(pg.required_credentials.len(), 2);
        assert_eq!(pg.optional_config.len(), 5); // host, database, port, schema, ssl_mode
    }

    #[test]
    fn test_get_metadata() {
        let registry = ConnectorRegistry::new();

        let metadata = registry.get_metadata("Snowflake").unwrap();
        assert_eq!(metadata.name, "Snowflake");
        assert!(metadata.description.contains("clustering"));
        assert!(!metadata.capabilities.streaming);
        assert_eq!(metadata.capabilities.max_batch_size, Some(100000));

        // Check Snowflake-specific config
        let account_config = metadata
            .optional_config
            .iter()
            .find(|c| c.name == "account")
            .unwrap();
        assert!(account_config.validation_regex.is_some());

        let dbx = registry.get_metadata("Databricks").unwrap();
        assert_eq!(dbx.name, "Databricks");
        assert!(dbx.tags.contains(&"lakehouse".to_string()));
        assert!(dbx
            .optional_config
            .iter()
            .any(|field| field.name == "workspaceUrl"));
        assert!(dbx
            .optional_config
            .iter()
            .any(|field| field.name == "httpPath"));
        assert!(dbx
            .optional_config
            .iter()
            .any(|field| field.name == "warehouseId"));
    }

    #[test]
    fn test_enable_disable_connector() {
        let mut registry = ConnectorRegistry::new();
        let total_count = expected_connector_types().len();

        assert_eq!(registry.list_enabled_connectors().len(), total_count);

        registry.disable_connector("PostgreSQL").unwrap();
        assert_eq!(registry.list_enabled_connectors().len(), total_count - 1);
        assert!(!registry.supports("PostgreSQL"));

        registry.enable_connector("PostgreSQL").unwrap();
        assert_eq!(registry.list_enabled_connectors().len(), total_count);
        assert!(registry.supports("PostgreSQL"));
    }

    #[test]
    fn test_registry_statistics() {
        let registry = ConnectorRegistry::new();
        let stats = registry.get_statistics();
        let total_count = expected_connector_types().len();

        assert_eq!(stats.total_count, total_count);
        assert_eq!(stats.enabled_count, total_count);
        assert_eq!(stats.disabled_count, 0);

        let expected_database_count = if cfg!(feature = "odbc") { 7 } else { 4 };
        assert_eq!(
            stats.by_category.get("database").copied(),
            Some(expected_database_count)
        );
        assert_eq!(stats.by_category.get("file").copied(), Some(2));

        assert_eq!(stats.total_usage, 0);
    }

    #[test]
    fn test_credential_field_metadata() {
        let registry = ConnectorRegistry::new();
        let metadata = registry.get_metadata("S3Parquet").unwrap();

        // S3 should require AWS credentials
        assert_eq!(metadata.required_credentials.len(), 2);

        let access_key = metadata
            .required_credentials
            .iter()
            .find(|c| c.name == "access_key_id")
            .unwrap();
        assert!(access_key.sensitive);
        assert!(access_key.required);

        let secret_key = metadata
            .required_credentials
            .iter()
            .find(|c| c.name == "secret_access_key")
            .unwrap();
        assert!(secret_key.sensitive);
    }

    #[test]
    fn test_csv_connector_has_no_credentials() {
        let registry = ConnectorRegistry::new();
        let metadata = registry.get_metadata("CsvFile").unwrap();

        // CSV connector doesn't require authentication
        assert_eq!(metadata.required_credentials.len(), 0);

        // But has config options
        assert!(metadata.optional_config.len() > 0);
        let delimiter = metadata
            .optional_config
            .iter()
            .find(|c| c.name == "delimiter")
            .unwrap();
        assert_eq!(delimiter.default_value, Some(",".to_string()));
    }
}
