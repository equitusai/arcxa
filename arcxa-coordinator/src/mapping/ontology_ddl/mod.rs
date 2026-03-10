//! Ontology-Driven DDL Generation
//!
//! This module provides semantic ontology mapping for DDL generation,
//! ensuring cross-source consistency and RDF-first architecture.
//!
//! # Architecture
//!
//! The ontology-DDL pipeline consists of 4 stages:
//!
//! 1. **Discovery**: Extract schema from sources (CSV, Parquet, databases)
//! 2. **Ontology Mapping**: Map fields to semantic ontology terms (e.g., schema.org)
//! 3. **SHACL Generation**: Create SHACL PropertyShapes with ontology-derived constraints
//! 4. **DDL Generation**: Convert SHACL shapes to SQL DDL via existing infrastructure
//!
//! # Example
//!
//! ```ignore
//! use graphica_coordinator::mapping::ontology_ddl::*;
//!
//! // Configure ontology-aware DDL generation
//! let config = OntologyDdlConfig {
//!     skip_ontology_mapping: false,
//!     min_mapping_confidence: 0.7,
//!     strict_constraints: true,
//!     record_lineage: true,
//!     max_candidates: 5,
//! };
//!
//! // Generate DDL with ontology mappings
//! let result = generate_ontology_aware_ddl(
//!     csv_path,
//!     "customers",
//!     "postgresql",
//!     &mapping_engine,
//!     Some(config),
//! ).await?;
//!
//! // Result includes:
//! // - DDL statements with consistent constraints
//! // - Ontology mappings (field → schema.org term)
//! // - SHACL shapes for validation
//! // - RDF lineage triples
//! ```

pub mod constraint_rules;
pub mod csv_integration;
pub mod mapping_resolver;
pub mod orchestrator;
pub mod rdf_lineage;
pub mod shacl_generator;
pub mod transformation_rules;
pub mod types;
pub mod unified_loader;

// Re-export commonly used types
pub use constraint_rules::OntologyConstraintRegistry;
pub use csv_integration::generate_ontology_ddl_from_csv;
pub use mapping_resolver::MappingResolver;
pub use orchestrator::{
    generate_ontology_ddl, generate_ontology_ddl_with_config, OntologyDdlOrchestrator,
};
pub use rdf_lineage::{add_lineage_to_result, LineageSummary, RdfLineageGenerator};
pub use shacl_generator::ShaclGenerator;
pub use transformation_rules::{FieldTransformation, OntologyTransformationRegistry};
pub use types::*;
pub use unified_loader::{
    load_csv_with_semantic_mapping, SemanticLoadConfig, SemanticLoadResult,
    SemanticLoaderJobManager,
};
