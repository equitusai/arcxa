//! Data Source Catalog Client Interface
//!
//! Trait definitions for interacting with the data source catalog.
//! This provides a storage-agnostic interface that can be implemented
//! by different backends (RDF store, mock for testing, etc.).

use super::api_types::{
    ConnectionTestResult, DataSourceResponse, DataSourceStatus, ListDataSourcesRequest,
    ListDataSourcesResponse, QueryResult, SchemaDefinition, UpdateDataSourcePatch,
};
use super::types::DataSource;
use crate::errors::GraphicaError;
use async_trait::async_trait;
use std::collections::HashMap;

/// Result type for catalog operations
pub type CatalogResult<T> = Result<T, GraphicaError>;

/// Data source catalog interface
///
/// This trait abstracts the catalog storage backend, allowing different implementations:
/// - RDF store backend (primary implementation)
/// - In-memory mock (for testing)
/// - Remote catalog service (for distributed scenarios)
#[async_trait]
pub trait DataSourceCatalog: Send + Sync {
    /// Register a new data source in the catalog
    ///
    /// Creates RDF triples in the governance graph with DCAT metadata.
    ///
    /// # Arguments
    /// * `source` - The data source to register
    ///
    /// # Returns
    /// The registered data source with generated ID and timestamps
    async fn register_source(&self, source: DataSource) -> CatalogResult<DataSourceResponse>;

    /// Retrieve a data source by ID
    ///
    /// # Arguments
    /// * `id` - The data source URN (e.g., "urn:graphica:datasource:uuid")
    ///
    /// # Returns
    /// The data source if found
    async fn get_source(&self, id: &str) -> CatalogResult<DataSourceResponse>;

    /// Update an existing data source
    ///
    /// Updates RDF triples in the governance graph.
    ///
    /// # Arguments
    /// * `id` - The data source URN
    /// * `updates` - Map of field names to new values
    ///
    /// # Returns
    /// The updated data source
    async fn update_source(
        &self,
        id: &str,
        updates: UpdateDataSourcePatch,
    ) -> CatalogResult<DataSourceResponse>;

    /// Delete a data source from the catalog
    ///
    /// Soft-deletes by marking as deleted in RDF graph (retains lineage).
    ///
    /// # Arguments
    /// * `id` - The data source URN
    async fn delete_source(&self, id: &str) -> CatalogResult<()>;

    /// List data sources with optional filtering
    ///
    /// Executes SPARQL query against the catalog graph.
    ///
    /// # Arguments
    /// * `request` - Filter criteria and pagination
    ///
    /// # Returns
    /// Paginated list of data sources
    async fn list_sources(
        &self,
        request: &ListDataSourcesRequest,
    ) -> CatalogResult<ListDataSourcesResponse>;

    /// Test connection to a data source
    ///
    /// Validates that the source is reachable and credentials are correct.
    /// Uses the appropriate connector based on source type.
    ///
    /// # Arguments
    /// * `id` - The data source URN
    ///
    /// # Returns
    /// Connection test result with timing and status
    async fn test_connection(&self, id: &str) -> CatalogResult<ConnectionTestResult>;

