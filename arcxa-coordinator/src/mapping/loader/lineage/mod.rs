//! Lineage Capture Module
//!
//! Captures W3C PROV-compliant lineage during ETL operations and stores
//! as RDF triples in the governance brain.

pub mod rdf_sink;

pub use rdf_sink::RdfLineageSink;
