//! Secret Management System
//!
//! Provides a pluggable, production-grade secret storage system supporting
//! multiple backends (Vault, AWS Secrets Manager, env vars, etc.).

pub mod cache;
pub mod config;
pub mod error;
pub mod providers;
pub mod types;

pub use config::{SecretStoreConfig, SecretStoreType};
pub use error::{SecretError, SecretResult};
pub use types::{Secret, SecretMetadata, SecretValue, SecretVersion};

use async_trait::async_trait;
use std::sync::Arc;

/// Core trait for secret store implementations
///
/// All secret store backends must implement this trait.
/// Implementations should be thread-safe (Arc-compatible).
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Get the name of this secret store implementation
    fn name(&self) -> &'static str;

    /// Store a secret
    ///
    /// # Arguments
    /// * `path` - Secret path/key (e.g., "datasources/postgres-prod")
    /// * `secret` - Secret value to store
    /// * `metadata` - Optional metadata (tags, TTL, etc.)
    ///
    /// # Returns
    /// Version ID of the stored secret
    async fn put_secret(
        &self,
        path: &str,
        secret: SecretValue,
        metadata: Option<SecretMetadata>,
    ) -> SecretResult<String>;

    /// Retrieve a secret by path
    ///
    /// # Arguments
    /// * `path` - Secret path/key
    /// * `version` - Optional specific version (None = latest)
    ///
    /// # Returns
    /// The secret with its metadata
    async fn get_secret(&self, path: &str, version: Option<&str>) -> SecretResult<Secret>;

    /// Delete a secret
    ///
    /// # Arguments
    /// * `path` - Secret path/key
    /// * `version` - Optional specific version (None = all versions)
    async fn delete_secret(&self, path: &str, version: Option<&str>) -> SecretResult<()>;

    /// List all secret paths (keys only, not values)
    ///
    /// # Arguments
    /// * `prefix` - Optional prefix to filter by
    async fn list_secrets(&self, prefix: Option<&str>) -> SecretResult<Vec<String>>;

    /// Check if a secret exists
    async fn exists(&self, path: &str) -> SecretResult<bool>;

    /// Rotate a secret (create new version, optionally mark old as deprecated)
    async fn rotate_secret(&self, path: &str, new_secret: SecretValue) -> SecretResult<String>;

    /// Get secret metadata without retrieving the value
    async fn get_metadata(&self, path: &str) -> SecretResult<SecretMetadata>;

    /// Health check for the secret store
    async fn health_check(&self) -> SecretResult<bool>;
}

/// Type alias for Arc-wrapped secret store
pub type SecretStoreRef = Arc<dyn SecretStore>;
