//! RDF Term Parsing and Validation
//!
//! This module provides production-quality parsing and validation for RDF terms,
//! including URIs, literals (plain, typed, language-tagged), and blank nodes.
//!
//! ## Features
//!
//! - **State Machine Parser**: Correctly handles escaped characters in literals
//! - **BCP 47 Validation**: Language tags validated against RFC 5646
//! - **XSD Datatype Support**: Common XSD datatypes recognized and validated
//! - **Performance**: Zero-copy where possible, minimal allocations
//! - **RDF 1.1 Compliant**: Follows W3C RDF 1.1 specification
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::shard_coordinator::rdf::parse_rdf_object;
//!
//! // Parse a typed literal
//! let (value, datatype, language) = parse_rdf_object(r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#)
//!     .expect("Valid RDF literal");
//!
//! assert_eq!(value, "\"42\"");
//! assert_eq!(datatype, "http://www.w3.org/2001/XMLSchema#integer");
//! assert_eq!(language, "");
//! ```

mod datatype;
mod language;
mod literal;
mod term;

pub use datatype::is_valid_xsd_datatype;
pub use language::is_valid_language_tag;
pub use literal::parse_rdf_object;
pub use term::{build_validated_triple, RdfTerm};
