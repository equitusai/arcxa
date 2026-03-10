//! Error types for workflow execution with row storage

use serde::{Deserialize, Serialize};
use std::fmt;

/// Workflow execution errors
#[derive(Debug)]
pub enum WorkflowError {
    /// Data not found
    DataNotFound(String),

    /// Invalid data format
    InvalidData(String),

    /// Storage operation failed
    Storage(String),

    /// Serialization/deserialization error
    Serialization(String),

    /// I/O error
    IoError(String),

    /// Resource limit exceeded
    ResourceLimit(String),

    /// Feature not implemented
    NotImplemented(String),

    /// Generic error
    Other(String),
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowError::DataNotFound(msg) => write!(f, "Data not found: {}", msg),
            WorkflowError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            WorkflowError::Storage(msg) => write!(f, "Storage error: {}", msg),
            WorkflowError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            WorkflowError::IoError(msg) => write!(f, "I/O error: {}", msg),
            WorkflowError::ResourceLimit(msg) => write!(f, "Resource limit exceeded: {}", msg),
            WorkflowError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
            WorkflowError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for WorkflowError {}

impl From<std::io::Error> for WorkflowError {
    fn from(err: std::io::Error) -> Self {
        WorkflowError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for WorkflowError {
    fn from(err: serde_json::Error) -> Self {
        WorkflowError::Serialization(err.to_string())
    }
}

impl From<anyhow::Error> for WorkflowError {
    fn from(err: anyhow::Error) -> Self {
        WorkflowError::Other(err.to_string())
    }
}

/// Result type for workflow operations
pub type Result<T> = std::result::Result<T, WorkflowError>;

/// Error category for retry logic and circuit breaker integration
///
/// Categorizes errors to determine appropriate handling strategy:
/// - Retryable errors: temporary issues that may succeed on retry
/// - Permanent errors: non-retryable issues requiring intervention
/// - Fatal errors: system-level failures that should abort workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowErrorCategory {
    // Retryable errors (transient issues)
    /// Network connection issues, TCP resets
    ConnectionError,
    /// Operation timeouts (query, connection, etc.)
    TimeoutError,
    /// Transaction conflicts (deadlocks, lock timeouts)
    TransactionError,
    /// Temporary resource unavailability (pool exhausted, temp file locked)
    TemporaryResourceError,

    // Permanent errors (non-retryable, require user intervention)
    /// Data validation failures (schema mismatch, constraint violations)
    DataValidationError,
    /// Configuration errors (invalid params, missing required fields)
    ConfigurationError,
    /// Authentication failures (invalid credentials)
    AuthenticationError,
    /// Authorization failures (permission denied)
    AuthorizationError,
    /// Resource not found (table, file, endpoint)
    NotFoundError,

    // Fatal errors (abort workflow immediately)
    /// System-level errors (OOM, disk full, kernel panic)
    SystemError,
    /// Internal corruption or unexpected panic
    InternalError,
}

impl WorkflowErrorCategory {
    /// Returns true if error is retryable (transient failure)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionError
                | Self::TimeoutError
                | Self::TransactionError
                | Self::TemporaryResourceError
        )
    }

    /// Returns true if error is fatal (should abort workflow)
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::SystemError | Self::InternalError)
    }

    /// Convert to string label for metrics
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConnectionError => "connection_error",
            Self::TimeoutError => "timeout_error",
            Self::TransactionError => "transaction_error",
            Self::TemporaryResourceError => "temporary_resource_error",
            Self::DataValidationError => "data_validation_error",
            Self::ConfigurationError => "configuration_error",
            Self::AuthenticationError => "authentication_error",
            Self::AuthorizationError => "authorization_error",
            Self::NotFoundError => "not_found_error",
            Self::SystemError => "system_error",
            Self::InternalError => "internal_error",
        }
    }
}
