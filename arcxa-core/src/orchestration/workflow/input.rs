//! Workflow Input System - SPARQL-First Data Targeting
//!
//! This module provides flexible input mechanisms for workflows, enabling
//! graph-native data selection via SPARQL queries, entity filters, and
//! streaming sources.
//!
//! ## Design Philosophy
//!
//! For an RDF/semantic graph platform, workflows should operate on **graph data**
//! rather than arbitrary JSON blobs. This module bridges that gap by providing:
//!
//! 1. **SPARQL Query Input**: Query the graph directly for workflow input
//! 2. **Entity Filter Input**: Type-based filtering with time ranges
//! 3. **Legacy JSON Input**: Backward compatibility
//! 4. **Streaming Input** (future): Real-time CDC subscriptions
//!
//! ## Architecture
//!
//! ```text
//! WorkflowInput
//!     │
//!     ├─> SparqlQuery ──> QueryAdapter ──> ExecutionContext
//!     ├─> EntityFilter ──> FilterAdapter ──> ExecutionContext
//!     ├─> Json ─────────> JsonAdapter ───> ExecutionContext
//!     ├─> Dataset ──────> DatasetAdapter ─> ExecutionContext
//!     └─> GraphStream ──> StreamAdapter ─> ExecutionContext (future)
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use graphica_core::orchestration::workflow::input::*;
//!
//! // SPARQL query input
//! let input = WorkflowInput::SparqlQuery {
//!     query: "SELECT ?customer ?name WHERE { ?customer a gph:Customer }".to_string(),
//!     graph: Some("http://graphica.io/latest".to_string()),
//!     batch_size: Some(100),
//!     limit: Some(1000),
//! };
//!
//! // Entity filter input
//! let input = WorkflowInput::EntityFilter {
//!     entity_type: "gph:Customer".to_string(),
//!     graph: None,
//!     created_after: Some("2025-10-01T00:00:00Z".to_string()),
//!     updated_after: None,
//!     limit: Some(1000),
//!     batch_size: Some(100),
//! };
//!
//! // Legacy JSON input (backward compatible)
//! let input = WorkflowInput::Json {
//!     data: serde_json::json!({"key": "value"}),
//! };
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use utoipa::ToSchema;

use super::executor::ExecutionContext;