    /// Infer schema from a data source
    ///
    /// Connects to the source and discovers tables/columns.
    /// Results can be stored in the RDF graph as schema metadata.
    ///
    /// # Arguments
    /// * `id` - The data source URN
    /// * `table_name` - Optional specific table to infer
    /// * `sample_size` - Number of rows to sample for type inference
    ///
    /// # Returns
    /// Discovered schema definition
    async fn infer_schema(
        &self,
        id: &str,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> CatalogResult<SchemaDefinition>;

    /// Execute a query against a data source
    ///
    /// Runs an ad-hoc query for exploration/testing purposes.
    /// NOT intended for production data ingestion (use WorkflowInput for that).
    ///
    /// # Arguments
    /// * `id` - The data source URN
    /// * `query` - Query string (SQL, etc.)
    /// * `parameters` - Query parameters
    /// * `limit` - Maximum rows to return
    ///
    /// # Returns
    /// Query results as JSON objects
    async fn execute_query(
        &self,
        id: &str,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> CatalogResult<QueryResult>;

    /// Mark a data source as synced
    ///
    /// Updates the lastSyncedAt timestamp after successful data ingestion.
    ///
    /// # Arguments
    /// * `id` - The data source URN
    async fn mark_synced(&self, id: &str) -> CatalogResult<()>;

    /// Update data source status
    ///
    /// Changes operational status (active, error, disabled, etc.).
    ///
    /// # Arguments
    /// * `id` - The data source URN
    /// * `status` - New status
    /// * `error_message` - Optional error message if status is Error
    async fn update_status(
        &self,
        id: &str,
        status: DataSourceStatus,
        error_message: Option<String>,
    ) -> CatalogResult<()>;

    /// Search data sources by text
    ///
    /// Full-text search across title, description, tags, metadata.
    ///
    /// # Arguments
    /// * `query` - Search query
    /// * `limit` - Maximum results
    ///
    /// # Returns
    /// Matching data sources ordered by relevance
    async fn search_sources(
        &self,
        query: &str,
        limit: usize,
    ) -> CatalogResult<Vec<DataSourceResponse>>;

    /// Get data sources by tag
    ///
    /// Finds all sources with a specific tag.
    ///
    /// # Arguments
    /// * `tag` - Tag to filter by
    ///
    /// # Returns
    /// Data sources with the tag
    async fn get_sources_by_tag(&self, tag: &str) -> CatalogResult<Vec<DataSourceResponse>>;

    /// Get usage statistics for a data source
    ///
    /// Queries lineage graph to find workflows using this source.
    ///
    /// # Arguments
    /// * `id` - The data source URN
    ///
    /// # Returns
    /// Usage statistics (workflow count, last used, etc.)
    async fn get_usage_stats(&self, id: &str) -> CatalogResult<UsageStatistics>;

    /// Retrieve a data source by title
    ///
    /// Searches for a data source matching the given title.
    /// Useful for workflows that reference sources by human-readable name.
    ///
    /// # Arguments
    /// * `title` - The data source title (e.g., "db2_professional_demo")
    ///
    /// # Returns
    /// The data source if found
    async fn get_source_by_title(&self, title: &str) -> CatalogResult<DataSourceResponse>;
}

/// Data source usage statistics
#[derive(Debug, Clone)]
pub struct UsageStatistics {
    /// Number of workflows using this source
    pub workflow_count: usize,

    /// Last time this source was used in a workflow
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,

    /// Total records processed from this source (all time)
    pub total_records_processed: u64,

    /// List of workflow IDs using this source
    pub workflow_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Mock catalog implementation for testing
    struct MockCatalog {
        sources: std::sync::Mutex<HashMap<String, DataSourceResponse>>,
    }

    impl MockCatalog {
        fn new() -> Self {
            Self {
                sources: std::sync::Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl DataSourceCatalog for MockCatalog {
        async fn register_source(&self, source: DataSource) -> CatalogResult<DataSourceResponse> {
            let response = DataSourceResponse {
                source: source.clone(),
                status: DataSourceStatus::Unverified,
                last_test_result: None,
                capabilities: None,
            };

            let mut sources = self.sources.lock().unwrap();
            sources.insert(source.id.clone(), response.clone());

            Ok(response)
        }

        async fn get_source(&self, id: &str) -> CatalogResult<DataSourceResponse> {
            let sources = self.sources.lock().unwrap();
            sources
                .get(id)
                .cloned()
                .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))
        }

        async fn update_source(
            &self,
            id: &str,
            _updates: UpdateDataSourcePatch,
        ) -> CatalogResult<DataSourceResponse> {
            self.get_source(id).await
        }

        async fn delete_source(&self, id: &str) -> CatalogResult<()> {
            let mut sources = self.sources.lock().unwrap();
            sources.remove(id);
            Ok(())
        }

        async fn list_sources(
            &self,
            _request: &ListDataSourcesRequest,
        ) -> CatalogResult<ListDataSourcesResponse> {
            let sources = self.sources.lock().unwrap();
            Ok(ListDataSourcesResponse {
                sources: sources.values().cloned().collect(),
                total: sources.len(),
                page: 0,
                page_size: 50,
            })
        }

        async fn test_connection(&self, _id: &str) -> CatalogResult<ConnectionTestResult> {
            Ok(ConnectionTestResult {
                success: true,
                duration_ms: 100,
                error: None,
                metadata: HashMap::new(),
                tested_at: chrono::Utc::now(),
            })
        }

        async fn infer_schema(
            &self,
            _id: &str,
            _table_name: Option<&str>,
            _sample_size: usize,
        ) -> CatalogResult<SchemaDefinition> {
            Ok(SchemaDefinition {
                name: "public".to_string(),
                tables: vec![],
                relationships: vec![],
                indexes: vec![],
                inferred_at: chrono::Utc::now(),
            })
        }

        async fn execute_query(
            &self,
            _id: &str,
            _query: &str,
            _parameters: HashMap<String, serde_json::Value>,
            _limit: Option<usize>,
        ) -> CatalogResult<QueryResult> {
            Ok(QueryResult {
                rows: vec![],
                row_count: 0,
                execution_time_ms: 10,
                truncated: false,
                columns: None,
            })
        }

        async fn mark_synced(&self, _id: &str) -> CatalogResult<()> {
            Ok(())
        }

        async fn update_status(
            &self,
            _id: &str,
            _status: DataSourceStatus,
            _error_message: Option<String>,
        ) -> CatalogResult<()> {
            Ok(())
        }

        async fn search_sources(
            &self,
            _query: &str,
            _limit: usize,
        ) -> CatalogResult<Vec<DataSourceResponse>> {
            Ok(vec![])
        }

        async fn get_sources_by_tag(&self, _tag: &str) -> CatalogResult<Vec<DataSourceResponse>> {
            Ok(vec![])
        }

        async fn get_usage_stats(&self, _id: &str) -> CatalogResult<UsageStatistics> {
            Ok(UsageStatistics {
                workflow_count: 0,
                last_used: None,
                total_records_processed: 0,
                workflow_ids: vec![],
            })
        }

        async fn get_source_by_title(&self, title: &str) -> CatalogResult<DataSourceResponse> {
            let sources = self.sources.lock().unwrap();
            sources
                .values()
                .find(|s| s.source.title == title)
                .cloned()
                .ok_or_else(|| {
                    GraphicaError::NotFound(format!("Data source not found with title: {}", title))
                })
        }
    }

    #[tokio::test]
    async fn test_mock_catalog_register_and_get() {
        let catalog = Arc::new(MockCatalog::new());

        let source = DataSource::new(
            "Test Source".to_string(),
            "PostgreSQL".to_string(),
            super::super::types::ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: super::super::types::SourceConfig::PostgreSQL(
                    super::super::types::PostgreSQLConfig {
                        host: "localhost".to_string(),
                        port: 5432,
                        database: "test".to_string(),
                        schema: None,
                        ssl_mode: None,
                    },
                ),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        // Register source
        let response = catalog.register_source(source.clone()).await.unwrap();
        assert_eq!(response.status, DataSourceStatus::Unverified);

        // Retrieve source
        let retrieved = catalog.get_source(&source.id).await.unwrap();
        assert_eq!(retrieved.source.title, "Test Source");
    }

    #[tokio::test]
    async fn test_mock_catalog_delete() {
        let catalog = Arc::new(MockCatalog::new());

        let source = DataSource::new(
            "Test Source".to_string(),
            "PostgreSQL".to_string(),
            super::super::types::ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: super::super::types::SourceConfig::PostgreSQL(
                    super::super::types::PostgreSQLConfig {
                        host: "localhost".to_string(),
                        port: 5432,
                        database: "test".to_string(),
                        schema: None,
                        ssl_mode: None,
                    },
                ),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        // Register and delete
        let response = catalog.register_source(source.clone()).await.unwrap();
        catalog.delete_source(&response.source.id).await.unwrap();

        // Verify deleted
        let result = catalog.get_source(&response.source.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_catalog_list() {
        let catalog = Arc::new(MockCatalog::new());

        // Register multiple sources
        for i in 0..3 {
            let source = DataSource::new(
                format!("Source {}", i),
                "PostgreSQL".to_string(),
                super::super::types::ConnectionDetails {
                    secret_ref: "vault://test".to_string(),
                    config: super::super::types::SourceConfig::PostgreSQL(
                        super::super::types::PostgreSQLConfig {
                            host: "localhost".to_string(),
                            port: 5432,
                            database: "test".to_string(),
                            schema: None,
                            ssl_mode: None,
                        },
                    ),
                    encryption_enabled: true,
                    credentials: Default::default(),
                },
            );
            catalog.register_source(source).await.unwrap();
        }

        // List sources
        let request = ListDataSourcesRequest::default();
        let response = catalog.list_sources(&request).await.unwrap();
        assert_eq!(response.total, 3);
        assert_eq!(response.sources.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_catalog_connection_test() {
        let catalog = Arc::new(MockCatalog::new());

        let result = catalog.test_connection("dummy_id").await.unwrap();
        assert!(result.success);
        assert_eq!(result.duration_ms, 100);
    }
}
