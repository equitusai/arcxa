//! SPARQL DTOs
//!
//! Request and response types for SPARQL query operations.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// =============================================================================
// SPARQL Query DTOs
// =============================================================================

#[derive(Deserialize, ToSchema)]
pub struct SparqlQuery {
    /// SPARQL query string (SELECT, INSERT DATA, etc.)
    pub sparql: String,
}

#[derive(Serialize, ToSchema)]
pub struct SparqlResults {
    /// Query results as JSON values
    pub results: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SparqlResultRow {
    /// Subject URI
    pub subject: String,
    /// Predicate URI
    pub predicate: String,
    /// Object value
    pub object: String,
    /// Named graph (optional)
    pub graph: Option<String>,
}
