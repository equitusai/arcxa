//! Data Source Catalog Types
//!
//! Core types for data source configuration and metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Data source configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataSource {
    /// Unique identifier (URN format: urn:graphica:datasource:uuid)
    #[serde(rename = "@id")]
    pub id: String,

    /// Human-readable title
    pub title: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Source type (PostgreSQL, Oracle, etc.)
    #[serde(rename = "sourceType")]
    pub source_type: String,

    /// Connection configuration
    pub connection: ConnectionDetails,

    /// Optional schema reference (URN to schema in graph)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "schemaRef")]
    pub schema_ref: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,

    /// Last updated timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,

    /// Last successful sync timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "lastSyncedAt")]
    pub last_synced_at: Option<DateTime<Utc>>,
}

/// Connection details for a data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectionDetails {
    /// Reference to secret store (vault://..., aws://..., etc.)
    #[serde(rename = "secretRef")]
    pub secret_ref: String,

    /// Source-specific configuration (non-sensitive)
    pub config: SourceConfig,

    /// Whether TLS/SSL encryption is enabled
    #[serde(rename = "encryptionEnabled")]
    pub encryption_enabled: bool,

    /// Inline credentials (development/testing only)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub credentials: HashMap<String, String>,
}

/// Source-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum SourceConfig {
    #[serde(rename = "PostgreSQL")]
    PostgreSQL(PostgreSQLConfig),

    #[serde(rename = "MySQL")]
    MySQL(MySQLConfig),

    #[serde(rename = "Oracle")]
    Oracle(OracleConfig),

    #[serde(rename = "DB2", alias = "IBM DB2")]
    DB2(DB2Config),

    #[serde(rename = "SAPHANA")]
    SAPHANA(SAPHANAConfig),

    #[serde(rename = "Snowflake")]
    Snowflake(SnowflakeConfig),

    #[serde(rename = "Databricks")]
    Databricks(DatabricksConfig),

    #[serde(rename = "S3Parquet")]
    S3Parquet(S3ParquetConfig),

    #[serde(rename = "CsvFile")]
    CsvFile(CsvFileConfig),

    #[serde(rename = "RDFNTriples")]
    RDFNTriples(RDFNTriplesConfig),
}

/// PostgreSQL connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostgreSQLConfig {
    pub host: String,
    #[serde(default = "default_postgres_port")]
    pub port: u16,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sslMode")]
    pub ssl_mode: Option<String>,
}

fn default_postgres_port() -> u16 {
    5432
}

/// MySQL connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MySQLConfig {
    pub host: String,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sslMode")]
    pub ssl_mode: Option<String>,
}

fn default_mysql_port() -> u16 {
    3306
}

/// Oracle connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OracleConfig {
    pub host: String,
    #[serde(default = "default_oracle_port")]
    pub port: u16,
    /// Service name or SID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "serviceName")]
    #[serde(alias = "service_name")]
    pub service_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

fn default_oracle_port() -> u16 {
    1521
}

/// IBM DB2 connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DB2Config {
    pub host: String,
    #[serde(default = "default_db2_port")]
    pub port: u16,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

fn default_db2_port() -> u16 {
    50000
}

/// SAP HANA connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SAPHANAConfig {
    pub host: String,
    #[serde(default = "default_hana_port")]
    pub port: u16,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "instanceNumber")]
    pub instance_number: Option<String>,
}

fn default_hana_port() -> u16 {
    30015
}

/// Snowflake connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SnowflakeConfig {
    /// Account identifier (e.g., "xy12345.us-east-1")
    pub account: String,
    pub warehouse: String,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Databricks SQL Warehouse connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatabricksConfig {
    /// Workspace URL (e.g., "https://adb-1234567890123456.7.azuredatabricks.net")
    #[serde(rename = "workspaceUrl")]
    #[serde(alias = "workspace_url")]
    pub workspace_url: String,
    /// SQL endpoint HTTP path
    #[serde(rename = "httpPath")]
    #[serde(alias = "http_path")]
    pub http_path: String,
    /// Optional catalog name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    /// Optional schema name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Optional warehouse ID for governance/executor references
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "warehouseId")]
    #[serde(alias = "warehouse_id")]
    pub warehouse_id: Option<String>,
}

/// S3 Parquet connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct S3ParquetConfig {
    pub bucket: String,
    #[serde(rename = "pathPrefix")]
    pub path_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    #[serde(rename = "partitionColumns")]
    pub partition_columns: Vec<String>,
}

