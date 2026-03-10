//! # Embedding Service
//!
//! Provides semantic embeddings for field names using pre-trained transformer models.
//!
//! ## Model
//!
//! Uses `sentence-transformers/all-MiniLM-L6-v2`:
//! - Dimensions: 384
//! - Model size: ~23MB (ONNX)
//! - Performance: ~5ms per embedding
//! - Domain: General-purpose sentence embeddings
//!
//! ## Caching Strategy
//!
//! - LRU cache with 10,000 entry limit
//! - TTL: No expiration (field names don't change semantics)
//! - Hit rate target: >80% for repeated schemas
//! - Memory overhead: ~15MB for full cache

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, InitOptions};
use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Embedding vector type alias
pub type Embedding = Vec<f32>;

/// Errors that can occur during embedding operations
#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("Model initialization failed: {0}")]
    ModelInit(String),

    #[error("Embedding generation failed: {0}")]
    GenerationFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Embedding service for semantic field name similarity
///
/// This service uses a pre-trained transformer model (all-MiniLM-L6-v2) to generate
/// semantic embeddings for field names. Embeddings are cached in an LRU cache for performance.
///
/// ## Thread Safety
///
/// This service is thread-safe and can be shared across threads using `Arc`.
///
/// ## Performance
///
/// - First embedding (cold start): ~50ms (model load) + 5ms (inference)
/// - Subsequent embeddings (warm cache): <1ms (cache hit) or ~5ms (cache miss)
/// - Cosine similarity: <1ms
pub struct EmbeddingService {
    /// Pre-trained embedding model
    model: Arc<TextEmbedding>,

    /// LRU cache for field name embeddings
    /// Key: normalized field name, Value: 384-dim embedding vector
    cache: Arc<Mutex<LruCache<String, Embedding>>>,

    /// Cache statistics (for monitoring)
    cache_hits: Arc<Mutex<u64>>,
    cache_misses: Arc<Mutex<u64>>,
}

impl EmbeddingService {
    /// Default cache size (10,000 field names)
    const DEFAULT_CACHE_SIZE: usize = 10_000;

    /// Create a new embedding service with default configuration
    ///
    /// Downloads the model on first run (~23MB).
    /// Subsequent runs load from cache.
    ///
    /// ## Errors
    ///
    /// Returns error if model initialization fails (network issue, disk space, etc.)
    pub fn new() -> Result<Self, EmbeddingError> {
        Self::with_cache_size(Self::DEFAULT_CACHE_SIZE)
    }

