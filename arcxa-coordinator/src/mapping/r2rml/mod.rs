//! # R2RML Mapping Module
//!
//! Semi-automated semantic mapping from CSV/Parquet to RDF using R2RML (RDB to RDF Mapping Language).
//!
//! ## Architecture
//!
//! This module implements the W3C R2RML specification for mapping relational data to RDF triples.
//! It integrates with the Source Profiling module (Sprint 1.1) to suggest intelligent mappings.
//!
//! ## Workflow
//!
//! ```text
//! ProfileResult (from Sprint 1.1)
//!     ↓
//! Mapping Suggestion Engine
//!     ↓
//! R2RML Types (TriplesMap, SubjectMap, PredicateObjectMap)
//!     ↓
//! R2RML Serialization (Turtle format)
//!     ↓
//! RDF Store (Governance Brain)
//!     ↓
//! R2RML Executor (apply to CSV records)
//!     ↓
//! RDF Triples (output)
//! ```
//!
//! ## Module Structure
//!
//! - `types/` - R2RML data structures (TriplesMap, SubjectMap, PredicateObjectMap, ObjectMap)
//! - `serialization/` - R2RML Turtle serialization
//! - `executor/` - R2RML mapping executor (apply to CSV records)
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::mapping::r2rml::{R2rmlMapping, TriplesMap};
//!
//! // Define a mapping
//! let mapping = R2rmlMapping {
//!     base_uri: "http://example.com/".to_string(),
//!     triples_maps: vec![
//!         TriplesMap {
//!             name: "CustomerMap".to_string(),
//!             logical_table: "customers.csv".to_string(),
//!             subject_map: SubjectMap {
//!                 template: "http://example.com/customer/{customer_id}".to_string(),
//!                 class: Some("schema:Person".to_string()),
//!             },
//!             predicate_object_maps: vec![
//!                 PredicateObjectMap {
//!                     predicate: "schema:name".to_string(),
//!                     object_map: ObjectMap::ColumnValueMap {
//!                         column: "full_name".to_string(),
//!                         datatype: Some("xsd:string".to_string()),
//!                     },
//!                 },
//!             ],
//!         },
//!     ],
//! };
//!
//! // Serialize to R2RML Turtle
//! let turtle = mapping.to_r2rml_turtle()?;
//!
//! // Execute mapping on CSV data
//! let executor = R2rmlExecutor::new(mapping);
//! let triples = executor.execute_csv("customers.csv").await?;
//! ```
//!
//! ## W3C R2RML Specification
//!
//! This implementation follows the W3C R2RML specification:
//! https://www.w3.org/TR/r2rml/
//!
//! Key concepts:
//! - **TriplesMap**: Defines how to generate RDF triples from a logical table
//! - **SubjectMap**: Defines how to generate RDF subjects (URIs or blank nodes)
//! - **PredicateObjectMap**: Defines how to generate RDF predicates and objects
//! - **ObjectMap**: Defines how to generate RDF objects (columns, constants, or references)
//! - **LogicalTable**: Source data (CSV file, SQL query, etc.)
//!
//! ## Integration with Sprint 1.1 (Profiling)
//!
//! The R2RML module uses profile results to suggest intelligent mappings:
//!
//! - **Candidate keys** → Subject template URIs
//! - **Column types** → XSD datatype annotations
//! - **Semantic types** → Predicate selection (schema:email, schema:telephone)
//! - **Cardinality** → Key/non-key detection
//!

pub mod executor;
pub mod serialization;
pub mod types;

pub use executor::R2rmlExecutor;
pub use serialization::R2rmlSerializer;
pub use types::*;