/// Workflow input specification
///
/// Defines how a workflow should select its input data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowInput {
    /// SPARQL query to select input data
    ///
    /// Executes the query against the RDF store and passes
    /// results as workflow input.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "sparql_query",
    ///   "query": "SELECT ?entity ?name WHERE { ?entity a gph:Customer }",
    ///   "graph": "http://graphica.io/latest",
    ///   "batch_size": 100
    /// }
    /// ```
    SparqlQuery {
        /// SPARQL SELECT query
        query: String,

        /// Optional named graph to query
        #[serde(skip_serializing_if = "Option::is_none")]
        graph: Option<String>,

        /// Batch size for processing results (default: 1000)
        #[serde(skip_serializing_if = "Option::is_none")]
        batch_size: Option<usize>,

        /// Optional result limit (default: unlimited)
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },

    /// Entity type filter with time range
    ///
    /// Automatically builds SPARQL query to select entities
    /// of a specific type with optional filters.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "entity_filter",
    ///   "entity_type": "gph:Customer",
    ///   "created_after": "2025-10-01T00:00:00Z",
    ///   "limit": 1000
    /// }
    /// ```
    EntityFilter {
        /// Entity type URI (e.g., "gph:Customer")
        entity_type: String,

        /// Optional named graph
        #[serde(skip_serializing_if = "Option::is_none")]
        graph: Option<String>,

        /// Filter entities created after this timestamp
        #[serde(skip_serializing_if = "Option::is_none")]
        created_after: Option<String>,

        /// Filter entities updated after this timestamp
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_after: Option<String>,

        /// Maximum number of entities to process
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,

        /// Batch size for processing (default: 1000)
        #[serde(skip_serializing_if = "Option::is_none")]
        batch_size: Option<usize>,
    },

    /// Direct JSON input (legacy/backward compatibility)
    ///
    /// Accepts arbitrary JSON data as workflow input.
    /// Use this for workflows that don't operate on graph data.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "json",
    ///   "data": {
    ///     "customer_ids": ["cust_123", "cust_456"],
    ///     "operation": "enrich"
    ///   }
    /// }
    /// ```
    Json {
        /// Arbitrary JSON data
        #[schema(value_type = Object)]
        data: JsonValue,
    },

    /// Materialized dataset input
    ///
    /// Loads rows from a dataset that has already been materialized and
    /// registered in the catalog (currently Parquet-backed imports).
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "dataset",
    ///   "dataset_id": "ds_datasource_abc123",
    ///   "batch_size": 1000,
    ///   "limit": 10000
    /// }
    /// ```
    Dataset {
        /// Materialized dataset ID from the catalog
        dataset_id: String,

        /// Batch size for processing rows (default: 1000)
        #[serde(skip_serializing_if = "Option::is_none")]
        batch_size: Option<usize>,

        /// Maximum number of rows to load (default: unlimited)
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },

    /// Data source query (external databases, files, etc.)
    ///
    /// Query external data sources registered in the catalog
    /// (PostgreSQL, Oracle, DB2, SAP HANA, Snowflake, Parquet, CSV).
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "data_source_query",
    ///   "source_id": "urn:graphica:datasource:postgres_prod",
    ///   "query": "SELECT * FROM customers WHERE created_at > '2025-10-01'",
    ///   "batch_size": 1000,
    ///   "limit": 10000
    /// }
    /// ```
    DataSourceQuery {
        /// Data source ID (URN from catalog)
        source_id: String,

        /// SQL query or filter expression (source-dependent)
        query: String,

        /// Optional query parameters (parameterized queries)
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schema(value_type = Option<HashMap<String, Object>>)]
        parameters: Option<std::collections::HashMap<String, serde_json::Value>>,

        /// Batch size for processing results (default: 1000)
        #[serde(skip_serializing_if = "Option::is_none")]
        batch_size: Option<usize>,

        /// Maximum number of rows to fetch (default: unlimited)
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,

        /// Query timeout in seconds (default: 30)
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
    },

    /// Graph change stream (future - for real-time workflows)
    ///
    /// Subscribe to changes in a named graph and process
    /// them as they occur.
    #[allow(dead_code)]
    GraphStream {
        /// Named graph URI to monitor
        graph_uri: String,

        /// Stream starting point (timestamp or "now")
        #[serde(skip_serializing_if = "Option::is_none")]
        start_from: Option<String>,

        /// Entity types to include (empty = all types)
        #[serde(default)]
        entity_types: Vec<String>,
    },
}

impl WorkflowInput {
    /// Get the execution mode implied by this input type
    pub fn execution_mode(&self) -> ExecutionMode {
        match self {
            WorkflowInput::SparqlQuery { batch_size, .. } => {
                if batch_size.is_some() {
                    ExecutionMode::Batch
                } else {
                    ExecutionMode::Single
                }
            }
            WorkflowInput::EntityFilter { batch_size, .. } => {
                if batch_size.is_some() {
                    ExecutionMode::Batch
                } else {
                    ExecutionMode::Single
                }
            }
            WorkflowInput::DataSourceQuery { batch_size, .. } => {
                if batch_size.is_some() {
                    ExecutionMode::Batch
                } else {
                    ExecutionMode::Single
                }
            }
            WorkflowInput::Dataset { batch_size, .. } => {
                if batch_size.is_some() {
                    ExecutionMode::Batch
                } else {
                    ExecutionMode::Single
                }
            }
            WorkflowInput::Json { .. } => ExecutionMode::Single,
            WorkflowInput::GraphStream { .. } => ExecutionMode::Streaming,
        }
    }

