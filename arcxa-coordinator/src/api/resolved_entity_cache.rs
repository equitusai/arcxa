//! Streaming Resolved Entity Cache
//!
//! High-performance in-memory cache for resolved entities created by the streaming dataflow pipeline.
//!
//! # Architecture
//! - Receives resolved entities from Timely dataflow via channel
//! - Thread-safe concurrent access using DashMap
//! - TTL-based expiration (configurable, default 5 minutes)
//! - Size-based eviction using LRU when limit reached
//! - Metrics tracking: hit rate, miss rate, size, evictions
//!
//! # Usage
//! ```ignore
//! let cache = ResolvedEntityCache::new(config);
//!
//! // From dataflow: insert resolved entity
//! cache.insert(golden_record);
//!
//! // From API: retrieve resolved entity
//! if let Some(record) = cache.get(&entity_id) {
//!     // Cache hit - use cached record
//! } else {
//!     // Cache miss - query RDF store
//! }
//! ```

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use graphica_core::ingestion::resolved_entities::StreamingResolvedEntity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Cached resolved entity with metadata
#[derive(Debug, Clone)]
pub struct CachedResolvedEntity {
    /// Entity ID
    pub entity_id: String,

    /// Resolved fields
    pub fields: HashMap<String, CachedFieldValue>,

    /// Overall confidence
    pub overall_confidence: f64,

    /// Conflict count
    pub conflict_count: usize,

    /// Requires human review
    pub requires_review: bool,

    /// When resolved entity was created in dataflow
    pub created_at: DateTime<Utc>,

    /// When cached (for TTL)
    pub cached_at: DateTime<Utc>,

    /// Number of times accessed from cache
    pub access_count: u64,

    /// Source count (for metrics)
    pub source_count: usize,
}

/// Cached field value (simplified from full FieldValue)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFieldValue {
    pub value: serde_json::Value,
    pub confidence: f64,
    pub resolved_at: DateTime<Utc>,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries
    pub max_size: usize,

    /// Time-to-live for entries (seconds)
    pub ttl_seconds: i64,

    /// Whether to enable automatic cleanup
    pub enable_cleanup: bool,

    /// Cleanup interval (seconds)
    pub cleanup_interval_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000, // 10K entities
            ttl_seconds: 300, // 5 minutes
            enable_cleanup: true,
            cleanup_interval_seconds: 60, // 1 minute
        }
    }
}

/// Cache metrics
#[derive(Debug, Clone)]
pub struct CacheMetrics {
    /// Total cache hits
    pub hits: Arc<AtomicU64>,
    /// Total cache misses
    pub misses: Arc<AtomicU64>,
    /// Total insertions
    pub insertions: Arc<AtomicU64>,
    /// Total evictions (TTL)
    pub ttl_evictions: Arc<AtomicU64>,
    /// Total evictions (size limit)
    pub size_evictions: Arc<AtomicU64>,
    /// Current size
    pub current_size: Arc<AtomicUsize>,
}

impl CacheMetrics {
    fn new() -> Self {
        Self {
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
            insertions: Arc::new(AtomicU64::new(0)),
            ttl_evictions: Arc::new(AtomicU64::new(0)),
            size_evictions: Arc::new(AtomicU64::new(0)),
            current_size: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get hit rate (0.0 - 1.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        let total = hits + self.misses.load(Ordering::Relaxed) as f64;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }

    /// Get current size
    pub fn size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }
}

/// Streaming resolved entity cache
pub struct ResolvedEntityCache {
    /// Cache storage
    cache: Arc<DashMap<String, CachedResolvedEntity>>,

    /// Configuration
    config: CacheConfig,

    /// Metrics
    metrics: CacheMetrics,

