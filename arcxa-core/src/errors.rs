//! Typed error system for Graphica
//!
//! This module provides structured error types to replace anyhow errors
//! in critical paths, enabling better error handling and circuit breaking.

use thiserror::Error;

/// Main error type for Graphica operations
#[derive(Error, Debug)]
pub enum GraphicaError {
    /// Storage layer errors (RocksDB, Parquet, etc.)
    #[error("Storage error: {0}")]
    Storage(String),

    /// Ingestion pipeline errors (Kafka, parsing, etc.)
    #[error("Ingestion error: {0}")]
    Ingestion(String),

    /// Governance layer errors (RDF, SPARQL, etc.)
    #[error("Governance error: {0}")]
    Governance(String),

    /// Circuit breaker is open, rejecting requests
    #[error("Circuit breaker open - system protecting itself from failures")]
    CircuitBreakerOpen,

    /// Batch processing timeout
    #[error("Batch processing timeout after {0}ms")]
    BatchTimeout(u64),

    /// IO errors (file system, network, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource not found errors
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Generic errors for gradual migration from anyhow
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias for Graphica operations
pub type Result<T> = std::result::Result<T, GraphicaError>;

/// Convert anyhow errors to GraphicaError for gradual migration
impl From<anyhow::Error> for GraphicaError {
    fn from(err: anyhow::Error) -> Self {
        GraphicaError::Internal(err.to_string())
    }
}

impl GraphicaError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            GraphicaError::BatchTimeout(_) | GraphicaError::Io(_) | GraphicaError::Storage(_)
        )
    }

    /// Check if this error should trigger circuit breaker
    pub fn should_open_circuit(&self) -> bool {
        matches!(
            self,
            GraphicaError::Storage(_) | GraphicaError::Governance(_) | GraphicaError::Io(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = GraphicaError::Storage("test error".to_string());
        assert_eq!(err.to_string(), "Storage error: test error");
    }

    #[test]
    fn test_retryable_errors() {
        assert!(GraphicaError::BatchTimeout(1000).is_retryable());
        assert!(GraphicaError::Storage("test".into()).is_retryable());
        assert!(!GraphicaError::CircuitBreakerOpen.is_retryable());
    }

    #[test]
    fn test_circuit_breaker_errors() {
        assert!(GraphicaError::Storage("test".into()).should_open_circuit());
        assert!(GraphicaError::Governance("test".into()).should_open_circuit());
        assert!(!GraphicaError::CircuitBreakerOpen.should_open_circuit());
    }
}
