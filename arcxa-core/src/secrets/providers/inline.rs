//! Inline secret store (in-memory, for development/testing)

use crate::secrets::{Secret, SecretError, SecretMetadata, SecretResult, SecretStore, SecretValue};
use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

/// In-memory secret store for development and testing
///
/// **Warning**: This stores secrets in memory and should only be used for:
/// - Local development
/// - Testing
/// - Proof of concept deployments
///
/// For production, use Vault, AWS Secrets Manager, or other secure stores.
pub struct InlineSecretStore {
    secrets: RwLock<HashMap<String, Vec<Secret>>>,
}

impl InlineSecretStore {
    /// Create a new inline secret store
    pub fn new() -> Self {
        Self {
            secrets: RwLock::new(HashMap::new()),
        }
    }

    /// Create with pre-populated secrets (for testing)
    pub fn with_secrets(secrets: HashMap<String, SecretValue>) -> Self {
        let store = Self::new();

        for (path, value) in secrets {
            let _ = futures::executor::block_on(store.put_secret(&path, value, None));
        }

        store
    }
}

impl Default for InlineSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for InlineSecretStore {
    fn name(&self) -> &'static str {
        "inline"
    }

    async fn put_secret(
        &self,
        path: &str,
        secret: SecretValue,
        metadata: Option<SecretMetadata>,
    ) -> SecretResult<String> {
        let version = Uuid::new_v4().to_string();
        let now = Utc::now();

        let secret_obj = Secret {
            path: path.to_string(),
            value: secret,
            metadata: metadata.unwrap_or_default(),
            version: version.clone(),
            created_at: now,
            updated_at: now,
        };

        let mut secrets = self.secrets.write();
        secrets
            .entry(path.to_string())
            .or_insert_with(Vec::new)
            .push(secret_obj);

        Ok(version)
    }

    async fn get_secret(&self, path: &str, version: Option<&str>) -> SecretResult<Secret> {
        let secrets = self.secrets.read();

        let versions = secrets
            .get(path)
            .ok_or_else(|| SecretError::NotFound(format!("Secret not found: {}", path)))?;

        if versions.is_empty() {
            return Err(SecretError::NotFound(format!("Secret not found: {}", path)));
        }

        let secret = if let Some(ver) = version {
            // Find specific version
            versions.iter().find(|s| s.version == ver).ok_or_else(|| {
                SecretError::NotFound(format!("Version {} not found for secret: {}", ver, path))
            })?
        } else {
            // Return latest version
            versions
                .last()
                .ok_or_else(|| SecretError::NotFound(format!("Secret not found: {}", path)))?
        };

        Ok(secret.clone())
    }

    async fn delete_secret(&self, path: &str, version: Option<&str>) -> SecretResult<()> {
        let mut secrets = self.secrets.write();

        if let Some(ver) = version {
            // Delete specific version
            if let Some(versions) = secrets.get_mut(path) {
                versions.retain(|s| s.version != ver);
                if versions.is_empty() {
                    secrets.remove(path);
                }
            }
        } else {
            // Delete all versions
            secrets.remove(path);
        }

        Ok(())
    }

    async fn list_secrets(&self, prefix: Option<&str>) -> SecretResult<Vec<String>> {
        let secrets = self.secrets.read();

        let paths: Vec<String> = secrets
            .keys()
            .filter(|path| {
                if let Some(p) = prefix {
                    path.starts_with(p)
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        Ok(paths)
    }

    async fn exists(&self, path: &str) -> SecretResult<bool> {
        let secrets = self.secrets.read();
        Ok(secrets.contains_key(path))
    }

    async fn rotate_secret(&self, path: &str, new_secret: SecretValue) -> SecretResult<String> {
        // Get existing metadata
        let metadata = self.get_metadata(path).await.ok();

        // Put new version
        let version = self.put_secret(path, new_secret, metadata).await?;

        Ok(version)
    }

    async fn get_metadata(&self, path: &str) -> SecretResult<SecretMetadata> {
        let secrets = self.secrets.read();

        let versions = secrets
            .get(path)
            .ok_or_else(|| SecretError::NotFound(format!("Secret not found: {}", path)))?;

        let secret = versions
            .last()
            .ok_or_else(|| SecretError::NotFound(format!("Secret not found: {}", path)))?;

        Ok(secret.metadata.clone())
    }

    async fn health_check(&self) -> SecretResult<bool> {
        // Always healthy for in-memory store
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inline_put_and_get() {
        let store = InlineSecretStore::new();

        let path = "test/secret";
        let secret_value = SecretValue::from_credentials("user", "pass");

        let version = store.put_secret(path, secret_value, None).await.unwrap();
        assert!(!version.is_empty());

        let retrieved = store.get_secret(path, None).await.unwrap();
        assert_eq!(retrieved.value.username(), Some("user"));
        assert_eq!(retrieved.value.password(), Some("pass"));
    }

    #[tokio::test]
    async fn test_inline_versioning() {
        let store = InlineSecretStore::new();

        let path = "test/secret";

        // Put first version
        let v1 = store
            .put_secret(path, SecretValue::from_string("v1"), None)
            .await
            .unwrap();

        // Put second version
        let _v2 = store
            .put_secret(path, SecretValue::from_string("v2"), None)
            .await
            .unwrap();

        // Get latest (should be v2)
        let latest = store.get_secret(path, None).await.unwrap();
        assert_eq!(latest.value.as_string(), Some("v2"));

        // Get specific version v1
        let first = store.get_secret(path, Some(&v1)).await.unwrap();
        assert_eq!(first.value.as_string(), Some("v1"));
    }

    #[tokio::test]
    async fn test_inline_list() {
        let store = InlineSecretStore::new();

        store
            .put_secret(
                "prod/db/postgres",
                SecretValue::from_string("secret1"),
                None,
            )
            .await
            .unwrap();
        store
            .put_secret("prod/api/key", SecretValue::from_string("secret2"), None)
            .await
            .unwrap();
        store
            .put_secret("dev/db/postgres", SecretValue::from_string("secret3"), None)
            .await
            .unwrap();

        // List all
        let all = store.list_secrets(None).await.unwrap();
        assert_eq!(all.len(), 3);

        // List with prefix
        let prod = store.list_secrets(Some("prod/")).await.unwrap();
        assert_eq!(prod.len(), 2);
    }

    #[tokio::test]
    async fn test_inline_delete() {
        let store = InlineSecretStore::new();

        let path = "test/secret";
        store
            .put_secret(path, SecretValue::from_string("secret"), None)
            .await
            .unwrap();

        assert!(store.exists(path).await.unwrap());

        store.delete_secret(path, None).await.unwrap();

        assert!(!store.exists(path).await.unwrap());
    }

    #[tokio::test]
    async fn test_inline_rotate() {
        let store = InlineSecretStore::new();

        let path = "test/secret";

        // Initial secret
        store
            .put_secret(path, SecretValue::from_string("old"), None)
            .await
            .unwrap();

        // Rotate
        let new_version = store
            .rotate_secret(path, SecretValue::from_string("new"))
            .await
            .unwrap();

        // Get latest should return new secret
        let latest = store.get_secret(path, None).await.unwrap();
        assert_eq!(latest.value.as_string(), Some("new"));
        assert_eq!(latest.version, new_version);
    }
}
