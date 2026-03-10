//! Semantic Mediation Engine
//!
//! Maps source → semantic ontology → target via universal business concepts.
//!
//! ## Architecture Decision: Why not direct vendor-to-vendor mappings?
//!
//! **Problem**: Direct Oracle→SAP mappings are:
//! - Not reusable (Oracle→Databricks requires new mapping)
//! - Not composable (Can't merge Oracle + Salesforce → SAP)
//! - Limited governance (No business context in lineage)
//! - O(N²) mapping explosion (5 vendors = 20 pairwise mappings)
//!
//! **Solution**: Semantic mediation layer:
//! ```
//! Oracle → accounting:AccountingDocument → SAP
//! Oracle → accounting:AccountingDocument → Databricks
//! Salesforce → accounting:AccountingDocument → SAP
//! ```
//! - O(2N) mappings (5 vendors = 10 mappings: 5 to semantic + 5 from semantic)
//! - Business lineage (track at concept level, not field level)
//! - Multi-source consolidation (all sources map to same semantic concept)
//!
//! ## Storage Strategy
//!
//! ### Decision Point #1: Where to store semantic mappings?
//!
//! **Options Considered**:
//! 1. RocksDB (like manual mappings) - Fast, local, no SPARQL queries
//! 2. RDF triple store - Semantic queries, graph traversal, but slower
//! 3. Hybrid: RocksDB index + RDF triples
//!
//! **Decision**: Hybrid approach
//! - **RocksDB**: Fast lookups by (vendor_id, table_name) → semantic concept
//! - **RDF triples**: Semantic lineage queries, impact analysis, governance
//!
//! **Rationale**:
//! - Composition algorithm needs fast "find semantic concept for table" queries (RocksDB)
//! - Governance needs "show all sources for concept" queries (RDF)
//! - Write-once-read-many pattern (pre-built mappings loaded at startup)
//!
//! ### Decision Point #2: Embedded vs external ontologies?
//!
//! **Options**:
//! 1. Embedded in binary (compile-time, faster startup, no file I/O)
//! 2. Filesystem (runtime loading, easier updates)
//! 3. Hybrid: Core ontologies embedded, vendor ontologies filesystem
//!
//! **Decision**: Hybrid
//! - Core semantic ontologies (accounting.ttl, supply_chain.ttl) embedded
//! - Vendor ontologies loaded from filesystem (allow user customization)
//!
//! ## Performance Considerations
//!
//! - Composition is O(S × T) where S = source field count, T = target field count
//! - For Oracle GL (3,247 fields) → SAP FI (7,000 fields) = 22M comparisons
//! - **Optimization**: RocksDB index pre-filters to semantic properties (reduces to ~100 comparisons)
//! - **Caching**: Composed mappings cached (invalidate on ontology update)
//!
//! ## Scalability Questions
//!
//! Q: What if customer has 100 custom fields in Oracle not in standard ontology?
//! A: Fallback to field mapping engine for unmapped fields (best effort)
//!
//! Q: What if two sources map to same semantic property with conflicting transformations?
//! A: Confidence-based resolution (highest confidence wins) + audit log
//!
//! Q: Can we parallelize composition for large schemas?
//! A: Yes, table-level parallelism via rayon (each table independent)

pub mod composition;
pub mod lineage;
pub mod storage;
pub mod types;

pub use composition::SemanticMediationEngine;
pub use storage::{SemanticMappingStore, VendorOntologyLibrary};
pub use types::*;

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};

use crate::mapping::ontology_registry::PersistedOntologyRegistry;
use crate::governance::rdf_store::RdfStore;

/// Core coordinator for semantic-mediated mappings
///
/// This is the main entry point for semantic mapping operations.
/// It coordinates between:
/// - Vendor ontology library (vendor schemas)
/// - Semantic mapping store (vendor↔semantic mappings)
/// - Composition engine (source→semantic→target)
/// - Lineage tracker (semantic provenance)
pub struct SemanticMappingCoordinator {
    /// Vendor ontology library (Oracle, SAP, etc.)
    vendor_library: Arc<VendorOntologyLibrary>,

    /// Semantic mapping store (vendor→semantic and semantic→vendor)
    mapping_store: Arc<SemanticMappingStore>,

    /// Composition engine (orchestrates source→semantic→target)
    composition_engine: Arc<SemanticMediationEngine>,

    /// Ontology registry (for core semantic ontologies)
    ontology_registry: Arc<PersistedOntologyRegistry>,

    /// RDF store (for semantic lineage and governance queries)
    rdf_store: Arc<dyn RdfStore>,
}