    /// Validate input configuration
    pub fn validate(&self) -> Result<()> {
        match self {
            WorkflowInput::SparqlQuery {
                query, batch_size, ..
            } => {
                if query.trim().is_empty() {
                    anyhow::bail!("SPARQL query cannot be empty");
                }
                if !query.trim().to_uppercase().starts_with("SELECT") {
                    anyhow::bail!("Only SELECT queries are supported for workflow input");
                }
                if let Some(size) = batch_size {
                    if *size == 0 || *size > 10_000 {
                        anyhow::bail!("Batch size must be between 1 and 10,000");
                    }
                }
                Ok(())
            }
            WorkflowInput::EntityFilter {
                entity_type,
                batch_size,
                ..
            } => {
                if entity_type.trim().is_empty() {
                    anyhow::bail!("Entity type cannot be empty");
                }
                if let Some(size) = batch_size {
                    if *size == 0 || *size > 10_000 {
                        anyhow::bail!("Batch size must be between 1 and 10,000");
                    }
                }
                Ok(())
            }
            WorkflowInput::DataSourceQuery {
                source_id,
                query,
                batch_size,
                timeout_secs,
                ..
            } => {
                if source_id.trim().is_empty() {
                    anyhow::bail!("Data source ID cannot be empty");
                }
                if query.trim().is_empty() {
                    anyhow::bail!("Query cannot be empty");
                }
                if let Some(size) = batch_size {
                    if *size == 0 || *size > 10_000 {
                        anyhow::bail!("Batch size must be between 1 and 10,000");
                    }
                }
                if let Some(timeout) = timeout_secs {
                    if *timeout == 0 || *timeout > 600 {
                        anyhow::bail!("Timeout must be between 1 and 600 seconds");
                    }
                }
                Ok(())
            }
            WorkflowInput::Dataset {
                dataset_id,
                batch_size,
                ..
            } => {
                if dataset_id.trim().is_empty() {
                    anyhow::bail!("Dataset ID cannot be empty");
                }
                if let Some(size) = batch_size {
                    if *size == 0 || *size > 10_000 {
                        anyhow::bail!("Batch size must be between 1 and 10,000");
                    }
                }
                Ok(())
            }
            WorkflowInput::Json { data } => {
                if data.is_null() {
                    anyhow::bail!("JSON input cannot be null");
                }
                Ok(())
            }
            WorkflowInput::GraphStream { graph_uri, .. } => {
                if graph_uri.trim().is_empty() {
                    anyhow::bail!("Graph URI cannot be empty");
                }
                Ok(())
            }
        }
    }
}

/// Workflow execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Process all input at once
    Single,
    /// Process input in batches
    Batch,
    /// Process input as a stream
    Streaming,
}

/// Input adapter trait - converts WorkflowInput to ExecutionContext
///
/// Adapters are responsible for:
/// 1. Fetching data from the source (SPARQL, filters, etc.)
/// 2. Converting to ExecutionContext format
/// 3. Handling batching and pagination
#[async_trait::async_trait]
pub trait InputAdapter: Send + Sync {
    /// Convert workflow input to execution context
    ///
    /// Returns a vector of execution contexts (one per batch).
    /// For single-shot execution, returns a single-element vec.
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>>;

    /// Get adapter name (for logging/debugging)
    fn name(&self) -> &str;
}

/// JSON input adapter (legacy/backward compatibility)
pub struct JsonInputAdapter;

#[async_trait::async_trait]
impl InputAdapter for JsonInputAdapter {
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>> {
        match input {
            WorkflowInput::Json { data } => {
                // Single execution context with JSON data
                Ok(vec![ExecutionContext::new(data.clone())])
            }
            _ => anyhow::bail!("JsonInputAdapter only handles Json input type"),
        }
    }

    fn name(&self) -> &str {
        "json"
    }
}

