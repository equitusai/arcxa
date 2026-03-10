//! Data Source Connector Trait
//!
//! Defines the interface for connecting to and querying external data sources.

use super::api_types::{ConnectionTestResult, QueryResult, SchemaDefinition};
use super::types::{DataSource, SourceConfig};
use crate::errors::GraphicaError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result type for connector operations
pub type ConnectorResult<T> = Result<T, GraphicaError>;

/// Data source connector interface
///
/// Each connector implements connection logic for a specific source type.
/// Connectors handle:
/// - Connection pooling
/// - Authentication via secret providers
/// - Schema inference
/// - Query execution
/// - Result serialization to JSON
#[async_trait]
pub trait DataSourceConnector: Send + Sync {
    /// Connector name (e.g., "PostgreSQL", "Oracle")
    fn name(&self) -> &'static str;

    /// Connector version
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    /// Source type this connector handles
    fn source_type(&self) -> &'static str;

    /// Validate source configuration
    ///
    /// Checks that config has all required fields and valid values.
    /// Does NOT attempt connection.
    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult>;

    /// Test connection to data source
    ///
    /// Attempts to connect using provided credentials.
    /// Returns timing and success/error status.
    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult>;

    /// Infer schema from data source
    ///
    /// Discovers tables/collections and their columns/fields.
    /// May sample data to infer types.
    async fn infer_schema(
        &self,
        source: &DataSource,
        credentials: Credentials,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition>;

    /// Execute query against data source
    ///
    /// Runs query and returns results as JSON objects.
    /// Applies limit if specified.
    async fn execute_query(
        &self,
        source: &DataSource,
        credentials: Credentials,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult>;

    /// Get capabilities of this connector
    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities::default()
    }
}

/// Validation result for source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,

    /// Validation errors (if any)
    pub errors: Vec<String>,

    /// Validation warnings (non-fatal)
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: vec![],
        }
    }

    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }
}

/// Credentials for connecting to data source
///
/// Retrieved from secret provider (Vault, AWS Secrets Manager, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    /// Username
    pub username: String,

    /// Password
    pub password: String,

    /// Additional key-value credentials (e.g., API keys, tokens)
    #[serde(default)]
    pub additional: HashMap<String, String>,
}

impl Credentials {
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password,
            additional: HashMap::new(),
        }
    }

    pub fn with_additional(mut self, key: String, value: String) -> Self {
        self.additional.insert(key, value);
        self
    }
}

/// Connector capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorCapabilities {
    /// Supports parameterized queries
    pub parameterized_queries: bool,

    /// Supports schema inference
    pub schema_inference: bool,

    /// Supports query timeouts
    pub query_timeout: bool,

    /// Supports streaming results
    pub streaming: bool,

    /// Supports transactions
    pub transactions: bool,

    /// Maximum recommended batch size
    pub max_batch_size: Option<usize>,
}

impl Default for ConnectorCapabilities {
    fn default() -> Self {
        Self {
            parameterized_queries: true,
            schema_inference: true,
            query_timeout: true,
            streaming: false,
            transactions: false,
            max_batch_size: Some(10000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result() {
        let valid = ValidationResult::valid();
        assert!(valid.valid);
        assert!(valid.errors.is_empty());

        let invalid = ValidationResult::invalid(vec!["Error 1".to_string()]);
        assert!(!invalid.valid);
        assert_eq!(invalid.errors.len(), 1);
    }

    #[test]
    fn test_credentials() {
        let creds = Credentials::new("user".to_string(), "pass".to_string())
            .with_additional("api_key".to_string(), "key123".to_string());

        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
        assert_eq!(creds.additional.get("api_key"), Some(&"key123".to_string()));
    }

    #[test]
    fn test_capabilities_default() {
        let caps = ConnectorCapabilities::default();
        assert!(caps.parameterized_queries);
        assert!(caps.schema_inference);
        assert_eq!(caps.max_batch_size, Some(10000));
    }
}
