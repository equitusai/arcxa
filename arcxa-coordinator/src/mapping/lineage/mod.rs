//! Extended Lineage Tracking Module
//!
//! This module provides comprehensive lineage tracking through the entire
//! CSV-to-Database pipeline, including:
//!
//! - Forward lineage (CSV field → Ontology term → DB column)
//! - Reverse lineage (DB column → CSV sources)
//! - Fusion impact analysis (which target rows are affected by entity fusion)
//! - SPARQL query generation for lineage traversal
//!
//! ## Architecture
//!
//! The lineage tracking workflow:
//! 1. CSV field mappings create RDF triples: `<csv_field> gph:mapsTo <ontology_term>`
//! 2. Unified mappings create triples: `<ontology_term> gph:mapsTo <db_column>`
//! 3. Fusion operations create triples: `<rdf_entity> gph:wasFusedFrom <source_entity>`
//! 4. Load operations create triples: `<db_row> gph:derivedFrom <rdf_entity>`
//!
//! ## SPARQL Queries
//!
//! The service generates SPARQL queries to traverse these triple chains in both
//! forward and reverse directions.

pub mod extended;

// Re-export all types for easier access
pub use extended::{
    ExtendedLineageChain, ExtendedLineageService, FusionImpactResult, FusionInfo, OntologyTermInfo,
    ReverseLineageResult, SourceFieldContribution, SourceInfo, TargetColumnRef, TargetInfo,
    UnifiedMappingInfo,
};
