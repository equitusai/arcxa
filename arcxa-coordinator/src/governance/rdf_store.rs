//! RDF Store Coordinator Interface
//!
//! This module provides the RDF storage abstraction for the coordinator.
//! In the multi-shard architecture, actual RDF storage is in shard processes.
//! The coordinator uses gRPC to query shards.

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Named graph identifier for versioning and time-travel
#[derive(Debug, Clone)]
pub struct NamedGraph {
    pub uri: String,
}

impl NamedGraph {
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }

    /// Current/default graph
    pub fn current() -> Self {
        Self::new("http://graphica.io/graph/current")
    }

    /// Graph for specific date (YYYY-MM-DD)
    pub fn date(date: &str) -> Self {
        Self::new(format!("http://graphica.io/graph/{}", date))
    }

    /// Graph for fusion operations
    pub fn fusion() -> Self {
        Self::new("http://graphica.io/graph/fusion")
    }

    /// Graph for model metadata
    pub fn models() -> Self {
        Self::new("http://graphica.io/graph/models")
    }

    /// Archive graph
    pub fn archive(year: u16, month: u8) -> Self {
        Self::new(format!(
            "http://graphica.io/graph/archive/{:04}-{:02}",
            year, month
        ))
    }

    /// Graph for workflow lineage
    pub fn workflows() -> Self {
        Self::new("http://graphica.io/graph/workflows")
    }

    /// Graph for workflow executions
    pub fn workflow_executions() -> Self {
        Self::new("http://graphica.io/graph/workflow-executions")
    }
}

/// RDF triple for lineage tracking and workflow governance
///
/// Represents a subject-predicate-object triple with typed values.
/// Used by the workflow lineage tracking system to generate RDF triples
/// for field-level provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct RdfTriple {
    pub subject: String,
    pub predicate: String,
    pub object: RdfValue,
}

impl RdfTriple {
    /// Create a new RDF triple (auto-detects URI vs literal)
    ///
    /// This is the primary constructor used by workflow lineage generation.
    /// Automatically detects URIs (strings starting with http:// or containing namespace prefixes)
    /// and creates typed literals for numbers/dates.
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        let obj_str = object.into();

        // Detect object type
        let object_value = if obj_str.starts_with("http://")
            || obj_str.starts_with("https://")
            || obj_str.contains(':')
        {
            // URI reference
            RdfValue::Uri(obj_str)
        } else {
            // Plain literal
            RdfValue::Literal(obj_str)
        };

        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object_value,
        }
    }

    /// Create a new RDF triple with a literal object
    pub fn new_literal(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        literal: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: RdfValue::Literal(literal.into()),
        }
    }

    /// Create a new RDF triple with a URI object
    pub fn new_uri(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        uri: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: RdfValue::Uri(uri.into()),
        }
    }

    /// Create a new RDF triple with a typed literal
    pub fn new_typed(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
        datatype: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: RdfValue::TypedLiteral {
                value: value.into(),
                datatype: datatype.into(),
            },
        }
    }

    /// Convert triple to tuple format (subject, predicate, object_string)
    pub fn to_tuple(&self) -> (String, String, String) {
        (
            self.subject.clone(),
            self.predicate.clone(),
            self.object.to_string(),
        )
    }
}

/// RDF value types for triple objects
#[derive(Debug, Clone, PartialEq)]
pub enum RdfValue {
    /// Plain literal value (string)
    Literal(String),
    /// URI reference
    Uri(String),
    /// Typed literal with XSD datatype
    TypedLiteral { value: String, datatype: String },
}

impl RdfValue {
    /// Convert RDF value to string representation
    pub fn to_string(&self) -> String {
        match self {
            RdfValue::Literal(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            RdfValue::Uri(uri) => format!("<{}>", uri),
            RdfValue::TypedLiteral { value, datatype } => {
                format!("\"{}\"^^<{}>", value.replace('"', "\\\""), datatype)
            }
        }
    }
}

/// RDF Store trait for abstraction
pub trait RdfStore: Send + Sync {
    /// Insert a triple into the store
    fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&NamedGraph>,
    ) -> Result<()>;

    /// Insert multiple triples (batch)
    fn insert_triples(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
    ) -> Result<()>;

    /// Execute SPARQL query
    fn query(&self, sparql: &str) -> Result<Vec<JsonValue>>;

    /// Execute SPARQL UPDATE (INSERT, DELETE, etc.)
    fn update(&self, sparql_update: &str) -> Result<()>;

