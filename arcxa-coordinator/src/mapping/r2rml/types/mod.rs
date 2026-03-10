//! R2RML Type Definitions (Backward Compatibility)
//!
//! **DEPRECATED**: These types have been migrated to `mapping::semantic_mapping::rdf::r2rml_types`.
//! This module re-exports them for backward compatibility during Phase 2 migration.
//!
//! New code should import from `crate::mapping::semantic_mapping::rdf::r2rml_types` instead.

// Re-export all types from new location for backward compatibility
pub use crate::mapping::semantic_mapping::rdf::r2rml_types::*;

#[deprecated(
    since = "0.2.0",
    note = "Use crate::mapping::semantic_mapping::rdf::r2rml_types instead"
)]
pub use crate::mapping::semantic_mapping::rdf::r2rml_types as semantic_r2rml;
