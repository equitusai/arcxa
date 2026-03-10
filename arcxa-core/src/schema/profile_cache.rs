//! Schema Profile Caching Layer
//!
//! Provides an in-memory and persistent caching layer for profiled schemas
//! to avoid expensive re-profiling operations.

use super::UnifiedSchema;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Cache entry for a profiled schema
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// The profiled schema
    schema: UnifiedSchema,

    /// When this entry was cached
    cached_at: DateTime<Utc>,

    /// When this entry expires (optional)
    expires_at: Option<DateTime<Utc>>,

    /// Fingerprint of the source data (for invalidation)
    source_fingerprint: Option<String>,

    /// Number of times this entry has been accessed
    access_count: u64,

    /// Last access time
    last_accessed: DateTime<Utc>,
}

impl CacheEntry {
    fn new(
        schema: UnifiedSchema,
        ttl: Option<Duration>,
        source_fingerprint: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema,
            cached_at: now,
            expires_at: ttl.map(|d| now + d),
            source_fingerprint,
            access_count: 0,
            last_accessed: now,
        }
    }

    fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    fn is_valid(&self, current_fingerprint: Option<&str>) -> bool {
        // Check expiration
        if self.is_expired() {
            return false;
        }

        // Check source fingerprint if provided
        if let (Some(cached_fp), Some(current_fp)) = (&self.source_fingerprint, current_fingerprint)
        {
            if cached_fp != current_fp {
                return false;
            }
        }

        true
    }

    fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}

/// Configuration for profile cache
#[derive(Debug, Clone)]
pub struct ProfileCacheConfig {
    /// Default time-to-live for cache entries
    pub default_ttl: Option<Duration>,

    /// Maximum number of entries in the cache
    pub max_entries: usize,

    /// Enable LRU eviction when max_entries is reached
    pub enable_lru_eviction: bool,

    /// Enable persistent caching to disk
    pub enable_persistent_cache: bool,

    /// Directory for persistent cache files
    pub cache_directory: Option<String>,
}

impl Default for ProfileCacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: Some(Duration::hours(24)), // Cache for 24 hours by default
            max_entries: 1000,
            enable_lru_eviction: true,
            enable_persistent_cache: false,
            cache_directory: None,
        }
    }
}

/// In-memory cache for profiled schemas
pub struct ProfileCache {
    /// Cache storage
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,

    /// Configuration
    config: ProfileCacheConfig,

    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of cache hits
    pub hits: u64,

    /// Total number of cache misses
    pub misses: u64,

    /// Total number of evictions
    pub evictions: u64,

    /// Total number of invalidations
    pub invalidations: u64,

    /// Total size of cached data (approximate)
    pub total_size_bytes: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl ProfileCache {
    /// Create a new profile cache with default configuration
    pub fn new() -> Self {
        Self::with_config(ProfileCacheConfig::default())
    }

    /// Create a new profile cache with custom configuration
    pub fn with_config(config: ProfileCacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get a schema from the cache
    pub fn get(&self, key: &str, source_fingerprint: Option<&str>) -> Option<UnifiedSchema> {
        let mut cache = self.cache.write().unwrap();

        if let Some(entry) = cache.get_mut(key) {
            if entry.is_valid(source_fingerprint) {
                entry.record_access();

                // Update stats
                let mut stats = self.stats.write().unwrap();
                stats.hits += 1;

                return Some(entry.schema.clone());
            } else {
                // Entry is invalid, remove it
                cache.remove(key);

                let mut stats = self.stats.write().unwrap();
                stats.invalidations += 1;
                stats.misses += 1;
            }
        } else {
            // Cache miss
            let mut stats = self.stats.write().unwrap();
            stats.misses += 1;
        }

        None
    }

    /// Put a schema into the cache
    pub fn put(&self, key: String, schema: UnifiedSchema, source_fingerprint: Option<String>) {
        let mut cache = self.cache.write().unwrap();

        // Check if we need to evict entries
        if cache.len() >= self.config.max_entries && self.config.enable_lru_eviction {
            self.evict_lru(&mut cache);
        }

        let entry = CacheEntry::new(schema, self.config.default_ttl, source_fingerprint);
        cache.insert(key, entry);
    }

    /// Put a schema with custom TTL
    pub fn put_with_ttl(
        &self,
        key: String,
        schema: UnifiedSchema,
        ttl: Option<Duration>,
        source_fingerprint: Option<String>,
    ) {
        let mut cache = self.cache.write().unwrap();

        if cache.len() >= self.config.max_entries && self.config.enable_lru_eviction {
            self.evict_lru(&mut cache);
        }

        let entry = CacheEntry::new(schema, ttl, source_fingerprint);
        cache.insert(key, entry);
    }

    /// Invalidate a specific cache entry
    pub fn invalidate(&self, key: &str) -> bool {
        let mut cache = self.cache.write().unwrap();

        if cache.remove(key).is_some() {
            let mut stats = self.stats.write().unwrap();
            stats.invalidations += 1;
            true
        } else {
            false
        }
    }

    /// Invalidate all cache entries
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        let count = cache.len();
        cache.clear();

        let mut stats = self.stats.write().unwrap();
        stats.invalidations += count as u64;
    }

    /// Invalidate all expired entries
    pub fn cleanup_expired(&self) -> usize {
        let mut cache = self.cache.write().unwrap();

        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            cache.remove(&key);
        }

        if count > 0 {
            let mut stats = self.stats.write().unwrap();
            stats.invalidations += count as u64;
        }

        count
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Check if cache contains a key
    pub fn contains(&self, key: &str) -> bool {
        self.cache.read().unwrap().contains_key(key)
    }

    /// Evict least recently used entry
    fn evict_lru(&self, cache: &mut HashMap<String, CacheEntry>) {
        if cache.is_empty() {
            return;
        }

        // Find the least recently used entry
        let lru_key = cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone());

        if let Some(key) = lru_key {
            cache.remove(&key);

            let mut stats = self.stats.write().unwrap();
            stats.evictions += 1;
        }
    }