/// Dataset resolver trait
///
/// Implemented by the coordinator to resolve materialized datasets to rows.
#[async_trait::async_trait]
pub trait DatasetResolver: Send + Sync {
    /// Load rows from a materialized dataset.
    async fn load_rows(&self, dataset_id: &str, limit: Option<usize>) -> Result<Vec<JsonValue>>;
}

/// Materialized dataset input adapter.
pub struct DatasetInputAdapter {
    resolver: Arc<dyn DatasetResolver>,
}

impl DatasetInputAdapter {
    /// Create a new dataset input adapter.
    pub fn new(resolver: Arc<dyn DatasetResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait::async_trait]
impl InputAdapter for DatasetInputAdapter {
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>> {
        match input {
            WorkflowInput::Dataset {
                dataset_id,
                batch_size,
                limit,
            } => {
                let rows = self
                    .resolver
                    .load_rows(dataset_id, *limit)
                    .await
                    .with_context(|| format!("Failed to load dataset {}", dataset_id))?;

                let batch_sz = batch_size.unwrap_or(1000);
                let mut contexts = Vec::new();

                for chunk in rows.chunks(batch_sz) {
                    contexts.push(ExecutionContext::new(JsonValue::Array(chunk.to_vec())));
                }

                if contexts.is_empty() {
                    contexts.push(ExecutionContext::new(JsonValue::Array(vec![])));
                }

                Ok(contexts)
            }
            _ => anyhow::bail!("DatasetInputAdapter only handles Dataset input type"),
        }
    }

    fn name(&self) -> &str {
        "dataset"
    }
}

/// SPARQL query input adapter
///
/// Executes SPARQL queries and converts results to execution contexts.
/// Supports batching for large result sets.
pub struct SparqlInputAdapter {
    /// Query executor (injected from coordinator)
    query_executor: Arc<dyn QueryExecutor>,
}

impl SparqlInputAdapter {
    /// Create new SPARQL input adapter
    pub fn new(query_executor: Arc<dyn QueryExecutor>) -> Self {
        Self { query_executor }
    }
}

#[async_trait::async_trait]
impl InputAdapter for SparqlInputAdapter {
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>> {
        match input {
            WorkflowInput::SparqlQuery {
                query,
                graph,
                batch_size,
                limit,
            } => {
                // Execute SPARQL query
                let results = self
                    .query_executor
                    .execute_query(query, graph.as_deref())
                    .await
                    .context("Failed to execute SPARQL query")?;

                // Apply limit if specified
                let results = if let Some(lim) = limit {
                    results.into_iter().take(*lim).collect()
                } else {
                    results
                };

                // Convert results to execution contexts
                if let Some(batch_sz) = batch_size {
                    // Batched execution
                    let mut contexts = Vec::new();
                    for chunk in results.chunks(*batch_sz) {
                        let context = ExecutionContext::new(JsonValue::Array(chunk.to_vec()));
                        contexts.push(context);
                    }
                    Ok(contexts)
                } else {
                    // Single execution
                    Ok(vec![ExecutionContext::new(JsonValue::Array(results))])
                }
            }
            _ => anyhow::bail!("SparqlInputAdapter only handles SparqlQuery input type"),
        }
    }

    fn name(&self) -> &str {
        "sparql_query"
    }
}

/// Entity filter input adapter
///
/// Builds SPARQL queries from entity filters and executes them.
pub struct EntityFilterAdapter {
    /// Query executor
    query_executor: Arc<dyn QueryExecutor>,
}

impl EntityFilterAdapter {
    /// Create new entity filter adapter
    pub fn new(query_executor: Arc<dyn QueryExecutor>) -> Self {
        Self { query_executor }
    }

