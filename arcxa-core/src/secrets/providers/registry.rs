//! Secret store registry and factory

use super::{
    AwsSecretsManagerStore, EnvSecretStore, FileSecretStore, InlineSecretStore, VaultSecretStore,
};
use crate::secrets::{
    cache::SecretCache, config::ProviderConfig, SecretError, SecretResult, SecretStoreConfig,
    SecretStoreRef, SecretStoreType,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Secret store registry
///
/// Manages multiple secret store providers and provides factory methods
/// to create stores based on configuration.
pub struct SecretStoreRegistry {
    /// Registered stores by name
    stores: RwLock<HashMap<String, SecretStoreRef>>,

    /// Default store to use
    default_store: RwLock<Option<SecretStoreRef>>,

    /// Optional shared cache
    cache: Option<Arc<SecretCache>>,
}

impl SecretStoreRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            stores: RwLock::new(HashMap::new()),
            default_store: RwLock::new(None),
            cache: None,
        }
    }

    /// Create a registry with caching enabled
    pub fn with_cache(ttl_seconds: u64, max_entries: usize) -> Self {
        Self {
            stores: RwLock::new(HashMap::new()),
            default_store: RwLock::new(None),
            cache: Some(Arc::new(SecretCache::new(ttl_seconds, max_entries))),
        }
    }

    /// Register a secret store with a name
    pub fn register(&self, name: impl Into<String>, store: SecretStoreRef) {
        let mut stores = self.stores.write();
        stores.insert(name.into(), store);
    }

    /// Get a registered store by name
    pub fn get(&self, name: &str) -> Option<SecretStoreRef> {
        let stores = self.stores.read();
        stores.get(name).cloned()
    }

    /// Set the default store
    pub fn set_default(&self, store: SecretStoreRef) {
        let mut default = self.default_store.write();
        *default = Some(store);
    }

    /// Get the default store
    pub fn default(&self) -> Option<SecretStoreRef> {
        let default = self.default_store.read();
        default.clone()
    }

    /// Create a secret store from configuration
    pub fn create_from_config(&self, config: &SecretStoreConfig) -> SecretResult<SecretStoreRef> {
        let store: SecretStoreRef = match &config.store_type {
            SecretStoreType::Inline => Arc::new(InlineSecretStore::new()),
            SecretStoreType::Env => Arc::new(EnvSecretStore::new()),
            SecretStoreType::File => match &config.provider_config {
                ProviderConfig::File { directory, format } => Arc::new(
                    FileSecretStore::with_directory_and_format(directory.clone(), format)?,
                ),
                ProviderConfig::Inline
                | ProviderConfig::Env { .. }
                | ProviderConfig::Vault { .. }
                | ProviderConfig::AwsSecretsManager { .. }
                | ProviderConfig::AzureKeyVault { .. }
                | ProviderConfig::GcpSecretManager { .. } => {
                    return Err(SecretError::ConfigurationError(
                        "File secret store requires provider config 'file'".to_string(),
                    ));
                }
            },
            SecretStoreType::Vault => Arc::new(VaultSecretStore::new()),
            SecretStoreType::AwsSecretsManager => Arc::new(AwsSecretsManagerStore::new()),
            SecretStoreType::AzureKeyVault => {
                return Err(SecretError::Internal(
                    "Azure Key Vault not yet implemented".to_string(),
                ));
            }
            SecretStoreType::GcpSecretManager => {
                return Err(SecretError::Internal(
                    "GCP Secret Manager not yet implemented".to_string(),
                ));
            }
        };

        // TODO: Wrap in caching layer if enabled
        // if config.enable_cache {
        //     store = Arc::new(CachedSecretStore::new(store, cache));
        // }

        Ok(store)
    }

    /// List all registered store names
    pub fn list_stores(&self) -> Vec<String> {
        let stores = self.stores.read();
        stores.keys().cloned().collect()
    }

    /// Remove a store from registry
    pub fn unregister(&self, name: &str) -> bool {
        let mut stores = self.stores.write();
        stores.remove(name).is_some()
    }

    /// Clear all registered stores
    pub fn clear(&self) {
        let mut stores = self.stores.write();
        stores.clear();
        let mut default = self.default_store.write();
        *default = None;
    }

    /// Get cache statistics (if caching is enabled)
    pub fn cache_stats(&self) -> Option<crate::secrets::cache::CacheStats> {
        self.cache.as_ref().map(|cache| cache.stats())
    }
}

impl Default for SecretStoreRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::config::ProviderConfig;

    #[test]
    fn test_registry_register_and_get() {
        let registry = SecretStoreRegistry::new();
        let store: SecretStoreRef = Arc::new(InlineSecretStore::new());

        registry.register("test", store.clone());

        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_registry_default() {
        let registry = SecretStoreRegistry::new();
        let store: SecretStoreRef = Arc::new(InlineSecretStore::new());

        registry.set_default(store.clone());

        let default = registry.default();
        assert!(default.is_some());
    }

    #[test]
    fn test_registry_create_from_config() {
        let registry = SecretStoreRegistry::new();

        let config = SecretStoreConfig {
            store_type: SecretStoreType::Inline,
            enable_cache: false,
            cache_ttl_seconds: 300,
            provider_config: ProviderConfig::Inline,
        };

        let store = registry.create_from_config(&config);
        assert!(store.is_ok());
    }

    #[test]
    fn test_registry_list_stores() {
        let registry = SecretStoreRegistry::new();
        let store1: SecretStoreRef = Arc::new(InlineSecretStore::new());
        let store2: SecretStoreRef = Arc::new(EnvSecretStore::new());

        registry.register("store1", store1);
        registry.register("store2", store2);

        let stores = registry.list_stores();
        assert_eq!(stores.len(), 2);
        assert!(stores.contains(&"store1".to_string()));
        assert!(stores.contains(&"store2".to_string()));
    }

    #[test]
    fn test_registry_unregister() {
        let registry = SecretStoreRegistry::new();
        let store: SecretStoreRef = Arc::new(InlineSecretStore::new());

        registry.register("test", store);
        assert!(registry.get("test").is_some());

        let removed = registry.unregister("test");
        assert!(removed);
        assert!(registry.get("test").is_none());
    }
}
