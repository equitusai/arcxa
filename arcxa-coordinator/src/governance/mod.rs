//! # Governance Module
//!
//! RDF-First semantic governance layer using Oxigraph as the foundation.
//!
//! This module implements the core RDF triple store that serves as the
//! foundational data model for Graphica. All governance data (lineage,
//! models, entities, fusion operations) are stored as RDF triples.
//!
//! Architecture:
//! - Oxigraph: RDF triple store with SPARQL 1.1 support
//! - Named Graphs: Versioning and time-travel via graph URIs
//! - SHACL: Constraint validation
//! - Operational Indexes: RocksDB column families for fast lookups

pub mod async_brain;
pub mod async_brain_v2;
pub mod async_config;
pub mod async_core;
pub mod async_rdf_adapter; // Async adapter for existing RdfStore (single source of truth)
pub mod audited_coordinator;
pub mod batch_processor;
pub mod bitemporal;
pub mod converters;
pub mod converters_star;
pub mod distributed;
pub mod embedded_shard; // DEPRECATED: To be removed (creates duplicate RDF store)
pub mod feature_flags;
pub mod gdpr_rdf;
pub mod in_memory_rdf_store;
pub mod lineage_converter;
pub mod message_router;
pub mod ontology;
pub mod processor_pool;
pub mod prometheus_metrics;
pub mod rdf_star;
pub mod rdf_store;
pub mod rdf_wal; // NEW: RDF Write-Ahead Log for durability
pub mod schema_versioning; // RDF-backed schema version persistence
pub mod shacl;
pub mod shard_coordinator;
pub mod shared;
pub mod shared_async;
pub mod sparql_templates;
pub mod workflow_ontology;
pub mod workflow_persistence;
pub mod workflow_query_adapter; // GDPR to RDF conversion for governance brain

// Re-export legacy modules for compatibility
pub mod rdf {
    //! Legacy RDF module - see rdf_store for new implementation
    pub use super::ontology::*;
}

pub mod sparql {
    //! Legacy SPARQL module - see sparql_templates for new implementation
    pub use super::sparql_templates::*;
}

pub use async_rdf_adapter::AsyncRdfStoreAdapter;
pub use in_memory_rdf_store::InMemoryRdfStore;
pub use rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore};

// DEPRECATED: Use AsyncRdfStoreAdapter instead (avoids duplicate RDF store)
pub use converters_star::{FusionOperation, ModelPrediction};
#[allow(deprecated)]
pub use embedded_shard::{DistributedShardClient, EmbeddedShard, RdfQueryClient, StorageMode};
pub use ontology::{GraphicaOntology, GRAPHICA_NS, ML_NS, PROV_NS};
pub use rdf_star::{
    AnnotatedTriple, AnnotatedTripleBuilder, Annotation, ToRdfStarTriples, TripleValue,
};
pub use sparql_templates::SparqlTemplates;
// Use async-compatible version of SharedGovernanceBrain
pub use async_config::AsyncGovernanceConfig;
pub use shared_async::{materialize_lineage_event, SharedGovernanceBrain};
// Re-export lineage_converter for external use
pub use async_brain_v2::AsyncGovernanceBrainV2;
pub use async_core::{AsyncBrainState, EventBatch, GovernanceMessage, ProcessorMetrics};
pub use audited_coordinator::AuditedShardCoordinator;
pub use batch_processor::BatchProcessor;
pub use bitemporal::{
    AuditEntry, BitemporalAnnotations, ExistingVersion, MVCCQueryExecutor, TemporalIndexes,
    TransactionId, TransactionManager, TripleMetadata, VersionManager, VersionRef, WalEntry,
    WalOperation, WalStatistics, WriteAheadLog,
};
pub use distributed::{
    ClusterTopology, HashRange, ReplicationConfig, ShardId, ShardMetadata, ShardRegistry,
    ShardStatus,
};
pub use lineage_converter::LineageConverter;
pub use processor_pool::{PoolError, PoolStats, ProcessorPool, ProcessorPoolConfig};
pub use schema_versioning::RdfSchemaVersionStore;
pub use shard_coordinator::ShardCoordinatingRdfStore;
pub use workflow_persistence::{ExecutionSummary, WorkflowResultPersistence};
// Note: SchemaVersion and SchemaVersionStore are re-exported from mapping::ddl::evolution::versioning

use anyhow::Result;

/// Governance brain managing RDF knowledge graph
///
/// This is the main entry point for the governance layer.
/// Wraps the RdfStore with convenience methods.
pub struct GovernanceBrain {
    store: GraphicaRdfStore,
}

impl GovernanceBrain {
    /// Create new governance brain with Oxigraph RDF store
    #[allow(deprecated)] // Simple constructor for backward compatibility
    pub fn new(storage_path: &str) -> Result<Self> {
        let store = GraphicaRdfStore::new(storage_path)?;
        Ok(Self { store })
    }

    /// Load Graphica ontology definitions
    pub fn load_ontology(&self, turtle: &str) -> Result<()> {
        self.store.load_ontology(turtle)?;
        tracing::info!("Loaded Graphica ontology ({} bytes)", turtle.len());
        Ok(())
    }

    /// Execute SPARQL query
    pub fn query(&self, sparql: &str) -> Result<Vec<serde_json::Value>> {
        self.store.query(sparql)
    }

    /// Validate data against SHACL shapes
    pub fn validate_shacl(&self, data_graph: &str, _shapes_graph: &str) -> Result<bool> {
        // SHACL validation to be implemented
        tracing::debug!("SHACL validation for {} triples", data_graph.len());
        Ok(true)
    }

    /// Insert lineage triple
    pub fn insert_lineage_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
    ) -> Result<()> {
        self.store.insert_triple(subject, predicate, object, None)?;
        tracing::debug!("Inserted triple: {} {} {}", subject, predicate, object);
        Ok(())
    }

    /// Get the underlying RDF store
    pub fn store(&self) -> &GraphicaRdfStore {
        &self.store
    }
}

impl Default for GovernanceBrain {
    fn default() -> Self {
        Self::new("./data/rdf").expect("Failed to initialize governance brain")
    }
}

/// Initialize the governance brain
pub fn initialize_governance(storage_path: &str) -> Result<GovernanceBrain> {
    let brain = GovernanceBrain::new(storage_path)?;

    // Load Graphica ontology
    let ontology = GraphicaOntology::default();
    brain.load_ontology(&ontology.to_turtle())?;

    tracing::info!("Governance brain initialized at {}", storage_path);

    Ok(brain)
}