    /// Create embedding service with custom cache size
    pub fn with_cache_size(cache_size: usize) -> Result<Self, EmbeddingError> {
        info!("Initializing embedding service (model: all-MiniLM-L6-v2)");

        // Initialize fastembed model
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true)
        )
        .map_err(|e| EmbeddingError::ModelInit(e.to_string()))?;

        let cache_size_nz = NonZeroUsize::new(cache_size)
            .ok_or_else(|| EmbeddingError::InvalidInput("Cache size must be > 0".to_string()))?;

        info!(
            "Embedding service initialized (cache size: {})",
            cache_size
        );

        Ok(Self {
            model: Arc::new(model),
            cache: Arc::new(Mutex::new(LruCache::new(cache_size_nz))),
            cache_hits: Arc::new(Mutex::new(0)),
            cache_misses: Arc::new(Mutex::new(0)),
        })
    }

    /// Generate semantic embedding for a field name
    ///
    /// Field names are normalized (lowercase, trimmed) before embedding.
    /// Results are cached for subsequent calls with the same field name.
    ///
    /// ## Arguments
    ///
    /// * `field_name` - The field name to embed (e.g., "customer_id", "cust_email")
    ///
    /// ## Returns
    ///
    /// 384-dimensional embedding vector
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use graphica_core::ml::EmbeddingService;
    /// # fn example() -> anyhow::Result<()> {
    /// let service = EmbeddingService::new()?;
    ///
    /// let embedding = service.embed_field_name("customer_email")?;
    /// assert_eq!(embedding.len(), 384);
    /// # Ok(())
    /// # }
    /// ```
    pub fn embed_field_name(&self, field_name: &str) -> Result<Embedding, EmbeddingError> {
        // Normalize field name (lowercase, trimmed)
        let normalized = self.normalize_field_name(field_name);

        // Check cache first
        if let Some(cached_embedding) = self.cache.lock().get(&normalized).cloned() {
            *self.cache_hits.lock() += 1;
            debug!("Cache hit for field: {}", normalized);
            return Ok(cached_embedding);
        }

        // Cache miss - generate embedding
        *self.cache_misses.lock() += 1;
        debug!("Cache miss for field: {}", normalized);

        let embedding = self.generate_embedding(&normalized)?;

        // Store in cache
        self.cache.lock().put(normalized, embedding.clone());

        Ok(embedding)
    }

    /// Generate embeddings for multiple field names in batch
    ///
    /// More efficient than calling `embed_field_name` multiple times
    /// due to batched inference.
    ///
    /// ## Arguments
    ///
    /// * `field_names` - List of field names to embed
    ///
    /// ## Returns
    ///
    /// Vector of embeddings in the same order as input
    pub fn embed_batch(&self, field_names: &[&str]) -> Result<Vec<Embedding>, EmbeddingError> {
        let normalized: Vec<String> = field_names
            .iter()
            .map(|name| self.normalize_field_name(name))
            .collect();

        // Check cache for each field
        let mut results = Vec::with_capacity(normalized.len());
        let mut uncached_indices = Vec::new();
        let mut uncached_names = Vec::new();

        for (idx, norm_name) in normalized.iter().enumerate() {
            if let Some(cached) = self.cache.lock().get(norm_name).cloned() {
                *self.cache_hits.lock() += 1;
                results.push(Some(cached));
            } else {
                *self.cache_misses.lock() += 1;
                results.push(None);
                uncached_indices.push(idx);
                uncached_names.push(norm_name.as_str());
            }
        }

        // Generate embeddings for uncached fields
        if !uncached_names.is_empty() {
            let batch_embeddings = self.generate_embeddings_batch(&uncached_names)?;

            // Fill in results and update cache
            for (uncached_idx, embedding) in uncached_indices.iter().zip(batch_embeddings) {
                let norm_name = &normalized[*uncached_idx];
                self.cache.lock().put(norm_name.clone(), embedding.clone());
                results[*uncached_idx] = Some(embedding);
            }
        }

        // Unwrap all results (should all be Some at this point)
        Ok(results.into_iter().map(|r| r.unwrap()).collect())
    }

    /// Calculate cosine similarity between two embeddings
    ///
    /// ## Arguments
    ///
    /// * `a` - First embedding vector (384-dim)
    /// * `b` - Second embedding vector (384-dim)
    ///
    /// ## Returns
    ///
    /// Cosine similarity score between -1.0 and 1.0
    /// - 1.0: Identical vectors
    /// - 0.0: Orthogonal vectors
    /// - -1.0: Opposite vectors
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use graphica_core::ml::EmbeddingService;
    /// # fn example() -> anyhow::Result<()> {
    /// let service = EmbeddingService::new()?;
    ///
    /// let emb1 = service.embed_field_name("customer_id")?;
    /// let emb2 = service.embed_field_name("cust_id")?;
    ///
    /// let similarity = service.cosine_similarity(&emb1, &emb2);
    /// assert!(similarity > 0.7); // High semantic similarity
    /// # Ok(())
    /// # }
    /// ```
    pub fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() {
            warn!(
                "Embedding dimension mismatch: {} vs {}",
                a.len(),
                b.len()
            );
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            warn!("Zero-norm embedding detected");
            return 0.0;
        }

        (dot_product / (norm_a * norm_b)) as f64
    }

    /// Get cache statistics
    ///
    /// Returns (hits, misses, hit_rate)
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        let hits = *self.cache_hits.lock();
        let misses = *self.cache_misses.lock();
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        (hits, misses, hit_rate)
    }

    /// Clear the embedding cache
    pub fn clear_cache(&self) {
        self.cache.lock().clear();
        *self.cache_hits.lock() = 0;
        *self.cache_misses.lock() = 0;
        info!("Embedding cache cleared");
    }

    // ========================================================================
    // Private Methods
    // ========================================================================

    /// Normalize field name for caching consistency
    fn normalize_field_name(&self, name: &str) -> String {
        name.to_lowercase().trim().to_string()
    }

    /// Generate embedding for a single field name (bypasses cache)
    fn generate_embedding(&self, normalized_name: &str) -> Result<Embedding, EmbeddingError> {
        let embeddings = self
            .model
            .embed(vec![normalized_name.to_string()], None)
            .map_err(|e| EmbeddingError::GenerationFailed(e.to_string()))?;

        if embeddings.is_empty() {
            return Err(EmbeddingError::GenerationFailed(
                "No embedding generated".to_string(),
            ));
        }

        Ok(embeddings[0].clone())
    }

    /// Generate embeddings for multiple field names (bypasses cache)
    fn generate_embeddings_batch(
        &self,
        normalized_names: &[&str],
    ) -> Result<Vec<Embedding>, EmbeddingError> {
        let names: Vec<String> = normalized_names.iter().map(|s| s.to_string()).collect();

        let embeddings = self
            .model
            .embed(names, None)
            .map_err(|e| EmbeddingError::GenerationFailed(e.to_string()))?;

        Ok(embeddings)
    }
}