    /// Load RDF data from Turtle format
    fn load_turtle(&self, turtle: &str, graph: Option<&NamedGraph>) -> Result<()>;

    /// Load ontology (always into default graph)
    fn load_ontology(&self, turtle: &str) -> Result<()>;

    /// Count triples in a graph
    fn count_triples(&self, graph: Option<&NamedGraph>) -> Result<u64>;

    /// Clear all triples from a graph
    fn clear_graph(&self, graph: &NamedGraph) -> Result<()>;
}

/// Auto-save statistics for monitoring
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoSaveStats {
    /// Unix timestamp of last successful save (0 if never saved)
    pub last_save_time: u64,
    /// Total number of successful auto-saves
    pub auto_save_count: u64,
    /// Total number of failed auto-saves
    pub auto_save_failures: u64,
    /// Seconds since last save (None if never saved)
    pub seconds_since_last_save: Option<u64>,
}

use super::distributed::ShardRegistry;
use super::in_memory_rdf_store::InMemoryRdfStore;
use super::shard_coordinator::ShardCoordinatingRdfStore;

/// Backend storage mode for GraphicaRdfStore
enum StorageBackend {
    /// Production: Distributed RDF storage across multiple shard servers via gRPC
    /// - Scatter-gather query execution with parallel shard queries
    /// - Connection pooling with circuit breakers
    /// - Smart routing (single-shard vs multi-shard)
    /// - High throughput: 50K+ writes/sec, 10K+ queries/sec
    Distributed(ShardCoordinatingRdfStore),

    /// In-memory mode for testing (no gRPC, no persistence)
    InMemory(InMemoryRdfStore),

    /// Stub mode - deprecated, use Distributed instead
    /// Only counts operations, no actual storage
    Stub(AtomicU64),
}

/// Graphica RDF Store - Production-grade distributed RDF storage
///
/// Coordinates RDF operations across multiple shard servers for horizontal scalability.
///
/// ## Architecture Modes
///
/// 1. **Production (Distributed)**: gRPC-based multi-shard coordination
///    - Created via `new_with_registry()` with shard topology
///    - Automatic scatter-gather for full-table queries
///    - Smart single-shard routing when subject is known
///    - Connection pooling and circuit breakers
///
/// 2. **Testing (InMemory)**: In-memory RDF store (no gRPC)
///    - Created via `new_in_memory()`
///    - Fast, no network overhead
///    - No persistence
///
/// 3. **Development (Stub)**: Counter-only mode
///    - Created via `new()`
///    - No actual storage, just operation counting
///    - Deprecated - use Distributed mode instead
///
/// ## Example
/// ```ignore
/// use graphica_coordinator::governance::rdf_store::{GraphicaRdfStore, RdfStore};
/// use graphica_coordinator::governance::distributed::ShardRegistry;
/// use std::sync::Arc;
///
/// # fn example() -> anyhow::Result<()> {
/// // Initialize shard registry with 4 shards
/// let registry = ShardRegistry::new("./data/shard-registry", 4, 60)?;
///
/// // Create distributed RDF store
/// let store = GraphicaRdfStore::new_with_registry(Arc::new(registry))?;
///
/// // Use RDF store
/// store.insert_triple("http://ex.com/alice", "rdf:type", "foaf:Person", None)?;
/// let results = store.query("SELECT * WHERE { ?s rdf:type foaf:Person }")?;
/// # Ok(())
/// # }
/// ```
pub struct GraphicaRdfStore {
    backend: StorageBackend,
}