    /// Build SPARQL query from entity filter
    fn build_query(filter: &WorkflowInput) -> Result<String> {
        match filter {
            WorkflowInput::EntityFilter {
                entity_type,
                graph,
                created_after,
                updated_after,
                limit,
                ..
            } => {
                let mut query = String::from("SELECT ?entity WHERE {\n");

                // Add graph clause if specified
                if let Some(g) = graph {
                    query.push_str(&format!("  GRAPH <{}> {{\n", g));
                }

                // Entity type filter
                query.push_str(&format!("    ?entity a {} .\n", entity_type));

                // Created after filter
                if let Some(created) = created_after {
                    query.push_str(&format!(
                        "    ?entity gph:createdAt ?createdTime .\n    FILTER(?createdTime > \"{}\"^^xsd:dateTime)\n",
                        created
                    ));
                }

                // Updated after filter
                if let Some(updated) = updated_after {
                    query.push_str(&format!(
                        "    ?entity gph:updatedAt ?updatedTime .\n    FILTER(?updatedTime > \"{}\"^^xsd:dateTime)\n",
                        updated
                    ));
                }

                // Close graph clause
                if graph.is_some() {
                    query.push_str("  }\n");
                }

                query.push('}');

                // Add limit
                if let Some(lim) = limit {
                    query.push_str(&format!("\nLIMIT {}", lim));
                }

                Ok(query)
            }
            _ => anyhow::bail!("EntityFilterAdapter only handles EntityFilter input type"),
        }
    }
}

#[async_trait::async_trait]
impl InputAdapter for EntityFilterAdapter {
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>> {
        // Build SPARQL query from filter
        let query = Self::build_query(input)?;

        // Execute via SPARQL adapter
        let sparql_input = if let WorkflowInput::EntityFilter {
            graph, batch_size, ..
        } = input
        {
            WorkflowInput::SparqlQuery {
                query,
                graph: graph.clone(),
                batch_size: *batch_size,
                limit: None, // Limit already in query
            }
        } else {
            unreachable!()
        };

        // Delegate to SPARQL adapter
        let sparql_adapter = SparqlInputAdapter::new(self.query_executor.clone());
        sparql_adapter.prepare_context(&sparql_input).await
    }

    fn name(&self) -> &str {
        "entity_filter"
    }
}

/// Data source query input adapter
///
/// Executes queries against external data sources (PostgreSQL, Oracle, DB2, etc.)
/// via the data source catalog and connector system.
pub struct DataSourceInputAdapter {
    /// Data source catalog client
    catalog: Arc<dyn crate::catalog::client::DataSourceCatalog>,
}

impl DataSourceInputAdapter {
    /// Create new data source input adapter
    pub fn new(catalog: Arc<dyn crate::catalog::client::DataSourceCatalog>) -> Self {
        Self { catalog }
    }
}

#[async_trait::async_trait]
impl InputAdapter for DataSourceInputAdapter {
    async fn prepare_context(&self, input: &WorkflowInput) -> Result<Vec<ExecutionContext>> {
        match input {
            WorkflowInput::DataSourceQuery {
                source_id,
                query,
                parameters,
                batch_size,
                limit,
                timeout_secs,
            } => {
                // Route workflow queries through the catalog so workflows, preview queries,
                // discovery, and datasource APIs share one execution path.
                let query_limit = *limit;
                let params = parameters.clone().unwrap_or_default();

                let _query_timeout = timeout_secs.unwrap_or(30);
                let query_result = self
                    .catalog
                    .execute_query(source_id, query, params, query_limit)
                    .await
                    .context("Failed to execute data source query")?;

                // Convert query results to execution contexts
                let batch_sz = batch_size.unwrap_or(1000);

                let mut contexts = Vec::new();
                for chunk in query_result.rows.chunks(batch_sz) {
                    // Convert rows to JSON array
                    let context = ExecutionContext::new(JsonValue::Array(chunk.to_vec()));
                    contexts.push(context);
                }

                if contexts.is_empty() {
                    // Return single empty context if no results
                    contexts.push(ExecutionContext::new(JsonValue::Array(vec![])));
                }

                Ok(contexts)
            }
            _ => anyhow::bail!("DataSourceInputAdapter only handles DataSourceQuery input type"),
        }
    }

