//! Field-Level Lineage Module
//!
//! Tracks provenance for individual fields in golden records.
//! Supports multiple voting strategies for conflict resolution.

pub mod ontology;
pub mod resolver;
pub mod storage;
pub mod types;
pub mod voting;

pub use resolver::FieldResolver;
pub use storage::FieldLineageStore;
pub use types::*;
pub use voting::VotingEngine;
