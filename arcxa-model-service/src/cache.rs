//! # In-Memory Embedding Cache
//!
//! Fast LRU cache for frequently requested embeddings.
//! Complements coordinator's RocksDB cache for multi-layer caching strategy.

use dashmap::DashMap;
use ndarray::Array1;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::debug;

/// In-memory LRU cache for embeddings
pub struct EmbeddingCache {
    /// Cache storage (concurrent HashMap)
    cache: Arc<DashMap<String, Array1<f32>>>,

    /// LRU order tracking
    lru: Arc<RwLock<VecDeque<String>>>,

    /// Maximum cache size
    max_size: usize,

    /// Cache statistics
    hits: Arc<parking_lot::Mutex<u64>>,
    misses: Arc<parking_lot::Mutex<u64>>,
}

impl EmbeddingCache {
    /// Create a new cache with specified max size
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            lru: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            hits: Arc::new(parking_lot::Mutex::new(0)),
            misses: Arc::new(parking_lot::Mutex::new(0)),
        }
    }

    /// Get embedding from cache
    pub fn get(&self, text: &str) -> Option<Array1<f32>> {
        let key = self.normalize_key(text);

        if let Some(entry) = self.cache.get(&key) {
            // Cache hit - update LRU
            self.update_lru(&key);

            *self.hits.lock() += 1;
            debug!("Cache HIT: {}", text);

            Some(entry.value().clone())
        } else {
            // Cache miss
            *self.misses.lock() += 1;
            debug!("Cache MISS: {}", text);

            None
        }
    }

    /// Put embedding into cache
    pub fn put(&self, text: &str, embedding: &Array1<f32>) {
        let key = self.normalize_key(text);

        // Check if we need to evict
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&key) {
            self.evict_lru();
        }

        // Insert into cache
        self.cache.insert(key.clone(), embedding.clone());

        // Update LRU
        self.update_lru(&key);
    }

    /// Get or compute embedding
    pub fn get_or_compute<F>(&self, text: &str, compute_fn: F) -> anyhow::Result<Array1<f32>>
    where
        F: FnOnce() -> anyhow::Result<Array1<f32>>,
    {
        if let Some(embedding) = self.get(text) {
            return Ok(embedding);
        }

        let embedding = compute_fn()?;
        self.put(text, &embedding);
        Ok(embedding)
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
        self.lru.write().clear();
        *self.hits.lock() = 0;
        *self.misses.lock() = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let hits = *self.hits.lock();
        let misses = *self.misses.lock();
        let total = hits + misses;

        CacheStats {
            size: self.cache.len(),
            max_size: self.max_size,
            hits,
            misses,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
        }
    }

    /// Normalize cache key (lowercase, trim)
    fn normalize_key(&self, text: &str) -> String {
        text.trim().to_lowercase()
    }

    /// Update LRU order
    fn update_lru(&self, key: &str) {
        let mut lru = self.lru.write();

        // Remove if already present
        if let Some(pos) = lru.iter().position(|k| k == key) {
            lru.remove(pos);
        }

        // Add to front (most recently used)
        lru.push_front(key.to_string());

        // Trim if too long
        while lru.len() > self.max_size {
            lru.pop_back();
        }
    }

    /// Evict least recently used entry
    fn evict_lru(&self) {
        let mut lru = self.lru.write();

        if let Some(key) = lru.pop_back() {
            self.cache.remove(&key);
            debug!("Evicted from cache: {}", key);
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = EmbeddingCache::new(3);

        let emb1 = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let emb2 = Array1::from_vec(vec![4.0, 5.0, 6.0]);

        // Put and get
        cache.put("text1", &emb1);
        cache.put("text2", &emb2);

        assert!(cache.get("text1").is_some());
        assert!(cache.get("text2").is_some());
        assert!(cache.get("text3").is_none());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = EmbeddingCache::new(2);

        let emb1 = Array1::from_vec(vec![1.0]);
        let emb2 = Array1::from_vec(vec![2.0]);
        let emb3 = Array1::from_vec(vec![3.0]);

        cache.put("text1", &emb1);
        cache.put("text2", &emb2);
        cache.put("text3", &emb3); // Should evict text1

        assert!(cache.get("text1").is_none()); // Evicted
        assert!(cache.get("text2").is_some());
        assert!(cache.get("text3").is_some());
    }

    #[test]
    fn test_cache_stats() {
        let cache = EmbeddingCache::new(10);

        let emb = Array1::from_vec(vec![1.0, 2.0]);

        cache.put("text1", &emb);

        cache.get("text1"); // Hit
        cache.get("text2"); // Miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[test]
    fn test_get_or_compute() {
        let cache = EmbeddingCache::new(10);

        let emb = cache
            .get_or_compute("text1", || Ok(Array1::from_vec(vec![1.0, 2.0, 3.0])))
            .unwrap();

        assert_eq!(emb.len(), 3);

        // Second call should hit cache
        let emb2 = cache
            .get_or_compute("text1", || {
                panic!("Should not be called!");
            })
            .unwrap();

        assert_eq!(emb2.len(), 3);
    }
}