    fn name(&self) -> &str {
        "data_source_query"
    }
}

/// Query executor trait (abstraction over coordinator's query system)
///
/// This allows the workflow engine to be independent of the specific
/// query executor implementation.
#[async_trait::async_trait]
pub trait QueryExecutor: Send + Sync {
    /// Execute SPARQL SELECT query
    ///
    /// Returns array of result bindings as JSON objects.
    async fn execute_query(&self, query: &str, graph: Option<&str>) -> Result<Vec<JsonValue>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct MockDatasetResolver {
        rows: Vec<JsonValue>,
    }

    #[async_trait::async_trait]
    impl DatasetResolver for MockDatasetResolver {
        async fn load_rows(
            &self,
            _dataset_id: &str,
            limit: Option<usize>,
        ) -> Result<Vec<JsonValue>> {
            Ok(match limit {
                Some(limit) => self.rows.iter().take(limit).cloned().collect(),
                None => self.rows.clone(),
            })
        }
    }

    #[test]
    fn test_workflow_input_validation() {
        // Valid SPARQL query
        let input = WorkflowInput::SparqlQuery {
            query: "SELECT ?s WHERE { ?s a gph:Entity }".to_string(),
            graph: None,
            batch_size: Some(100),
            limit: None,
        };
        assert!(input.validate().is_ok());

        // Empty query
        let input = WorkflowInput::SparqlQuery {
            query: "".to_string(),
            graph: None,
            batch_size: None,
            limit: None,
        };
        assert!(input.validate().is_err());

        // Non-SELECT query
        let input = WorkflowInput::SparqlQuery {
            query: "INSERT DATA { <s> <p> <o> }".to_string(),
            graph: None,
            batch_size: None,
            limit: None,
        };
        assert!(input.validate().is_err());

        // Invalid batch size
        let input = WorkflowInput::SparqlQuery {
            query: "SELECT ?s WHERE { ?s a gph:Entity }".to_string(),
            graph: None,
            batch_size: Some(0),
            limit: None,
        };
        assert!(input.validate().is_err());

        // Valid entity filter
        let input = WorkflowInput::EntityFilter {
            entity_type: "gph:Customer".to_string(),
            graph: None,
            created_after: None,
            updated_after: None,
            limit: Some(1000),
            batch_size: Some(100),
        };
        assert!(input.validate().is_ok());

        // Empty entity type
        let input = WorkflowInput::EntityFilter {
            entity_type: "".to_string(),
            graph: None,
            created_after: None,
            updated_after: None,
            limit: None,
            batch_size: None,
        };
        assert!(input.validate().is_err());

        // Valid JSON
        let input = WorkflowInput::Json {
            data: serde_json::json!({"key": "value"}),
        };
        assert!(input.validate().is_ok());

        // Null JSON
        let input = WorkflowInput::Json {
            data: JsonValue::Null,
        };
        assert!(input.validate().is_err());

        // Valid DataSourceQuery
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: Some(1000),
            limit: Some(10000),
            timeout_secs: Some(60),
        };
        assert!(input.validate().is_ok());

        // Empty source_id
        let input = WorkflowInput::DataSourceQuery {
            source_id: "".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: None,
            limit: None,
            timeout_secs: None,
        };
        assert!(input.validate().is_err());

        // Empty query
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "".to_string(),
            parameters: None,
            batch_size: None,
            limit: None,
            timeout_secs: None,
        };
        assert!(input.validate().is_err());