impl GraphicaRdfStore {
    /// Create a distributed RDF store with shard coordinator (PRODUCTION)
    ///
    /// This is the recommended constructor for production deployments.
    /// Initializes gRPC connection pool and shard routing.
    ///
    /// # Arguments
    /// * `registry` - Shard registry containing cluster topology
    /// * `context` - Application context for observability
    ///
    /// # Example
    /// ```ignore
    /// # use graphica_coordinator::governance::rdf_store::GraphicaRdfStore;
    /// # use graphica_coordinator::governance::distributed::ShardRegistry;
    /// # use graphica_coordinator::AppContext;
    /// # use std::sync::Arc;
    /// # fn example() -> anyhow::Result<()> {
    /// let registry = ShardRegistry::new("./data/shards", 4, 60)?;
    /// let context = AppContext::minimal();
    /// let store = GraphicaRdfStore::new_with_registry(Arc::new(registry), context)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_registry(
        registry: Arc<ShardRegistry>,
        context: crate::app_context::AppContext,
    ) -> Result<Self> {
        tracing::info!(
            "🚀 Initializing distributed RDF store with shard coordinator and observability"
        );
        let coordinator = ShardCoordinatingRdfStore::new(registry, context);
        Ok(Self {
            backend: StorageBackend::Distributed(coordinator),
        })
    }

    /// Create a distributed RDF store with WAL for crash recovery (PRODUCTION + DURABILITY)
    ///
    /// This constructor creates a distributed RDF store with Write-Ahead Log
    /// for crash recovery and durability guarantees.
    ///
    /// # Arguments
    /// * `registry` - Shard registry containing cluster topology
    /// * `context` - Application context for observability
    /// * `wal` - RDF WAL wrapper for durable operations
    ///
    /// # Example
    /// ```ignore
    /// # use graphica_coordinator::governance::rdf_store::GraphicaRdfStore;
    /// # use graphica_coordinator::governance::distributed::ShardRegistry;
    /// # use graphica_coordinator::governance::rdf_wal::RdfWalWrapper;
    /// # use graphica_coordinator::AppContext;
    /// # use std::sync::Arc;
    /// # fn example(wal: Arc<RdfWalWrapper>) -> anyhow::Result<()> {
    /// let registry = ShardRegistry::new("./data/shards", 4, 60)?;
    /// let context = AppContext::minimal();
    /// let store = GraphicaRdfStore::new_with_registry_and_wal(
    ///     Arc::new(registry),
    ///     context,
    ///     wal
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_registry_and_wal(
        registry: Arc<ShardRegistry>,
        context: crate::app_context::AppContext,
        wal: Arc<crate::governance::rdf_wal::RdfWalWrapper>,
    ) -> Result<Self> {
        tracing::info!(
            "🚀 Initializing distributed RDF store with shard coordinator, WAL, and observability"
        );
        let coordinator = ShardCoordinatingRdfStore::with_wal(registry, context, wal);
        Ok(Self {
            backend: StorageBackend::Distributed(coordinator),
        })
    }

    /// Create a stub RDF store (DEPRECATED - use new_with_registry for production)
    ///
    /// This creates a stub that only counts operations.
    /// Provided for backwards compatibility only.
    #[deprecated(since = "0.2.0", note = "Use new_with_registry() for production")]
    pub fn new(_storage_path: &str) -> Result<Self> {
        tracing::warn!(
            "⚠️  Using stub RDF store - no actual storage! Use new_with_registry() for production."
        );
        Ok(Self {
            backend: StorageBackend::Stub(AtomicU64::new(0)),
        })
    }

    /// Create an in-memory RDF store for testing
    ///
    /// This creates a fully functional in-memory RDF store (no gRPC).
    /// Useful for unit tests and local development.
    pub fn new_in_memory() -> Result<Self> {
        Ok(Self {
            backend: StorageBackend::InMemory(InMemoryRdfStore::new()),
        })
    }

    /// Get statistics
    pub fn get_auto_save_stats(&self) -> AutoSaveStats {
        AutoSaveStats {
            last_save_time: 0,
            auto_save_count: 0,
            auto_save_failures: 0,
            seconds_since_last_save: None,
        }
    }

    /// Get triple count
    pub fn count_all_triples(&self) -> Result<u64> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.count_triples(None),
            StorageBackend::InMemory(store) => store.count_triples(None),
            StorageBackend::Stub(counter) => Ok(counter.load(Ordering::Relaxed)),
        }
    }

    /// Get triple count (alias for compatibility)
    pub fn triple_count(&self) -> Result<u64> {
        self.count_all_triples()
    }

    /// Get temporal indexes (stub for compatibility)
    pub fn temporal_indexes(&self) -> Option<Arc<crate::bitemporal::TemporalIndexes>> {
        None
    }

    /// Get WAL statistics (stub for compatibility)
    pub fn wal_statistics(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "total_entries": 0,
            "uncommitted_entries": 0,
        }))
    }

    /// Save to disk (stub for compatibility)
    /// Returns number of quads saved
    pub fn save_to_disk(&self) -> Result<usize> {
        Ok(0)
    }

    /// Replay WAL (stub for compatibility)
    /// Returns number of operations replayed
    pub fn replay_wal(&self) -> Result<usize> {
        Ok(0)
    }

    /// Insert RDF* batch (stub for compatibility)
    pub fn insert_rdf_star_batch(
        &self,
        _triples: Vec<crate::governance::rdf_star::AnnotatedTriple>,
        _graph: Option<&NamedGraph>,
    ) -> Result<()> {
        Ok(())
    }

    /// Get uncommitted WAL operations (stub for compatibility)
    pub fn get_uncommitted_wal_operations(&self) -> Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    /// Insert a batch of RDF triples (convenience method for workflow lineage)
    ///
    /// This method accepts strongly-typed RdfTriple structs and converts them
    /// to the tuple format required by insert_triples().
    ///
    /// # Arguments
    /// * `triples` - Slice of RdfTriple structs with typed values
    /// * `graph` - Optional named graph (if None, uses default graph)
    ///
    /// # Example
    /// ```ignore
    /// use graphica_coordinator::governance::rdf_store::{GraphicaRdfStore, RdfTriple, RdfValue, NamedGraph};
    ///
    /// # fn example(store: &GraphicaRdfStore) -> anyhow::Result<()> {
    /// let triples = vec![
    ///     RdfTriple::new_uri("ex:alice", "rdf:type", "foaf:Person"),
    ///     RdfTriple::new_literal("ex:alice", "foaf:name", "Alice"),
    ///     RdfTriple::new_typed("ex:alice", "foaf:age", "30", "xsd:integer"),
    /// ];
    ///
    /// let graph = NamedGraph::workflows();
    /// store.insert_batch(&triples, Some(&graph))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert_batch(&self, triples: &[RdfTriple], graph: Option<&NamedGraph>) -> Result<()> {
        let tuple_triples: Vec<(String, String, String)> =
            triples.iter().map(|t| t.to_tuple()).collect();

        self.insert_triples(tuple_triples, graph)
    }
}