    /// Cleanup handle (if enabled)
    _cleanup_handle: Option<std::thread::JoinHandle<()>>,
}

impl ResolvedEntityCache {
    /// Create new cache with default config
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create new cache with custom config
    pub fn with_config(config: CacheConfig) -> Self {
        let cache = Arc::new(DashMap::new());
        let metrics = CacheMetrics::new();

        // Spawn cleanup thread if enabled
        let cleanup_handle = if config.enable_cleanup {
            let cache_clone = cache.clone();
            let ttl_seconds = config.ttl_seconds;
            let interval = std::time::Duration::from_secs(config.cleanup_interval_seconds);
            let ttl_evictions = metrics.ttl_evictions.clone();
            let current_size = metrics.current_size.clone();

            Some(std::thread::spawn(move || {
                loop {
                    std::thread::sleep(interval);

                    let now = Utc::now();
                    let ttl_duration = Duration::seconds(ttl_seconds);

                    // Remove expired entries
                    cache_clone.retain(|_key, record: &mut CachedResolvedEntity| {
                        let age = now - record.cached_at;
                        if age > ttl_duration {
                            ttl_evictions.fetch_add(1, Ordering::Relaxed);
                            current_size.fetch_sub(1, Ordering::Relaxed);
                            false // Remove
                        } else {
                            true // Keep
                        }
                    });

                    tracing::debug!(
                        "Golden record cache cleanup: {} entries, {} TTL evictions",
                        cache_clone.len(),
                        ttl_evictions.load(Ordering::Relaxed)
                    );
                }
            }))
        } else {
            None
        };

        Self {
            cache,
            config,
            metrics,
            _cleanup_handle: cleanup_handle,
        }
    }

    /// Insert resolved entity into cache
    pub fn insert(&self, record: CachedResolvedEntity) {
        let entity_id = record.entity_id.clone();

        // Check size limit
        if self.cache.len() >= self.config.max_size {
            // Evict least recently accessed entry
            self.evict_lru();
        }

        // Insert new entry
        self.cache.insert(entity_id, record);

        self.metrics.insertions.fetch_add(1, Ordering::Relaxed);
        self.metrics.current_size.fetch_add(1, Ordering::Relaxed);

        tracing::trace!("Inserted resolved entity into cache");
    }

