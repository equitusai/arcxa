//! Secret store errors

use thiserror::Error;

/// Secret store error types
#[derive(Error, Debug)]
pub enum SecretError {
    /// Secret not found
    #[error("Secret not found: {0}")]
    NotFound(String),

    /// Access denied
    #[error("Access denied: {0}")]
    AccessDenied(String),

    /// Invalid secret format
    #[error("Invalid secret format: {0}")]
    InvalidFormat(String),

    /// Secret already exists
    #[error("Secret already exists: {0}")]
    AlreadyExists(String),

    /// Secret expired
    #[error("Secret expired: {0}")]
    Expired(String),

    /// Connection error to secret store backend
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Provider-specific error
    #[error("Provider error ({provider}): {message}")]
    ProviderError { provider: String, message: String },

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for secret operations
pub type SecretResult<T> = Result<T, SecretError>;

impl From<serde_json::Error> for SecretError {
    fn from(err: serde_json::Error) -> Self {
        SecretError::SerializationError(err.to_string())
    }
}

impl From<reqwest::Error> for SecretError {
    fn from(err: reqwest::Error) -> Self {
        SecretError::ConnectionError(err.to_string())
    }
}