/// CSV file connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CsvFileConfig {
    /// File path or URL
    pub path: String,
    #[serde(default = "default_csv_delimiter")]
    pub delimiter: char,
    #[serde(default = "default_csv_has_header")]
    #[serde(rename = "hasHeader")]
    pub has_header: bool,
}

fn default_csv_delimiter() -> char {
    ','
}

fn default_csv_has_header() -> bool {
    true
}

/// RDF N-Triples connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RDFNTriplesConfig {
    /// File path or URL to N-Triples data
    pub source: String,
    /// Optional base URI for resolving relative URIs
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "baseUri")]
    pub base_uri: Option<String>,
    /// Graph URI to import into (defaults to default graph)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "targetGraph")]
    pub target_graph: Option<String>,
}

impl SourceConfig {
    /// Normalize source-specific configuration in place.
    pub fn normalize(&mut self) {
        if let SourceConfig::Oracle(config) = self {
            config.normalize();
        }
    }

    /// Validate configuration for this source type
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        match self {
            SourceConfig::PostgreSQL(config) => {
                if config.host.is_empty() {
                    errors.push("PostgreSQL host cannot be empty".to_string());
                }
                if config.database.is_empty() {
                    errors.push("PostgreSQL database cannot be empty".to_string());
                }
            }
            SourceConfig::MySQL(config) => {
                if config.host.is_empty() {
                    errors.push("MySQL host cannot be empty".to_string());
                }
                if config.database.is_empty() {
                    errors.push("MySQL database cannot be empty".to_string());
                }
            }
            SourceConfig::Oracle(config) => {
                if config.host.is_empty() {
                    errors.push("Oracle host cannot be empty".to_string());
                }
                if config.resolved_target().is_none() {
                    errors.push("Oracle requires either serviceName or sid".to_string());
                }
            }
            SourceConfig::DB2(config) => {
                if config.host.is_empty() {
                    errors.push("DB2 host cannot be empty".to_string());
                }
                if config.database.is_empty() {
                    errors.push("DB2 database cannot be empty".to_string());
                }
            }
            SourceConfig::SAPHANA(config) => {
                if config.host.is_empty() {
                    errors.push("SAP HANA host cannot be empty".to_string());
                }
                if config.database.is_empty() {
                    errors.push("SAP HANA database cannot be empty".to_string());
                }
            }
            SourceConfig::Snowflake(config) => {
                if config.account.is_empty() {
                    errors.push("Snowflake account cannot be empty".to_string());
                }
                if config.warehouse.is_empty() {
                    errors.push("Snowflake warehouse cannot be empty".to_string());
                }
                if config.database.is_empty() {
                    errors.push("Snowflake database cannot be empty".to_string());
                }
            }
            SourceConfig::Databricks(config) => {
                if config.workspace_url.is_empty() {
                    errors.push("Databricks workspaceUrl cannot be empty".to_string());
                }
                if config.http_path.is_empty() {
                    errors.push("Databricks httpPath cannot be empty".to_string());
                }
                if !config.workspace_url.starts_with("https://") {
                    errors.push("Databricks workspaceUrl must start with https://".to_string());
                }
            }
            SourceConfig::S3Parquet(config) => {
                if config.bucket.is_empty() {
                    errors.push("S3 bucket cannot be empty".to_string());
                }
                if config.path_prefix.is_empty() {
                    errors.push("S3 path prefix cannot be empty".to_string());
                }
            }
            SourceConfig::CsvFile(config) => {
                if config.path.is_empty() {
                    errors.push("CSV file path cannot be empty".to_string());
                }
            }
            SourceConfig::RDFNTriples(config) => {
                if config.source.is_empty() {
                    errors.push("RDF N-Triples source cannot be empty".to_string());
                }
                // Validate source is either a valid file path or URL
                if !config.source.starts_with("http://")
                    && !config.source.starts_with("https://")
                    && !config.source.starts_with("/")
                    && !config.source.starts_with("./")
                {
                    errors
                        .push("RDF N-Triples source must be a valid file path or URL".to_string());
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get the source type name
    pub fn source_type(&self) -> &'static str {
        match self {
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
        }
    }

    /// Check whether an external source type string refers to this config variant.
    pub fn matches_source_type_name(&self, source_type: &str) -> bool {
        normalize_source_type_name(source_type)
            .map(|normalized| normalized == self.source_type())
            .unwrap_or(false)
    }
}

/// Normalize user-supplied source type strings to canonical catalog names.
pub fn normalize_source_type_name(source_type: &str) -> Option<&'static str> {
    let normalized = source_type
        .trim()
        .to_lowercase()
        .replace([' ', '-', '_'], "");

    match normalized.as_str() {
        "postgresql" | "postgres" | "pg" => Some("PostgreSQL"),
        "mysql" => Some("MySQL"),
        "oracle" => Some("Oracle"),
        "db2" | "ibmdb2" => Some("DB2"),
        "saphana" | "hana" => Some("SAPHANA"),
        "snowflake" => Some("Snowflake"),
        "databricks" => Some("Databricks"),
        "s3parquet" | "parquet" => Some("S3Parquet"),
        "csvfile" | "csv" => Some("CsvFile"),
        "rdfntriples" | "ntriples" | "rdf" => Some("RDFNTriples"),
        _ => None,
    }
}

impl DataSource {
    /// Create a new data source with generated ID
    pub fn new(title: String, _source_type: String, mut connection: ConnectionDetails) -> Self {
        connection.config.normalize();
        let id = format!("urn:graphica:datasource:{}", uuid::Uuid::new_v4());
        let source_type = connection.config.source_type().to_string();

        Self {
            id,
            title,
            description: None,
            source_type,
            connection,
            schema_ref: None,
            tags: Vec::new(),
            metadata: HashMap::new(),
            created_at: Some(Utc::now()),
            updated_at: None,
            last_synced_at: None,
        }
    }