// Thread-safe Clone implementation
impl Clone for EmbeddingService {
    fn clone(&self) -> Self {
        Self {
            model: Arc::clone(&self.model),
            cache: Arc::clone(&self.cache),
            cache_hits: Arc::clone(&self.cache_hits),
            cache_misses: Arc::clone(&self.cache_misses),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create service for tests
    fn create_test_service() -> Result<EmbeddingService, EmbeddingError> {
        EmbeddingService::with_cache_size(100)
    }

    #[test]
    #[ignore] // Requires model download
    fn test_embedding_service_creation() {
        let service = create_test_service();
        assert!(service.is_ok(), "Failed to create embedding service");
    }

    #[test]
    #[ignore] // Requires model download
    fn test_embed_field_name() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        let embedding = service.embed_field_name("customer_id")?;

        assert_eq!(
            embedding.len(),
            384,
            "Embedding should be 384-dimensional"
        );
        assert!(
            embedding.iter().any(|&x| x != 0.0),
            "Embedding should not be all zeros"
        );

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_cache_hit() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        // First call - cache miss
        let emb1 = service.embed_field_name("customer_email")?;
        let (hits1, misses1, _) = service.cache_stats();
        assert_eq!(hits1, 0, "Should have 0 cache hits after first call");
        assert_eq!(misses1, 1, "Should have 1 cache miss after first call");

        // Second call - cache hit
        let emb2 = service.embed_field_name("customer_email")?;
        let (hits2, misses2, hit_rate) = service.cache_stats();
        assert_eq!(hits2, 1, "Should have 1 cache hit after second call");
        assert_eq!(misses2, 1, "Should still have 1 cache miss");
        assert_eq!(hit_rate, 0.5, "Hit rate should be 50%");

        // Embeddings should be identical
        assert_eq!(emb1, emb2, "Cached embedding should match original");

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_cosine_similarity_identical() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        let emb = service.embed_field_name("order_id")?;
        let similarity = service.cosine_similarity(&emb, &emb);

        assert!(
            (similarity - 1.0).abs() < 0.001,
            "Identical embeddings should have similarity ~1.0"
        );

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_cosine_similarity_similar_fields() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        let emb1 = service.embed_field_name("customer_id")?;
        let emb2 = service.embed_field_name("cust_id")?;

        let similarity = service.cosine_similarity(&emb1, &emb2);

        assert!(
            similarity > 0.7,
            "Similar field names should have high similarity (got {:.3})",
            similarity
        );

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_cosine_similarity_different_fields() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        let emb1 = service.embed_field_name("customer_id")?;
        let emb2 = service.embed_field_name("product_name")?;

        let similarity = service.cosine_similarity(&emb1, &emb2);

        assert!(
            similarity < 0.6,
            "Different field names should have low similarity (got {:.3})",
            similarity
        );

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_batch_embedding() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        let field_names = vec!["customer_id", "cust_email", "order_date"];
        let embeddings = service.embed_batch(&field_names)?;

        assert_eq!(
            embeddings.len(),
            3,
            "Should return 3 embeddings for 3 inputs"
        );

        for (i, emb) in embeddings.iter().enumerate() {
            assert_eq!(emb.len(), 384, "Embedding {} should be 384-dim", i);
        }

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_normalization() -> Result<(), EmbeddingError> {
        let service = create_test_service()?;

        // These should all produce the same embedding (after normalization)
        let emb1 = service.embed_field_name("Customer_ID")?;
        let emb2 = service.embed_field_name("customer_id")?;
        let emb3 = service.embed_field_name("  CUSTOMER_ID  ")?;

        assert_eq!(emb1, emb2, "Case should be normalized");
        assert_eq!(emb2, emb3, "Whitespace should be trimmed");

        Ok(())
    }

    #[test]
    #[ignore] // Requires model download
    fn test_cache_eviction() -> Result<(), EmbeddingError> {
        // Create service with small cache (3 items)
        let service = EmbeddingService::with_cache_size(3)?;

        // Fill cache
        service.embed_field_name("field1")?;
        service.embed_field_name("field2")?;
        service.embed_field_name("field3")?;

        // This should evict "field1"
        service.embed_field_name("field4")?;

        // Access field1 again - should be cache miss
        service.clear_cache(); // Reset stats
        service.embed_field_name("field4")?; // Cache hit
        service.embed_field_name("field1")?; // Cache miss (evicted)

        let (hits, misses, _) = service.cache_stats();
        assert_eq!(hits, 1, "field4 should be cache hit");
        assert_eq!(misses, 1, "field1 should be cache miss (evicted)");

        Ok(())
    }
}