    /// Generate cache key from source reference and table name
    pub fn generate_key(source_ref: &str, table_name: Option<&str>) -> String {
        match table_name {
            Some(table) => format!("{}::{}", source_ref, table),
            None => source_ref.to_string(),
        }
    }
}

impl Default for ProfileCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{SourceType, UnifiedField, UniversalDataType};

    fn create_test_schema(name: &str) -> UnifiedSchema {
        let mut schema = UnifiedSchema::new(
            name.to_string(),
            SourceType::CsvFile,
            "/tmp/test.csv".to_string(),
        );

        schema.add_field(UnifiedField::new(
            "id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        ));

        schema
    }

    #[test]
    fn test_cache_creation() {
        let cache = ProfileCache::new();
        assert_eq!(cache.size(), 0);

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = ProfileCache::new();
        let schema = create_test_schema("test_table");

        cache.put("test_key".to_string(), schema.clone(), None);
        assert_eq!(cache.size(), 1);

        let retrieved = cache.get("test_key", None);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test_table");

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_cache_miss() {
        let cache = ProfileCache::new();

        let retrieved = cache.get("nonexistent", None);
        assert!(retrieved.is_none());

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = ProfileCache::new();
        let schema = create_test_schema("test_table");

        cache.put("test_key".to_string(), schema, None);
        assert_eq!(cache.size(), 1);

        let invalidated = cache.invalidate("test_key");
        assert!(invalidated);
        assert_eq!(cache.size(), 0);

        let stats = cache.stats();
        assert_eq!(stats.invalidations, 1);
    }

    #[test]
    fn test_cache_clear() {
        let cache = ProfileCache::new();

        for i in 0..5 {
            let schema = create_test_schema(&format!("table_{}", i));
            cache.put(format!("key_{}", i), schema, None);
        }

        assert_eq!(cache.size(), 5);

        cache.clear();
        assert_eq!(cache.size(), 0);

        let stats = cache.stats();
        assert_eq!(stats.invalidations, 5);
    }

    #[test]
    fn test_cache_expiration() {
        let cache = ProfileCache::new();
        let schema = create_test_schema("test_table");

        // Set TTL to 1 second in the past (already expired)
        cache.put_with_ttl(
            "test_key".to_string(),
            schema,
            Some(Duration::seconds(-1)),
            None,
        );

        // Should not retrieve expired entry
        let retrieved = cache.get("test_key", None);
        assert!(retrieved.is_none());

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.invalidations, 1);
    }

    #[test]
    fn test_source_fingerprint_validation() {
        let cache = ProfileCache::new();
        let schema = create_test_schema("test_table");

        cache.put(
            "test_key".to_string(),
            schema.clone(),
            Some("fingerprint_v1".to_string()),
        );

        // Retrieve with matching fingerprint - should succeed
        let retrieved = cache.get("test_key", Some("fingerprint_v1"));
        assert!(retrieved.is_some());

        // Retrieve with different fingerprint - should fail
        let retrieved = cache.get("test_key", Some("fingerprint_v2"));
        assert!(retrieved.is_none());

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.invalidations, 1);
    }

    #[test]
    fn test_lru_eviction() {
        let config = ProfileCacheConfig {
            max_entries: 3,
            enable_lru_eviction: true,
            ..Default::default()
        };
        let cache = ProfileCache::with_config(config);

        // Add 3 entries
        for i in 0..3 {
            let schema = create_test_schema(&format!("table_{}", i));
            cache.put(format!("key_{}", i), schema, None);
        }

        assert_eq!(cache.size(), 3);

        // Access key_1 to make it more recently used
        cache.get("key_1", None);

        // Add a 4th entry, should evict LRU (key_0 or key_2, but not key_1)
        let schema = create_test_schema("table_3");
        cache.put("key_3".to_string(), schema, None);

        assert_eq!(cache.size(), 3);
        assert!(cache.contains("key_1")); // Should still be present
        assert!(cache.contains("key_3")); // Newly added

        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn test_generate_key() {
        let key1 = ProfileCache::generate_key("/tmp/test.csv", None);
        assert_eq!(key1, "/tmp/test.csv");

        let key2 = ProfileCache::generate_key("postgres://localhost/db", Some("users"));
        assert_eq!(key2, "postgres://localhost/db::users");
    }

    #[test]
    fn test_cleanup_expired() {
        let cache = ProfileCache::new();

        // Add one valid entry
        let schema1 = create_test_schema("valid");
        cache.put_with_ttl(
            "valid_key".to_string(),
            schema1,
            Some(Duration::hours(1)),
            None,
        );

        // Add two expired entries
        for i in 0..2 {
            let schema = create_test_schema(&format!("expired_{}", i));
            cache.put_with_ttl(
                format!("expired_key_{}", i),
                schema,
                Some(Duration::seconds(-1)),
                None,
            );
        }

        assert_eq!(cache.size(), 3);

        let cleaned = cache.cleanup_expired();
        assert_eq!(cleaned, 2);
        assert_eq!(cache.size(), 1);
        assert!(cache.contains("valid_key"));
    }

    #[test]
    fn test_hit_rate_calculation() {
        let stats = CacheStats {
            hits: 80,
            misses: 20,
            evictions: 0,
            invalidations: 0,
            total_size_bytes: 0,
        };

        assert_eq!(stats.hit_rate(), 0.8);

        let empty_stats = CacheStats::default();
        assert_eq!(empty_stats.hit_rate(), 0.0);
    }
}
