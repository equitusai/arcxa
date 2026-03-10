//! SHACL Module
//!
//! SHACL (Shapes Constraint Language) parsing and representation.

pub mod parser;
pub mod types;

pub use parser::ShaclParser;
pub use types::{NodeKind, NodeShape, PropertyShape, SeverityLevel};
