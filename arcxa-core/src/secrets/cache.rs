//! Secret caching layer for performance

use super::Secret;
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Cached secret with expiry
#[derive(Debug, Clone)]
struct CachedSecret {
    secret: Secret,
    expires_at: DateTime<Utc>,
}

/// Thread-safe LRU cache for secrets
pub struct SecretCache {
    /// Cache storage
    cache: Arc<RwLock<HashMap<String, CachedSecret>>>,

    /// Cache TTL in seconds
    ttl_seconds: u64,

    /// Maximum cache entries
    max_entries: usize,
}

impl SecretCache {
    /// Create a new secret cache
    pub fn new(ttl_seconds: u64, max_entries: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl_seconds,
            max_entries,
        }
    }

    /// Get a secret from cache
    pub fn get(&self, path: &str) -> Option<Secret> {
        let cache = self.cache.read();

        if let Some(cached) = cache.get(path) {
            // Check if expired
            if Utc::now() < cached.expires_at {
                return Some(cached.secret.clone());
            }
        }

        None
    }

    /// Put a secret in cache
    pub fn put(&self, path: String, secret: Secret) {
        let mut cache = self.cache.write();

        // Evict if at capacity
        if cache.len() >= self.max_entries {
            // Simple eviction: remove first (oldest) entry
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }

        let expires_at = Utc::now() + Duration::seconds(self.ttl_seconds as i64);
        cache.insert(path, CachedSecret { secret, expires_at });
    }

    /// Invalidate a specific secret
    pub fn invalidate(&self, path: &str) {
        let mut cache = self.cache.write();
        cache.remove(path);
    }

    /// Clear all cached secrets
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read();
        let total_entries = cache.len();

        let expired_entries = cache
            .values()
            .filter(|cached| Utc::now() >= cached.expires_at)
            .count();

        CacheStats {
            total_entries,
            active_entries: total_entries - expired_entries,
            expired_entries,
            max_entries: self.max_entries,
            ttl_seconds: self.ttl_seconds,
        }
    }

    /// Remove expired entries (cleanup)
    pub fn evict_expired(&self) {
        let mut cache = self.cache.write();
        let now = Utc::now();

        cache.retain(|_, cached| now < cached.expires_at);
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
    pub max_entries: usize,
    pub ttl_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::types::{SecretMetadata, SecretValue};

    fn create_test_secret(path: &str) -> Secret {
        Secret {
            path: path.to_string(),
            value: SecretValue::from_string("test-secret"),
            metadata: SecretMetadata::default(),
            version: "v1".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = SecretCache::new(300, 100);
        let secret = create_test_secret("test/secret");

        cache.put("test/secret".to_string(), secret.clone());

        let cached = cache.get("test/secret");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().path, "test/secret");
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = SecretCache::new(300, 100);
        let secret = create_test_secret("test/secret");

        cache.put("test/secret".to_string(), secret);
        assert!(cache.get("test/secret").is_some());

        cache.invalidate("test/secret");
        assert!(cache.get("test/secret").is_none());
    }

    #[test]
    fn test_cache_clear() {
        let cache = SecretCache::new(300, 100);

        cache.put(
            "test/secret1".to_string(),
            create_test_secret("test/secret1"),
        );
        cache.put(
            "test/secret2".to_string(),
            create_test_secret("test/secret2"),
        );

        cache.clear();

        assert!(cache.get("test/secret1").is_none());
        assert!(cache.get("test/secret2").is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = SecretCache::new(300, 100);

        cache.put(
            "test/secret1".to_string(),
            create_test_secret("test/secret1"),
        );
        cache.put(
            "test/secret2".to_string(),
            create_test_secret("test/secret2"),
        );

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.max_entries, 100);
    }
}
