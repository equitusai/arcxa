//! SPARQL Query and Update Builder
//!
//! This module provides safe, injection-resistant SPARQL query construction
//! for scatter-gather operations across distributed shards.
//!
//! ## Features
//!
//! - **Safe Query Building**: URI validation prevents SPARQL injection
//! - **SPARQL 1.1 UPDATE**: Full support for INSERT, DELETE, CLEAR operations
//! - **Named Graph Support**: Operations on specific named graphs
//! - **Performance**: Zero-copy where possible, minimal allocations
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
//!
//! // Build CLEAR GRAPH operation
//! let sparql = SparqlUpdateBuilder::clear_graph("http://example.com/graph")
//!     .expect("Valid graph URI");
//!
//! assert_eq!(sparql, "CLEAR GRAPH <http://example.com/graph>");
//! ```

mod builder;
mod insert_parser;
mod validator;

pub use builder::SparqlUpdateBuilder;
pub use insert_parser::InsertParser;
pub use validator::{is_valid_sparql_uri, sanitize_uri};
