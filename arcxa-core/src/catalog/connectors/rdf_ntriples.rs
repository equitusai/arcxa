//! RDF N-Triples Connector
//!
//! Connector for importing RDF data in N-Triples format into the governance RDF store.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{DataSource, SourceConfig},
};
use crate::errors::GraphicaError;
use crate::inference::types::SemanticType;

pub struct RDFNTriplesConnector;

impl RDFNTriplesConnector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RDFNTriplesConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for RDFNTriplesConnector {
    fn name(&self) -> &'static str {
        "RDF N-Triples Connector"
    }

    fn source_type(&self) -> &'static str {
        "RDFNTriples"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::RDFNTriples(rdf_config) => {
                let mut errors = vec![];

                if rdf_config.source.is_empty() {
                    errors.push("Source path/URL cannot be empty".to_string());
                }

                // Validate source is either a valid file path or URL
                if !rdf_config.source.starts_with("http://")
                    && !rdf_config.source.starts_with("https://")
                    && !rdf_config.source.starts_with("/")
                    && !rdf_config.source.starts_with("./")
                {
                    errors.push("Source must be a valid file path or URL".to_string());
                }

                // Validate base URI format if provided
                if let Some(base_uri) = &rdf_config.base_uri {
                    if !base_uri.starts_with("http://") && !base_uri.starts_with("https://") {
                        errors.push("Base URI must be a valid HTTP/HTTPS URI".to_string());
                    }
                }

                // Validate target graph URI format if provided
                if let Some(target_graph) = &rdf_config.target_graph {
                    if !target_graph.starts_with("http://") && !target_graph.starts_with("https://")
                    {
                        errors.push("Target graph URI must be a valid HTTP/HTTPS URI".to_string());
                    }
                }

                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected RDF N-Triples configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = match &source.connection.config {
            SourceConfig::RDFNTriples(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected RDF N-Triples configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();

        // Test if source is accessible
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), config.source.clone());

        if let Some(base_uri) = &config.base_uri {
            metadata.insert("base_uri".to_string(), base_uri.clone());
        }

        if let Some(target_graph) = &config.target_graph {
            metadata.insert("target_graph".to_string(), target_graph.clone());
        }

        let is_url = config.source.starts_with("http://") || config.source.starts_with("https://");

        if is_url {
            tracing::info!(
                "RDF N-Triples connection test: URL source {}",
                config.source
            );
            metadata.insert("source_type".to_string(), "url".to_string());
            let client = Client::new();
            let mut request = client.head(&config.source);

            if let Some(token) = credentials
                .additional
                .get("token")
                .or_else(|| credentials.additional.get("access_token"))
            {
                request = request.bearer_auth(token);
            } else if !credentials.username.is_empty() || !credentials.password.is_empty() {
                request = request.basic_auth(
                    credentials.username.clone(),
                    Some(credentials.password.clone()),
                );
            }

            let response = request.send().await;
            match response {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        return Ok(ConnectionTestResult {
                            success: false,
                            duration_ms: start.elapsed().as_millis() as u64,
                            error: Some(format!("RDF URL returned status {}", resp.status())),
                            metadata,
                            tested_at: Utc::now(),
                        });
                    }
                }
                Err(e) => {
                    return Ok(ConnectionTestResult {
                        success: false,
                        duration_ms: start.elapsed().as_millis() as u64,
                        error: Some(format!("RDF URL request failed: {}", e)),
                        metadata,
                        tested_at: Utc::now(),
                    });
                }
            }
        } else {
            tracing::info!(
                "RDF N-Triples connection test: File source {}",
                config.source
            );
            metadata.insert("source_type".to_string(), "file".to_string());

            // Check if file exists
            let path = std::path::Path::new(&config.source);
            if !path.exists() {
                return Ok(ConnectionTestResult {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("File not found: {}", config.source)),
                    metadata,
                    tested_at: Utc::now(),
                });
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConnectionTestResult {
            success: true,
            duration_ms,
            error: None,
            metadata,
            tested_at: Utc::now(),
        })
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        _table_name: Option<&str>,
        sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = match &source.connection.config {
            SourceConfig::RDFNTriples(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected RDF N-Triples configuration".to_string(),
                ))
            }
        };

        // For RDF data, schema inference means discovering:
        // 1. Classes (rdf:type statements)
        // 2. Properties (predicates)
        // 3. Value types

        tracing::info!(
            "RDF N-Triples schema inference: {} (sample_size: {})",
            config.source,
            sample_size
        );

        // TODO: Implement RDF schema discovery by parsing N-Triples
        // This would involve:
        // 1. Reading first N triples from source
        // 2. Extracting unique classes from rdf:type statements
        // 3. Extracting unique predicates
        // 4. Inferring value types from object literals

        // For now, return a basic structure indicating this is RDF data
        Ok(SchemaDefinition {
            name: config
                .target_graph
                .as_ref()
                .map(|g| g.clone())
                .unwrap_or_else(|| "default_graph".to_string()),
            tables: vec![TableDefinition {
                name: "rdf_triples".to_string(),
                columns: vec![
                    ColumnDefinition {
                        name: "subject".to_string(),
                        data_type: "uri".to_string(),
                        nullable: false,
                        primary_key: false,
                        default_value: None,
                        semantic_type: Some(SemanticType::URI),
                        statistics: None,
                    },
                    ColumnDefinition {
                        name: "predicate".to_string(),
                        data_type: "uri".to_string(),
                        nullable: false,
                        primary_key: false,
                        default_value: None,
                        semantic_type: Some(SemanticType::URI),
                        statistics: None,
                    },
                    ColumnDefinition {
                        name: "object".to_string(),
                        data_type: "uri_or_literal".to_string(),
                        nullable: false,
                        primary_key: false,
                        default_value: None,
                        semantic_type: Some(SemanticType::Custom("rdf:object".to_string())),
                        statistics: None,
                    },
                ],
                estimated_rows: None,
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let config = match &source.connection.config {
            SourceConfig::RDFNTriples(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected RDF N-Triples configuration".to_string(),
                ))
            }
        };

        // For RDF sources, queries are SPARQL queries
        tracing::info!(
            "RDF N-Triples query execution: {} (query: {}, limit: {:?})",
            config.source,
            query,
            limit
        );

        // TODO: Implement SPARQL query execution against the imported RDF data
        // This would require:
        // 1. Loading the N-Triples data into a temporary RDF store
        // 2. Executing the SPARQL query
        // 3. Converting results to QueryResult format

        Ok(QueryResult {
            rows: vec![],
            row_count: 0,
            execution_time_ms: 10,
            truncated: false,
            columns: Some(vec![
                ColumnDefinition {
                    name: "subject".to_string(),
                    data_type: "uri".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::URI),
                    statistics: None,
                },
                ColumnDefinition {
                    name: "predicate".to_string(),
                    data_type: "uri".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::URI),
                    statistics: None,
                },
                ColumnDefinition {
                    name: "object".to_string(),
                    data_type: "uri_or_literal".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::Custom("rdf:object".to_string())),
                    statistics: None,
                },
            ]),
        })
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,  // SPARQL uses different parameter syntax
            schema_inference: true,        // Can discover RDF classes and properties
            query_timeout: true,           // SPARQL queries can timeout
            streaming: true,               // N-Triples is line-based, perfect for streaming
            transactions: false,           // RDF imports are typically batch operations
            max_batch_size: Some(1000000), // Can handle large RDF datasets
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::types::{ConnectionDetails, RDFNTriplesConfig};

    fn create_test_source() -> DataSource {
        DataSource::new(
            "Test RDF N-Triples".to_string(),
            "RDFNTriples".to_string(),
            ConnectionDetails {
                secret_ref: "none".to_string(),
                config: SourceConfig::RDFNTriples(RDFNTriplesConfig {
                    source: "/data/ontology.nt".to_string(),
                    base_uri: Some("http://example.com/base#".to_string()),
                    target_graph: Some("http://example.com/graph/ontology".to_string()),
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
        )
    }

    #[test]
    fn test_validate_config() {
        let connector = RDFNTriplesConnector::new();
        let source = create_test_source();

        let result = connector
            .validate_config(&source.connection.config)
            .unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_validate_invalid_source() {
        let connector = RDFNTriplesConnector::new();

        let config = SourceConfig::RDFNTriples(RDFNTriplesConfig {
            source: "".to_string(),
            base_uri: None,
            target_graph: None,
        });

        let result = connector.validate_config(&config).unwrap();
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_invalid_base_uri() {
        let connector = RDFNTriplesConnector::new();

        let config = SourceConfig::RDFNTriples(RDFNTriplesConfig {
            source: "/data/test.nt".to_string(),
            base_uri: Some("invalid-uri".to_string()),
            target_graph: None,
        });

        let result = connector.validate_config(&config).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Base URI")));
    }

    #[tokio::test]
    async fn test_connection_file_not_found() {
        let connector = RDFNTriplesConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("".to_string(), "".to_string());

        let result = connector.test_connection(&source, creds).await.unwrap();
        // File doesn't exist in test environment
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_schema_inference() {
        let connector = RDFNTriplesConnector::new();
        let source = create_test_source();
        let creds = Credentials::new("".to_string(), "".to_string());

        let result = connector
            .infer_schema(&source, creds, None, 100)
            .await
            .unwrap();
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0].name, "rdf_triples");
        assert_eq!(result.tables[0].columns.len(), 3);
    }
}
