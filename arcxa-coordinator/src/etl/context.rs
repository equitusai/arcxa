//! ETL Execution Context
//!
//! Provides shared infrastructure for ETL executors including connection pools,
//! datasource catalog access, and RDF store access.

use anyhow::Result;
use graphica_core::catalog::client::DataSourceCatalog;
use std::collections::HashMap;
use std::sync::Arc;

use crate::governance::rdf_store::GraphicaRdfStore;

/// ETL execution context with shared resources
///
/// This context is passed to all ETL executors to provide access to:
/// - Connection pools (via datasource catalog)
/// - RDF store
/// - Workflow metadata
#[derive(Clone)]
pub struct EtlContext {
    /// Workflow execution ID
    pub workflow_id: String,

    /// Unique execution ID for this run
    pub execution_id: String,

    /// Current step ID
    pub step_id: String,

    /// Custom metadata for this execution
    pub metadata: HashMap<String, String>,

    /// Data source catalog for database connections
    pub catalog: Option<Arc<dyn DataSourceCatalog + Send + Sync>>,

    /// RDF store for semantic data loading
    pub rdf_store: Option<Arc<GraphicaRdfStore>>,
}

impl EtlContext {
    /// Create a new ETL context
    pub fn new(workflow_id: String, execution_id: String, step_id: String) -> Self {
        Self {
            workflow_id,
            execution_id,
            step_id,
            metadata: HashMap::new(),
            catalog: None,
            rdf_store: None,
        }
    }

    /// Add metadata to context
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Add datasource catalog
    pub fn with_catalog(mut self, catalog: Arc<dyn DataSourceCatalog + Send + Sync>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Add RDF store
    pub fn with_rdf_store(mut self, rdf_store: Arc<GraphicaRdfStore>) -> Self {
        self.rdf_store = Some(rdf_store);
        self
    }

    /// Get datasource catalog
    pub fn catalog(&self) -> Result<&Arc<dyn DataSourceCatalog + Send + Sync>> {
        self.catalog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DataSourceCatalog not configured in ETL context"))
    }

    /// Get RDF store
    pub fn rdf_store(&self) -> Result<&Arc<GraphicaRdfStore>> {
        self.rdf_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("RDF store not configured in ETL context"))
    }
}

impl std::fmt::Debug for EtlContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EtlContext")
            .field("workflow_id", &self.workflow_id)
            .field("execution_id", &self.execution_id)
            .field("step_id", &self.step_id)
            .field("metadata", &self.metadata)
            .field("has_catalog", &self.catalog.is_some())
            .field("has_rdf_store", &self.rdf_store.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etl_context_creation() {
        let context = EtlContext::new(
            "wf_001".to_string(),
            "exec_001".to_string(),
            "step_001".to_string(),
        )
        .with_metadata("key1".to_string(), "value1".to_string());

        assert_eq!(context.workflow_id, "wf_001");
        assert_eq!(context.execution_id, "exec_001");
        assert_eq!(context.step_id, "step_001");
        assert_eq!(context.metadata.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_context_without_catalog() {
        let context = EtlContext::new(
            "wf_001".to_string(),
            "exec_001".to_string(),
            "step_001".to_string(),
        );

        assert!(context.catalog().is_err());
    }
}
