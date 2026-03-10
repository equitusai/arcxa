//! Persistence Error Types
//!
//! Defines error types for the persistence layer, providing consistent
//! error handling across storage backends.

use std::fmt;

/// Result type alias for persistence operations
pub type Result<T> = std::result::Result<T, PersistenceError>;

/// Errors that can occur during persistence operations
#[derive(Debug)]
pub enum PersistenceError {
    /// Storage backend not available or connection failed
    StorageUnavailable { backend: String, reason: String },

    /// Failed to serialize data for storage
    SerializationFailed {
        entity_type: String,
        entity_id: String,
        reason: String,
    },

    /// Failed to deserialize data from storage
    DeserializationFailed {
        entity_type: String,
        entity_id: String,
        reason: String,
    },

    /// Entity not found in storage
    NotFound {
        entity_type: String,
        entity_id: String,
    },

    /// Entity already exists (duplicate key)
    AlreadyExists {
        entity_type: String,
        entity_id: String,
    },

    /// Transaction failed (e.g., optimistic locking failure)
    TransactionFailed { operation: String, reason: String },

    /// Storage corruption detected
    CorruptedData {
        entity_type: String,
        entity_id: String,
        details: String,
    },

    /// Storage quota or capacity exceeded
    QuotaExceeded { current_size: u64, max_size: u64 },

    /// Invalid query or operation
    InvalidOperation { operation: String, reason: String },

    /// Index maintenance failed
    IndexError { index_name: String, reason: String },

    /// Checkpoint operation failed
    CheckpointFailed {
        checkpoint_id: String,
        reason: String,
    },

    /// Recovery operation failed
    RecoveryFailed { stage: String, reason: String },

    /// Migration operation failed
    MigrationFailed { phase: String, reason: String },

    /// Internal storage error (e.g., RocksDB internal error)
    InternalError { backend: String, details: String },

    /// I/O error during storage operations
    IoError {
        operation: String,
        path: String,
        source: std::io::Error,
    },
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::StorageUnavailable { backend, reason } => {
                write!(f, "Storage backend '{}' unavailable: {}", backend, reason)
            }
            PersistenceError::SerializationFailed {
                entity_type,
                entity_id,
                reason,
            } => {
                write!(
                    f,
                    "Failed to serialize {} '{}': {}",
                    entity_type, entity_id, reason
                )
            }
            PersistenceError::DeserializationFailed {
                entity_type,
                entity_id,
                reason,
            } => {
                write!(
                    f,
                    "Failed to deserialize {} '{}': {}",
                    entity_type, entity_id, reason
                )
            }
            PersistenceError::NotFound {
                entity_type,
                entity_id,
            } => {
                write!(f, "{} '{}' not found", entity_type, entity_id)
            }
            PersistenceError::AlreadyExists {
                entity_type,
                entity_id,
            } => {
                write!(f, "{} '{}' already exists", entity_type, entity_id)
            }
            PersistenceError::TransactionFailed { operation, reason } => {
                write!(f, "Transaction failed for '{}': {}", operation, reason)
            }
            PersistenceError::CorruptedData {
                entity_type,
                entity_id,
                details,
            } => {
                write!(
                    f,
                    "Corrupted data detected for {} '{}': {}",
                    entity_type, entity_id, details
                )
            }
            PersistenceError::QuotaExceeded {
                current_size,
                max_size,
            } => {
                write!(
                    f,
                    "Storage quota exceeded: current {} bytes, max {} bytes",
                    current_size, max_size
                )
            }
            PersistenceError::InvalidOperation { operation, reason } => {
                write!(f, "Invalid operation '{}': {}", operation, reason)
            }
            PersistenceError::IndexError { index_name, reason } => {
                write!(f, "Index '{}' error: {}", index_name, reason)
            }
            PersistenceError::CheckpointFailed {
                checkpoint_id,
                reason,
            } => {
                write!(f, "Checkpoint '{}' failed: {}", checkpoint_id, reason)
            }
            PersistenceError::RecoveryFailed { stage, reason } => {
                write!(f, "Recovery failed at stage '{}': {}", stage, reason)
            }
            PersistenceError::MigrationFailed { phase, reason } => {
                write!(f, "Migration failed at phase '{}': {}", phase, reason)
            }
            PersistenceError::InternalError { backend, details } => {
                write!(f, "Internal error in backend '{}': {}", backend, details)
            }
            PersistenceError::IoError {
                operation,
                path,
                source,
            } => {
                write!(
                    f,
                    "I/O error during '{}' at '{}': {}",
                    operation, path, source
                )
            }
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PersistenceError::IoError { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(err: std::io::Error) -> Self {
        PersistenceError::IoError {
            operation: "unknown".to_string(),
            path: "unknown".to_string(),
            source: err,
        }
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(err: serde_json::Error) -> Self {
        PersistenceError::SerializationFailed {
            entity_type: "unknown".to_string(),
            entity_id: "unknown".to_string(),
            reason: err.to_string(),
        }
    }
}

// Helper methods for common error scenarios
impl PersistenceError {
    /// Create a simple serialization error with a message
    pub fn serialization(reason: impl ToString) -> Self {
        PersistenceError::SerializationFailed {
            entity_type: "WorkflowExecution".to_string(),
            entity_id: "unknown".to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a simple internal error
    pub fn internal(backend: impl ToString, details: impl ToString) -> Self {
        PersistenceError::InternalError {
            backend: backend.to_string(),
            details: details.to_string(),
        }
    }

    /// Create a not found error for an execution
    pub fn execution_not_found(id: impl ToString) -> Self {
        PersistenceError::NotFound {
            entity_type: "WorkflowExecution".to_string(),
            entity_id: id.to_string(),
        }
    }

    /// Create a storage unavailable error
    pub fn storage_unavailable(backend: impl ToString, reason: impl ToString) -> Self {
        PersistenceError::StorageUnavailable {
            backend: backend.to_string(),
            reason: reason.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PersistenceError::NotFound {
            entity_type: "WorkflowExecution".to_string(),
            entity_id: "exec_123".to_string(),
        };
        assert_eq!(err.to_string(), "WorkflowExecution 'exec_123' not found");

        let err = PersistenceError::StorageUnavailable {
            backend: "RocksDB".to_string(),
            reason: "Connection timeout".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Storage backend 'RocksDB' unavailable: Connection timeout"
        );
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let persistence_err: PersistenceError = io_err.into();

        match persistence_err {
            PersistenceError::IoError { .. } => {}
            _ => panic!("Expected IoError variant"),
        }
    }
}
