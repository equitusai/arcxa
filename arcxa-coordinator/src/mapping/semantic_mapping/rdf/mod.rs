//! # RDF Output Adapter
//!
//! Generates RDF triples from semantic mappings using W3C R2RML standard.
//!
//! Phase 2 implementation - R2RML types and serialization migrated from `mapping::r2rml`

pub mod r2rml_types;
pub mod serialization;

// Re-export for convenience
pub use r2rml_types::*;
pub use serialization::R2rmlSerializer;
