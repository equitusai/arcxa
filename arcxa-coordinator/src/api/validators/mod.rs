//! Validation Logic
//!
//! Business logic for validating API requests and data.

pub mod fusion;
pub mod sparql;

// Re-export validation functions
pub use fusion::{validate_entity_count, validate_match_rule};
pub use sparql::{is_query_too_complex, validate_sparql_query};