    /// Validate the complete data source configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let expected_source_type = self.connection.config.source_type();

        if self.title.is_empty() {
            errors.push("Title cannot be empty".to_string());
        }

        if self.source_type.is_empty() {
            errors.push("Source type cannot be empty".to_string());
        }

        if self.connection.secret_ref.is_empty() && self.connection.credentials.is_empty() {
            errors.push(
                "Secret reference cannot be empty unless credentials are provided".to_string(),
            );
        }

        // Validate source-specific configuration
        if let Err(config_errors) = self.connection.config.validate() {
            errors.extend(config_errors);
        }

        if !self
            .connection
            .config
            .matches_source_type_name(&self.source_type)
        {
            errors.push(format!(
                "Source type '{}' does not match connection type '{}'",
                self.source_type, expected_source_type
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_config_validation() {
        let config = SourceConfig::PostgreSQL(PostgreSQLConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            schema: None,
            ssl_mode: None,
        });

        assert!(config.validate().is_ok());

        // Test empty host
        let invalid_config = SourceConfig::PostgreSQL(PostgreSQLConfig {
            host: "".to_string(),
            port: 5432,
            database: "test".to_string(),
            schema: None,
            ssl_mode: None,
        });

        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_oracle_config_validation() {
        // Valid with service name
        let config = SourceConfig::Oracle(OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: Some("ORCL".to_string()),
            sid: None,
            schema: None,
        });

        assert!(config.validate().is_ok());

        // Invalid: neither service name nor SID
        let invalid_config = SourceConfig::Oracle(OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: None,
            sid: None,
            schema: None,
        });

        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_oracle_config_accepts_legacy_service_name_alias() {
        let config: OracleConfig = serde_json::from_value(serde_json::json!({
            "host": "localhost",
            "port": 1521,
            "service_name": "ORCL"
        }))
        .unwrap();

        assert_eq!(config.service_name.as_deref(), Some("ORCL"));
    }

    #[test]
    fn test_oracle_config_validation_rejects_blank_service_name_without_sid() {
        let config = SourceConfig::Oracle(OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: Some("   ".to_string()),
            sid: None,
            schema: None,
        });

        let errors = config.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("serviceName or sid")));
    }

    #[test]
    fn test_oracle_config_blank_service_name_is_normalized_on_deserialize() {
        let config: OracleConfig = serde_json::from_value(serde_json::json!({
            "host": "localhost",
            "port": 1521,
            "serviceName": "   ",
            "sid": "XE",
            "schema": " "
        }))
        .unwrap();

        let normalized = config.normalized();
        assert_eq!(normalized.service_name, None);
        assert_eq!(normalized.sid.as_deref(), Some("XE"));
        assert_eq!(normalized.schema, None);
    }

    #[test]
    fn test_data_source_new_normalizes_oracle_config() {
        let source = DataSource::new(
            "Oracle DS".to_string(),
            "Oracle".to_string(),
            ConnectionDetails {
                secret_ref: "vault://oracle".to_string(),
                config: SourceConfig::Oracle(OracleConfig {
                    host: "localhost".to_string(),
                    port: 1521,
                    service_name: Some(String::new()),
                    sid: Some("XE".to_string()),
                    schema: Some(" ".to_string()),
                }),
                encryption_enabled: false,
                credentials: HashMap::new(),
            },
        );

        match source.connection.config {
            SourceConfig::Oracle(config) => {
                assert_eq!(config.service_name, None);
                assert_eq!(config.sid.as_deref(), Some("XE"));
                assert_eq!(config.schema, None);
            }
            other => panic!("expected Oracle config, got {:?}", other),
        }
    }

