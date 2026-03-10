//! ML model response caching with TTL

use lru::LruCache;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::invoker::{ModelRequest, ModelResponse};

/// Model response cache with LRU eviction
pub struct ModelCache {
    /// LRU cache
    cache: Arc<RwLock<LruCache<u64, CachedResponse>>>,
    /// Cache configuration
    config: CacheConfig,
}

/// Cached response with expiration
struct CachedResponse {
    response: ModelResponse,
    cached_at: Instant,
    ttl: Duration,
}

impl CachedResponse {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size
    pub max_size: usize,
    /// Default TTL for cached responses
    pub default_ttl: Duration,
    /// Per-model TTL overrides
    pub model_ttls: std::collections::HashMap<String, Duration>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            default_ttl: Duration::from_secs(300), // 5 minutes
            model_ttls: Default::default(),
        }
    }
}

impl ModelCache {
    /// Create new model cache
    pub fn new(config: CacheConfig) -> Self {
        let cache = LruCache::new(
            NonZeroUsize::new(config.max_size).unwrap_or(NonZeroUsize::new(10_000).unwrap()),
        );

        Self {
            cache: Arc::new(RwLock::new(cache)),
            config,
        }
    }

    /// Get cached response
    pub async fn get(&self, request: &ModelRequest) -> Option<ModelResponse> {
        let key = Self::cache_key(request);
        let mut cache = self.cache.write().await;

        if let Some(cached) = cache.get(&key) {
            if !cached.is_expired() {
                return Some(cached.response.clone());
            } else {
                // Remove expired entry
                cache.pop(&key);
            }
        }

        None
    }

    /// Put response in cache
    pub async fn put(&self, request: &ModelRequest, response: &ModelResponse) {
        let key = Self::cache_key(request);

        let ttl = self
            .config
            .model_ttls
            .get(&request.model_id)
            .copied()
            .unwrap_or(self.config.default_ttl);

        let cached = CachedResponse {
            response: response.clone(),
            cached_at: Instant::now(),
            ttl,
        };

        let mut cache = self.cache.write().await;
        cache.put(key, cached);
    }

    /// Clear entire cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            size: cache.len(),
            capacity: cache.cap().get(),
        }
    }

    /// Generate cache key from request
    fn cache_key(request: &ModelRequest) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Hash model_id
        request.model_id.hash(&mut hasher);

        // Hash features by serializing to JSON (deterministic)
        if let Ok(json) = serde_json::to_string(&request.features) {
            json.hash(&mut hasher);
        }

        // Hash timeout
        request.timeout_ms.hash(&mut hasher);

        hasher.finish()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_cache_put_get() {
        let config = CacheConfig::default();
        let cache = ModelCache::new(config);

        let request = ModelRequest {
            model_id: "test_model".to_string(),
            features: HashMap::new(),
            timeout_ms: Some(500),
        };

        let response = ModelResponse {
            model_id: "test_model".to_string(),
            predictions: HashMap::new(),
            confidence: 0.9,
            model_version: Some("v1".to_string()),
        };

        // Initially not in cache
        assert!(cache.get(&request).await.is_none());

        // Put in cache
        cache.put(&request, &response).await;

        // Now should be in cache
        let cached = cache.get(&request).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().model_id, "test_model");
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let mut config = CacheConfig::default();
        config.default_ttl = Duration::from_millis(10); // Very short TTL

        let cache = ModelCache::new(config);

        let request = ModelRequest {
            model_id: "test_model".to_string(),
            features: HashMap::new(),
            timeout_ms: Some(500),
        };

        let response = ModelResponse {
            model_id: "test_model".to_string(),
            predictions: HashMap::new(),
            confidence: 0.9,
            model_version: Some("v1".to_string()),
        };

        cache.put(&request, &response).await;

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Should be expired
        assert!(cache.get(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let config = CacheConfig::default();
        let cache = ModelCache::new(config);

        let request = ModelRequest {
            model_id: "test_model".to_string(),
            features: HashMap::new(),
            timeout_ms: Some(500),
        };

        let response = ModelResponse {
            model_id: "test_model".to_string(),
            predictions: HashMap::new(),
            confidence: 0.9,
            model_version: Some("v1".to_string()),
        };

        cache.put(&request, &response).await;
        assert!(cache.get(&request).await.is_some());

        cache.clear().await;
        assert!(cache.get(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let config = CacheConfig {
            max_size: 100,
            ..Default::default()
        };
        let cache = ModelCache::new(config);

        let stats = cache.stats().await;
        assert_eq!(stats.size, 0);
        assert_eq!(stats.capacity, 100);
    }
}