impl SemanticMappingCoordinator {
    /// Create semantic mapping coordinator
    ///
    /// # Arguments
    ///
    /// * `vendor_ontology_path` - Path to vendor ontologies directory
    /// * `ontology_registry` - Core ontology registry (accounting.ttl, etc.)
    /// * `rdf_store` - RDF triple store for lineage
    ///
    /// # Returns
    ///
    /// Coordinator with all ontologies and mappings loaded
    pub async fn new(
        vendor_ontology_path: impl AsRef<std::path::Path>,
        ontology_registry: Arc<PersistedOntologyRegistry>,
        rdf_store: Arc<dyn RdfStore>,
    ) -> Result<Self> {
        info!("Initializing semantic mapping coordinator");

        // Load vendor ontologies
        let vendor_library = Arc::new(
            VendorOntologyLibrary::open(vendor_ontology_path)
                .await?
        );

        info!("Loaded {} vendor ontologies", vendor_library.count_vendors()?);

        // Load semantic mappings
        let mapping_store = Arc::new(
            SemanticMappingStore::open(&vendor_library, ontology_registry.clone())
                .await?
        );

        info!("Loaded {} semantic mappings", mapping_store.count_mappings()?);

        // Create composition engine
        let composition_engine = Arc::new(
            SemanticMediationEngine::new(
                vendor_library.clone(),
                mapping_store.clone(),
                ontology_registry.clone(),
            )
        );

        Ok(Self {
            vendor_library,
            mapping_store,
            composition_engine,
            ontology_registry,
            rdf_store,
        })
    }

    /// Compose source→target mapping via semantic layer
    ///
    /// This is the main API for creating vendor-to-vendor mappings.
    ///
    /// # Arguments
    ///
    /// * `request` - Composition request (source vendor, target vendor, modules)
    ///
    /// # Returns
    ///
    /// Composed mapping with confidence scores and lineage
    ///
    /// # Example
    ///
    /// ```ignore
    /// let request = ComposeMappingRequest {
    ///     source_vendor: "oracle_ebs_r12.2".to_string(),
    ///     target_vendor: "sap_s4hana_2023".to_string(),
    ///     modules: vec!["GL".to_string()],
    /// };
    ///
    /// let composed = coordinator.compose_mapping(request).await?;
    /// println!("Mapped {} tables with {}% coverage",
    ///     composed.table_mappings.len(),
    ///     composed.coverage_percent
    /// );
    /// ```
    pub async fn compose_mapping(
        &self,
        request: ComposeMappingRequest,
    ) -> Result<ComposedSemanticMapping> {
        debug!("Composing mapping: {:?} → {:?}", request.source_vendor, request.target_vendor);

        // Delegate to composition engine
        let composed = self.composition_engine.compose(request).await?;

        // Store lineage in RDF store
        self.store_semantic_lineage(&composed).await?;

        info!(
            "Composed mapping: {} tables, {:.1}% coverage",
            composed.table_mappings.len(),
            composed.coverage_percent
        );

        Ok(composed)
    }

    /// Store semantic lineage as RDF triples for governance queries
    async fn store_semantic_lineage(&self, composed: &ComposedSemanticMapping) -> Result<()> {
        // This will be implemented in lineage.rs
        // Creates RDF triples like:
        //   :oracle_gl_je_headers :mapsToSemantic :AccountingDocument
        //   :AccountingDocument :mapsToTarget :sap_bkpf
        //   :oracle_status_field :semanticProperty :documentStatus
        Ok(())
    }

    /// Get vendor ontology by ID
    pub async fn get_vendor_ontology(&self, vendor_id: &str) -> Result<VendorOntology> {
        self.vendor_library.get_ontology(vendor_id).await
    }

    /// List available vendor ontologies
    pub fn list_vendors(&self) -> Result<Vec<VendorOntologyMetadata>> {
        self.vendor_library.list_vendors()
    }

    /// Get semantic concept coverage for vendor
    ///
    /// Shows which semantic concepts are covered by vendor ontology
    pub async fn get_semantic_coverage(
        &self,
        vendor_id: &str,
    ) -> Result<SemanticCoverageReport> {
        self.mapping_store.get_coverage(vendor_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::in_memory_rdf_store::InMemoryRdfStore;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_semantic_coordinator_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let vendor_path = temp_dir.path().join("vendors");
        std::fs::create_dir(&vendor_path).unwrap();

        let ontology_registry = Arc::new(
            PersistedOntologyRegistry::open(temp_dir.path().join("ontologies"))
                .await
                .unwrap()
        );

        let rdf_store: Arc<dyn RdfStore> = Arc::new(InMemoryRdfStore::new());

        let coordinator = SemanticMappingCoordinator::new(
            vendor_path,
            ontology_registry,
            rdf_store,
        )
        .await
        .unwrap();

        // Should initialize successfully even with empty vendors directory
        assert_eq!(coordinator.list_vendors().unwrap().len(), 0);
    }
}
