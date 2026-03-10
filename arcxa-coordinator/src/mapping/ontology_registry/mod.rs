//! # Ontology Management for Field Mapping
//!
//! This module provides ontology term loading and management for the field mapping engine.
//!
//! ## Architecture
//!
//! The module is organized into three layers with clear separation of concerns:
//!
//! - **`parser`**: Low-level RDF/Turtle parsing to extract ontology terms
//! - **`registry_client`**: High-level registry integration and query orchestration
//! - **`defaults`**: Fallback ontology terms (schema.org) when no registry available
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::ontology_registry::RegistryClient;
//!
//! // Create a client with the ontology registry
//! let client = RegistryClient::new(ontology_registry.clone());
//!
//! // Get ontology terms (queries registry or falls back to defaults)
//! let terms = client.get_ontology_terms().await?;
//!
//! // Filter terms by namespace
//! let filtered = client.get_terms_by_namespaces(&["http://schema.org/"]).await?;
//! ```

pub mod defaults;
pub mod parser;
pub mod persisted_registry;
pub mod rdfxml_parser;
pub mod registry_client;

pub use defaults::DEFAULT_TERMS;
pub use parser::TurtleParser;
pub use persisted_registry::PersistedOntologyRegistry;
pub use rdfxml_parser::RdfXmlParser;
pub use registry_client::RegistryClient;

use crate::mapping::types::OntologyTerm;

/// Re-export for convenience
pub type OntologyTerms = Vec<OntologyTerm>;
