//! Storage layer for SoS validation
//!
//! This module provides RocksDB-based persistence for:
//! - System catalog (systems, interfaces, contracts)
//! - Validation reports and audit trails
//! - RDF representation for governance integration
//!
//! ## Design
//!
//! Uses RocksDB column families for data organization:
//! - `sos_systems`: System registrations
//! - `sos_interfaces`: Interface definitions
//! - `sos_contracts`: Data contracts
//! - `sos_validation_reports`: Validation results
//! - `sos_index_by_type`: Secondary index for system types
//! - `sos_index_by_vendor`: Secondary index for vendors
//!
//! ## RDF Integration
//!
//! All SoS entities are also stored as RDF triples in the governance store
//! to enable SPARQL-based policy validation.

pub mod manager;
pub mod rocks_store;

pub use manager::SosStorageManager;
pub use rocks_store::{Contract, Interface, SlaMetric, SosStore, System};
