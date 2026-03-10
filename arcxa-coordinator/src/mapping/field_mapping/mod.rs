//! # Unified Ontology Mapping System
//!
//! A consolidated mapping engine that eliminates duplication across the codebase
//! while preserving all existing capabilities from MappingEngine, OntologyDdlOrchestrator,
//! and graphica-core's FieldMapper.
//!
//! ## Architecture
//!
//! The unified system uses a strategy pattern with pluggable matchers:
//!
//! - **Pattern Strategy**: Detects emails, phones, URLs, etc. (0.85-0.95 confidence)
//! - **Semantic Strategy**: Transformer embeddings via graphica-model (0.80-0.90 confidence)
//! - **Statistical Strategy**: TF-IDF + N-grams (0.70-0.85 confidence)
//! - **Lexical Strategy**: Edit distance, Jaro-Winkler (0.65-0.80 confidence)
//! - **Registry Strategy**: Custom ontology matching (0.75-0.90 confidence)
//! - **Heuristic Strategy**: Name/type-based rules (0.60-0.75 confidence)

pub mod adapters;
pub mod engine;
pub mod scoring;
pub mod shared;
pub mod strategies;
pub mod types;

#[cfg(test)]
mod tests;

pub use adapters::{create_ontology_ddl_adapter, OntologyDdlAdapter};
pub use engine::UnifiedOntologyMappingEngine;
pub use scoring::ConfidenceScorer;
pub use types::*;
