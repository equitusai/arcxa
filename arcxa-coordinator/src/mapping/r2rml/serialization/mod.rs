//! R2RML Serialization (Backward Compatibility)
//!
//! **DEPRECATED**: Serialization has been migrated to `mapping::semantic_mapping::rdf::serialization`.
//! This module re-exports for backward compatibility during Phase 2 migration.
//!
//! New code should import from `crate::mapping::semantic_mapping::rdf::serialization` instead.

// Re-export from new location for backward compatibility
pub use crate::mapping::semantic_mapping::rdf::serialization::*;

#[deprecated(
    since = "0.2.0",
    note = "Use crate::mapping::semantic_mapping::rdf::serialization instead"
)]
pub use crate::mapping::semantic_mapping::rdf::serialization as semantic_serialization;