    /// Get resolved entity from cache
    pub fn get(&self, entity_id: &str) -> Option<CachedResolvedEntity> {
        if let Some(mut entry) = self.cache.get_mut(entity_id) {
            // Check if expired
            let age = Utc::now() - entry.cached_at;
            if age.num_seconds() > self.config.ttl_seconds {
                self.metrics.ttl_evictions.fetch_add(1, Ordering::Relaxed);
                self.metrics.current_size.fetch_sub(1, Ordering::Relaxed);
                drop(entry); // Release lock
                self.cache.remove(entity_id);
                self.metrics.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // Update access count
            entry.access_count += 1;

            self.metrics.hits.fetch_add(1, Ordering::Relaxed);
            Some(entry.clone())
        } else {
            self.metrics.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Remove resolved entity from cache
    pub fn remove(&self, entity_id: &str) -> Option<CachedResolvedEntity> {
        if let Some((_, record)) = self.cache.remove(entity_id) {
            self.metrics.current_size.fetch_sub(1, Ordering::Relaxed);
            Some(record)
        } else {
            None
        }
    }

    /// Clear all entries
    pub fn clear(&self) {
        let size = self.cache.len();
        self.cache.clear();
        self.metrics.current_size.store(0, Ordering::Relaxed);
        tracing::info!("Cleared resolved entity cache ({} entries)", size);
    }

    /// Get cache metrics
    pub fn metrics(&self) -> &CacheMetrics {
        &self.metrics
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Evict least recently used entry
    fn evict_lru(&self) {
        let mut lru_key: Option<String> = None;
        let mut lru_access_count = u64::MAX;
        let mut lru_cached_at = Utc::now();

        // Find LRU entry (least accessed, oldest)
        for entry in self.cache.iter() {
            let access_count = entry.value().access_count;
            let cached_at = entry.value().cached_at;

            if access_count < lru_access_count
                || (access_count == lru_access_count && cached_at < lru_cached_at)
            {
                lru_key = Some(entry.key().clone());
                lru_access_count = access_count;
                lru_cached_at = cached_at;
            }
        }

        // Evict LRU entry
        if let Some(key) = lru_key {
            self.cache.remove(&key);
            self.metrics.size_evictions.fetch_add(1, Ordering::Relaxed);
            self.metrics.current_size.fetch_sub(1, Ordering::Relaxed);
            tracing::debug!("Evicted LRU entry from resolved entity cache: {}", key);
        }
    }
}

impl Default for ResolvedEntityCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement the ResolvedEntityCache trait from graphica-core for dataflow integration
impl graphica_core::ingestion::resolved_entities::ResolvedEntityCache for ResolvedEntityCache {
    fn insert(&self, record: StreamingResolvedEntity) -> Result<(), String> {
        let cached_record = CachedResolvedEntity::from(record);
        self.insert(cached_record);
        Ok(())
    }
}

// Conversion from streaming resolved entity to cached version
impl From<StreamingResolvedEntity> for CachedResolvedEntity {
    fn from(
        streaming: graphica_core::ingestion::resolved_entities::StreamingResolvedEntity,
    ) -> Self {
        let mut cached_fields = HashMap::new();

        for (field_name, field_value) in streaming.fields {
            cached_fields.insert(
                field_name,
                CachedFieldValue {
                    value: field_value.value.clone(),
                    confidence: field_value.confidence,
                    resolved_at: field_value.resolved_at,
                },
            );
        }

        Self {
            entity_id: streaming.entity_id,
            fields: cached_fields,
            overall_confidence: streaming.overall_confidence,
            conflict_count: streaming.conflict_count,
            requires_review: streaming.requires_review,
            created_at: streaming.updated_at,
            cached_at: Utc::now(),
            access_count: 0,
            source_count: streaming.source_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_get() {
        let cache = ResolvedEntityCache::new();

        let mut fields = HashMap::new();
        fields.insert(
            "email".to_string(),
            CachedFieldValue {
                value: serde_json::json!("test@example.com"),
                confidence: 0.95,
                resolved_at: Utc::now(),
            },
        );

        let record = CachedResolvedEntity {
            entity_id: "test_entity".to_string(),
            fields,
            overall_confidence: 0.95,
            conflict_count: 0,
            requires_review: false,
            created_at: Utc::now(),
            cached_at: Utc::now(),
            access_count: 0,
            source_count: 2,
        };

        cache.insert(record.clone());

        let retrieved = cache
            .get("test_entity")
            .expect("Should retrieve from cache");
        assert_eq!(retrieved.entity_id, "test_entity");
        assert_eq!(retrieved.overall_confidence, 0.95);

        // Verify metrics
        assert_eq!(cache.metrics().hits.load(Ordering::Relaxed), 1);
        assert_eq!(cache.metrics().misses.load(Ordering::Relaxed), 0);
        assert_eq!(cache.metrics().insertions.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_cache_miss() {
        let cache = ResolvedEntityCache::new();

        let result = cache.get("nonexistent");
        assert!(result.is_none());

        assert_eq!(cache.metrics().misses.load(Ordering::Relaxed), 1);
        assert_eq!(cache.metrics().hits.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_cache_remove() {
        let cache = ResolvedEntityCache::new();

        let record = CachedResolvedEntity {
            entity_id: "test".to_string(),
            fields: HashMap::new(),
            overall_confidence: 0.9,
            conflict_count: 0,
            requires_review: false,
            created_at: Utc::now(),
            cached_at: Utc::now(),
            access_count: 0,
            source_count: 1,
        };

        cache.insert(record);
        assert_eq!(cache.size(), 1);

        cache.remove("test");
        assert_eq!(cache.size(), 0);

        let result = cache.get("test");
        assert!(result.is_none());
    }

    #[test]
    fn test_size_based_eviction() {
        let config = CacheConfig {
            max_size: 2,
            ttl_seconds: 300,
            enable_cleanup: false,
            cleanup_interval_seconds: 60,
        };

        let cache = ResolvedEntityCache::with_config(config);

        // Insert 3 records (max is 2)
        for i in 0..3 {
            let record = CachedResolvedEntity {
                entity_id: format!("entity_{}", i),
                fields: HashMap::new(),
                overall_confidence: 0.9,
                conflict_count: 0,
                requires_review: false,
                created_at: Utc::now(),
                cached_at: Utc::now(),
                access_count: 0,
                source_count: 1,
            };
            cache.insert(record);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Should have evicted oldest
        assert_eq!(cache.size(), 2);
        assert_eq!(cache.metrics().size_evictions.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_hit_rate_calculation() {
        let cache = ResolvedEntityCache::new();

        let record = CachedResolvedEntity {
            entity_id: "test".to_string(),
            fields: HashMap::new(),
            overall_confidence: 0.9,
            conflict_count: 0,
            requires_review: false,
            created_at: Utc::now(),
            cached_at: Utc::now(),
            access_count: 0,
            source_count: 1,
        };

        cache.insert(record);

        // 2 hits, 1 miss
        cache.get("test");
        cache.get("test");
        cache.get("nonexistent");

        let hit_rate = cache.metrics().hit_rate();
        assert!((hit_rate - 0.666).abs() < 0.01); // ~66.6%
    }
}
