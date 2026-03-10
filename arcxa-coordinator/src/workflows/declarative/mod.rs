//! Declarative workflow support - YAML/JSON parsing and building
//!
//! This module provides functionality to parse declarative workflow definitions
//! (YAML/JSON) and convert them to/from domain workflow objects.
//!
//! ## Architecture
//!
//! - **Parser**: Reads YAML/JSON files and deserializes into `WorkflowSchema`
//! - **Builder**: Converts `WorkflowSchema` to domain `Workflow` with validation
//! - **Serializer**: Converts domain `Workflow` back to `WorkflowSchema` for export
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::workflows::declarative::DeclarativeParser;
//!
//! // Parse YAML file
//! let schema = DeclarativeParser::parse_file("workflow.yaml")?;
//!
//! // Build domain workflow
//! let workflow = WorkflowBuilder::build(&schema)?;
//! ```

pub mod builder;
pub mod errors;
pub mod parser;
pub mod serializer;

#[cfg(test)]
mod integration_tests;

pub use builder::WorkflowBuilder;
pub use errors::{BuildError, ParseError};
pub use parser::DeclarativeParser;
pub use serializer::WorkflowSerializer;