    #[test]
    fn test_snowflake_config_validation() {
        let config = SourceConfig::Snowflake(SnowflakeConfig {
            account: "xy12345.us-east-1".to_string(),
            warehouse: "COMPUTE_WH".to_string(),
            database: "PROD_DB".to_string(),
            schema: Some("PUBLIC".to_string()),
            role: Some("ANALYST".to_string()),
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_databricks_config_validation() {
        let config = SourceConfig::Databricks(DatabricksConfig {
            workspace_url: "https://adb-12345.6.azuredatabricks.net".to_string(),
            http_path: "/sql/1.0/warehouses/abc123".to_string(),
            catalog: Some("main".to_string()),
            schema: Some("default".to_string()),
            warehouse_id: Some("abc123".to_string()),
        });

        assert!(config.validate().is_ok());

        let invalid = SourceConfig::Databricks(DatabricksConfig {
            workspace_url: "http://insecure.example.com".to_string(),
            http_path: "".to_string(),
            catalog: None,
            schema: None,
            warehouse_id: None,
        });

        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_databricks_config_accepts_legacy_snake_case_aliases() {
        let payload = serde_json::json!({
            "workspace_url": "https://adb-12345.6.azuredatabricks.net",
            "http_path": "/sql/1.0/warehouses/abc123",
            "catalog": "main",
            "schema": "default",
            "warehouse_id": "abc123"
        });

        let config: DatabricksConfig = serde_json::from_value(payload).unwrap();
        assert_eq!(
            config.workspace_url,
            "https://adb-12345.6.azuredatabricks.net"
        );
        assert_eq!(config.http_path, "/sql/1.0/warehouses/abc123");
        assert_eq!(config.warehouse_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_normalize_source_type_name() {
        assert_eq!(normalize_source_type_name("postgres"), Some("PostgreSQL"));
        assert_eq!(normalize_source_type_name("IBM DB2"), Some("DB2"));
        assert_eq!(normalize_source_type_name("sap_hana"), Some("SAPHANA"));
        assert_eq!(normalize_source_type_name("csv"), Some("CsvFile"));
        assert_eq!(normalize_source_type_name("unknown"), None);
    }

    #[test]
    fn test_data_source_creation() {
        let connection = ConnectionDetails {
            secret_ref: "vault://secrets/postgres_test".to_string(),
            config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                schema: None,
                ssl_mode: None,
            }),
            encryption_enabled: true,
            credentials: Default::default(),
        };

        let source = DataSource::new(
            "Test PostgreSQL".to_string(),
            "PostgreSQL".to_string(),
            connection,
        );

        assert!(source.id.starts_with("urn:graphica:datasource:"));
        assert_eq!(source.title, "Test PostgreSQL");
        assert!(source.created_at.is_some());
        assert!(source.validate().is_ok());
    }

    #[test]
    fn test_data_source_validation() {
        let connection = ConnectionDetails {
            secret_ref: "vault://secrets/test".to_string(),
            config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "".to_string(), // Invalid: empty host
                port: 5432,
                database: "test".to_string(),
                schema: None,
                ssl_mode: None,
            }),
            encryption_enabled: true,
            credentials: Default::default(),
        };

        let source = DataSource::new("Test".to_string(), "PostgreSQL".to_string(), connection);

        let result = source.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("host")));
    }

    #[test]
    fn test_data_source_validation_rejects_source_type_mismatch() {
        let connection = ConnectionDetails {
            secret_ref: "vault://secrets/test".to_string(),
            config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                schema: None,
                ssl_mode: None,
            }),
            encryption_enabled: true,
            credentials: Default::default(),
        };

        let mut source = DataSource::new("Test".to_string(), "PostgreSQL".to_string(), connection);
        source.source_type = "Oracle".to_string();

        let errors = source.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("does not match connection type")));
    }

    #[test]
    fn test_source_config_serialization() {
        let config = SourceConfig::Snowflake(SnowflakeConfig {
            account: "test".to_string(),
            warehouse: "WH".to_string(),
            database: "DB".to_string(),
            schema: None,
            role: None,
        });

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"Snowflake\""));
        assert!(json.contains("\"account\":\"test\""));

        // Test deserialization
        let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source_type(), "Snowflake");
    }

    #[test]
    fn test_source_config_db2_alias_deserialization() {
        let json = r#"{
            "type": "IBM DB2",
            "host": "localhost",
            "port": 50000,
            "database": "SAMPLE",
            "schema": null
        }"#;

        let deserialized: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.source_type(), "DB2");
    }
}
