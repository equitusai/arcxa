//! Async RDF Store Adapter
//!
//! Provides an async interface to the existing RdfStore trait.
//! This is a thin adapter that wraps sync RdfStore operations in tokio::spawn_blocking
//! to make them async-friendly for use in transformers and async contexts.
//!
//! ## Design Rationale
//!
//! - **Single Source of Truth**: Uses the existing RdfStore (no duplication)
//! - **Minimal Overhead**: Simple spawn_blocking wrapper
//! - **Consistent Data**: Same store used for sync and async access
//! - **Clean Architecture**: Adapter pattern for interface conversion
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::{GraphicaRdfStore, AsyncRdfStoreAdapter};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create the ONE RDF store
//! let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
//!
//! // Wrap it for async access
//! let async_adapter = AsyncRdfStoreAdapter::new(rdf_store);
//!
//! // Use async methods
//! let results = async_adapter.query("SELECT * WHERE { ?s ?p ?o }").await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tracing::debug;

use super::rdf_store::{NamedGraph, RdfStore};

/// Async adapter for RdfStore
///
/// Wraps a sync RdfStore implementation and provides async methods.
/// Uses tokio::spawn_blocking to avoid blocking the async runtime.
pub struct AsyncRdfStoreAdapter {
    /// The underlying RDF store (shared, thread-safe)
    store: Arc<dyn RdfStore>,
}

impl AsyncRdfStoreAdapter {
    /// Create a new async adapter wrapping an existing RDF store
    ///
    /// # Arguments
    ///
    /// * `store` - The RDF store to wrap
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
    /// let adapter = AsyncRdfStoreAdapter::new(rdf_store);
    /// ```
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        debug!("Created AsyncRdfStoreAdapter wrapping existing RDF store");
        Self { store }
    }

    /// Execute SPARQL query (async)
    ///
    /// # Arguments
    ///
    /// * `sparql` - SPARQL query string
    ///
    /// # Returns
    ///
    /// Vector of JSON objects representing query results
    pub async fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        let sparql = sparql.to_string();
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.query(&sparql))
            .await
            .context("Task join error")?
    }

    /// Load RDF data from Turtle format (async)
    ///
    /// # Arguments
    ///
    /// * `turtle` - Turtle-formatted RDF data
    /// * `graph` - Optional named graph
    pub async fn load_turtle(&self, turtle: &str, graph: Option<&str>) -> Result<()> {
        let turtle = turtle.to_string();
        let graph_opt = graph.map(|g| NamedGraph::new(g));
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.load_turtle(&turtle, graph_opt.as_ref()))
            .await
            .context("Task join error")?
    }

    /// Execute SPARQL UPDATE (async)
    ///
    /// # Arguments
    ///
    /// * `sparql_update` - SPARQL UPDATE query
    pub async fn update(&self, sparql_update: &str) -> Result<()> {
        let sparql_update = sparql_update.to_string();
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.update(&sparql_update))
            .await
            .context("Task join error")?
    }

    /// Count triples in a graph (async)
    ///
    /// # Arguments
    ///
    /// * `graph` - Optional named graph (None for default graph)
    pub async fn count(&self, graph: Option<&str>) -> Result<u64> {
        let graph_opt = graph.map(|g| NamedGraph::new(g));
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.count_triples(graph_opt.as_ref()))
            .await
            .context("Task join error")?
    }

    /// Insert a triple (async)
    ///
    /// # Arguments
    ///
    /// * `subject` - Subject URI
    /// * `predicate` - Predicate URI
    /// * `object` - Object (URI or literal)
    /// * `graph` - Optional named graph
    pub async fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&str>,
    ) -> Result<()> {
        let subject = subject.to_string();
        let predicate = predicate.to_string();
        let object = object.to_string();
        let graph_opt = graph.map(|g| NamedGraph::new(g));
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || {
            store.insert_triple(&subject, &predicate, &object, graph_opt.as_ref())
        })
        .await
        .context("Task join error")?
    }

    /// Insert multiple triples (async batch)
    ///
    /// # Arguments
    ///
    /// * `triples` - Vector of (subject, predicate, object) tuples
    /// * `graph` - Optional named graph
    pub async fn insert_triples(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&str>,
    ) -> Result<()> {
        let graph_opt = graph.map(|g| NamedGraph::new(g));
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.insert_triples(triples, graph_opt.as_ref()))
            .await
            .context("Task join error")?
    }

    /// Health check (async)
    ///
    /// Verifies that the underlying store is operational.
    pub async fn health_check(&self) -> Result<bool> {
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || {
            // Try a simple count query
            match store.count_triples(None) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        })
        .await
        .context("Task join error")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::in_memory_rdf_store::InMemoryRdfStore;

    #[tokio::test]
    async fn test_async_adapter_query() -> Result<()> {
        // Create in-memory store
        let store = Arc::new(InMemoryRdfStore::new());

        // Wrap in async adapter
        let adapter = AsyncRdfStoreAdapter::new(store.clone());

        // Insert some test data via sync interface
        store.insert_triple("http://example.com/alice", "rdf:type", "foaf:Person", None)?;

        // Query via async interface
        let results = adapter
            .query("SELECT * WHERE { ?s rdf:type foaf:Person }")
            .await?;

        assert!(!results.is_empty(), "Expected query results");

        Ok(())
    }

    #[tokio::test]
    async fn test_async_adapter_load_turtle() -> Result<()> {
        let store = Arc::new(InMemoryRdfStore::new());
        let adapter = AsyncRdfStoreAdapter::new(store.clone());

        let turtle = r#"
            @prefix foaf: <http://xmlns.com/foaf/0.1/> .
            <http://example.com/bob> a foaf:Person ;
                foaf:name "Bob" .
        "#;

        // Load via async interface
        adapter.load_turtle(turtle, None).await?;

        // Verify via sync interface
        let count = store.count_triples(None)?;
        assert!(count > 0, "Expected triples to be loaded");

        Ok(())
    }

    #[tokio::test]
    async fn test_async_adapter_count() -> Result<()> {
        let store = Arc::new(InMemoryRdfStore::new());
        let adapter = AsyncRdfStoreAdapter::new(store.clone());

        // Initially empty
        let count = adapter.count(None).await?;
        assert_eq!(count, 0);

        // Insert via sync interface
        store.insert_triple(
            "http://ex.com/s",
            "http://ex.com/p",
            "http://ex.com/o",
            None,
        )?;

        // Count via async interface
        let count = adapter.count(None).await?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_async_adapter_health_check() -> Result<()> {
        let store = Arc::new(InMemoryRdfStore::new());
        let adapter = AsyncRdfStoreAdapter::new(store);

        let healthy = adapter.health_check().await?;
        assert!(healthy, "Store should be healthy");

        Ok(())
    }
}