        // Invalid batch size
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: Some(0),
            limit: None,
            timeout_secs: None,
        };
        assert!(input.validate().is_err());

        // Invalid timeout
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: None,
            limit: None,
            timeout_secs: Some(0),
        };
        assert!(input.validate().is_err());

        let input = WorkflowInput::Dataset {
            dataset_id: "ds_datasource_123".to_string(),
            batch_size: Some(500),
            limit: Some(1000),
        };
        assert!(input.validate().is_ok());

        let input = WorkflowInput::Dataset {
            dataset_id: "".to_string(),
            batch_size: None,
            limit: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_execution_mode() {
        let input = WorkflowInput::SparqlQuery {
            query: "SELECT ?s WHERE { ?s a gph:Entity }".to_string(),
            graph: None,
            batch_size: Some(100),
            limit: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Batch);

        let input = WorkflowInput::SparqlQuery {
            query: "SELECT ?s WHERE { ?s a gph:Entity }".to_string(),
            graph: None,
            batch_size: None,
            limit: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Single);

        let input = WorkflowInput::Json {
            data: serde_json::json!({}),
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Single);

        // DataSourceQuery with batch_size -> Batch mode
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: Some(1000),
            limit: None,
            timeout_secs: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Batch);

        // DataSourceQuery without batch_size -> Single mode
        let input = WorkflowInput::DataSourceQuery {
            source_id: "urn:graphica:datasource:postgres_prod".to_string(),
            query: "SELECT * FROM customers".to_string(),
            parameters: None,
            batch_size: None,
            limit: None,
            timeout_secs: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Single);

        let input = WorkflowInput::Dataset {
            dataset_id: "ds_datasource_123".to_string(),
            batch_size: Some(1000),
            limit: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Batch);

        let input = WorkflowInput::Dataset {
            dataset_id: "ds_datasource_123".to_string(),
            batch_size: None,
            limit: None,
        };
        assert_eq!(input.execution_mode(), ExecutionMode::Single);
    }

    #[test]
    fn test_entity_filter_query_building() {
        // Basic filter
        let input = WorkflowInput::EntityFilter {
            entity_type: "gph:Customer".to_string(),
            graph: None,
            created_after: None,
            updated_after: None,
            limit: Some(100),
            batch_size: None,
        };
        let query = EntityFilterAdapter::build_query(&input).unwrap();
        assert!(query.contains("?entity a gph:Customer"));
        assert!(query.contains("LIMIT 100"));

        // With graph
        let input = WorkflowInput::EntityFilter {
            entity_type: "gph:Product".to_string(),
            graph: Some("http://graphica.io/latest".to_string()),
            created_after: None,
            updated_after: None,
            limit: None,
            batch_size: None,
        };
        let query = EntityFilterAdapter::build_query(&input).unwrap();
        assert!(query.contains("GRAPH <http://graphica.io/latest>"));
        assert!(query.contains("?entity a gph:Product"));

        // With time filters
        let input = WorkflowInput::EntityFilter {
            entity_type: "gph:Order".to_string(),
            graph: None,
            created_after: Some("2025-10-01T00:00:00Z".to_string()),
            updated_after: Some("2025-10-05T00:00:00Z".to_string()),
            limit: None,
            batch_size: None,
        };
        let query = EntityFilterAdapter::build_query(&input).unwrap();
        assert!(query.contains("?createdTime > \"2025-10-01T00:00:00Z\""));
        assert!(query.contains("?updatedTime > \"2025-10-05T00:00:00Z\""));
    }

    #[tokio::test]
    async fn test_json_adapter() {
        let adapter = JsonInputAdapter;
        let input = WorkflowInput::Json {
            data: serde_json::json!({"test": "data"}),
        };

        let contexts = adapter.prepare_context(&input).await.unwrap();
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].input_data, serde_json::json!({"test": "data"}));
    }

    #[tokio::test]
    async fn test_dataset_adapter_batches_rows() {
        let adapter = DatasetInputAdapter::new(Arc::new(MockDatasetResolver {
            rows: vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
                serde_json::json!({"id": 3}),
            ],
        }));
        let input = WorkflowInput::Dataset {
            dataset_id: "ds_datasource_123".to_string(),
            batch_size: Some(2),
            limit: None,
        };

        let contexts = adapter.prepare_context(&input).await.unwrap();
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].input_data.as_array().unwrap().len(), 2);
        assert_eq!(contexts[1].input_data.as_array().unwrap().len(), 1);
    }
}
