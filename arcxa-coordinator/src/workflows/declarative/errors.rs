//! Error types for declarative workflow parsing and building

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during parsing
#[derive(Debug, Error)]
pub enum ParseError {
    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// IO error reading file
    #[error("IO error reading file {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },

    /// YAML parsing error
    #[error("YAML parsing error in {path}: {source}")]
    YamlError {
        path: PathBuf,
        source: serde_yaml::Error,
    },

    /// JSON parsing error
    #[error("JSON parsing error in {path}: {source}")]
    JsonError {
        path: PathBuf,
        source: serde_json::Error,
    },

    /// Unsupported file format
    #[error("Unsupported file format: {0}. Expected .yaml, .yml, or .json")]
    UnsupportedFormat(String),

    /// Invalid content (empty file)
    #[error("File is empty: {0}")]
    EmptyFile(PathBuf),

    /// Invalid API version
    #[error("Invalid API version: {found}. Expected: {expected}")]
    InvalidApiVersion { found: String, expected: String },

    /// Invalid kind
    #[error("Invalid kind: {found}. Expected: {expected}")]
    InvalidKind { found: String, expected: String },

    /// Schema validation failed during parse
    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    /// Custom parse error
    #[error("{0}")]
    Custom(String),
}

/// Errors that can occur during building
#[derive(Debug, Error)]
pub enum BuildError {
    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid field value
    #[error("Invalid value for field '{field}': {reason}")]
    InvalidValue { field: String, reason: String },

    /// Route reference not found
    #[error("Route '{0}' not found in workflow")]
    RouteNotFound(String),

    /// Duplicate route name
    #[error("Duplicate route name: {0}")]
    DuplicateRoute(String),

    /// Invalid condition
    #[error("Invalid condition in route '{route}': {reason}")]
    InvalidCondition { route: String, reason: String },

    /// Invalid action
    #[error("Invalid action in route '{route}': {reason}")]
    InvalidAction { route: String, reason: String },

    /// Invalid schedule
    #[error("Invalid schedule configuration: {0}")]
    InvalidSchedule(String),

    /// Invalid resource specification
    #[error("Invalid resource specification: {0}")]
    InvalidResource(String),

    /// Validation error from graphica-core
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    /// Custom build error
    #[error("{0}")]
    Custom(String),
}

impl From<graphica_core::workflows::ValidationError> for BuildError {
    fn from(err: graphica_core::workflows::ValidationError) -> Self {
        BuildError::ValidationFailed(err.to_string())
    }
}