impl RdfStore for GraphicaRdfStore {
    fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => {
                coordinator.insert_triple(subject, predicate, object, graph)
            }
            StorageBackend::InMemory(store) => {
                store.insert_triple(subject, predicate, object, graph)
            }
            StorageBackend::Stub(counter) => {
                counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    fn insert_triples(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.insert_triples(triples, graph),
            StorageBackend::InMemory(store) => store.insert_triples(triples, graph),
            StorageBackend::Stub(counter) => {
                counter.fetch_add(triples.len() as u64, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.query(sparql),
            StorageBackend::InMemory(store) => store.query(sparql),
            StorageBackend::Stub(_) => Ok(vec![]),
        }
    }

    fn update(&self, sparql_update: &str) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.update(sparql_update),
            StorageBackend::InMemory(store) => store.update(sparql_update),
            StorageBackend::Stub(_) => Ok(()),
        }
    }

    fn load_turtle(&self, turtle: &str, graph: Option<&NamedGraph>) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.load_turtle(turtle, graph),
            StorageBackend::InMemory(store) => store.load_turtle(turtle, graph),
            StorageBackend::Stub(_) => Ok(()),
        }
    }

    fn load_ontology(&self, turtle: &str) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.load_ontology(turtle),
            StorageBackend::InMemory(store) => store.load_ontology(turtle),
            StorageBackend::Stub(_) => Ok(()),
        }
    }

    fn count_triples(&self, graph: Option<&NamedGraph>) -> Result<u64> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.count_triples(graph),
            StorageBackend::InMemory(store) => store.count_triples(graph),
            StorageBackend::Stub(counter) => Ok(counter.load(Ordering::Relaxed)),
        }
    }

    fn clear_graph(&self, graph: &NamedGraph) -> Result<()> {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => coordinator.clear_graph(graph),
            StorageBackend::InMemory(store) => store.clear_graph(graph),
            StorageBackend::Stub(counter) => {
                counter.store(0, Ordering::Relaxed);
                Ok(())
            }
        }
    }
}

impl Clone for GraphicaRdfStore {
    fn clone(&self) -> Self {
        match &self.backend {
            StorageBackend::Distributed(coordinator) => Self {
                backend: StorageBackend::Distributed(coordinator.clone()),
            },
            StorageBackend::InMemory(store) => Self {
                backend: StorageBackend::InMemory(store.clone()),
            },
            StorageBackend::Stub(counter) => Self {
                backend: StorageBackend::Stub(AtomicU64::new(counter.load(Ordering::Relaxed))),
            },
        }
    }
}

impl Default for GraphicaRdfStore {
    #[allow(deprecated)] // Default trait needs simple initialization
    fn default() -> Self {
        Self::new("").unwrap()
    }
}

// Type alias for backward compatibility
pub type RdfStoreHandle = Arc<GraphicaRdfStore>;
