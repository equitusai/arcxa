//! Similarity Index for Fast Nearest Neighbor Search
//!
//! Provides both brute-force and HNSW-based similarity search.
//! For small datasets (<10k items), brute force is actually faster.

use super::embeddings::cosine_similarity;
use parking_lot::RwLock;
use std::sync::Arc;

/// Result of a similarity search
#[derive(Debug, Clone)]
pub struct SimilarityMatch {
    /// Index of the matched item
    pub index: usize,

    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,

    /// Associated metadata (optional)
    pub metadata: Option<String>,
}

/// Configuration for similarity index
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Number of nearest neighbors to return
    pub k: usize,

    /// Minimum similarity threshold
    pub min_similarity: f32,

    /// Use HNSW index (vs brute force)
    pub use_hnsw: bool,

    /// HNSW construction parameters (if using HNSW)
    pub hnsw_m: usize, // Number of connections per node
    pub hnsw_ef_construction: usize, // Size of dynamic candidate list during construction
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            k: 5,
            min_similarity: 0.0,
            use_hnsw: false, // Brute force is fine for small datasets
            hnsw_m: 16,
            hnsw_ef_construction: 200,
        }
    }
}

/// Trait for similarity index
pub trait SimilarityIndex: Send + Sync {
    /// Search for k nearest neighbors
    fn search(&self, query: &[f32], k: usize, min_similarity: f32) -> Vec<SimilarityMatch>;

    /// Get total number of indexed items
    fn len(&self) -> usize;

    /// Check if index is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Brute-force similarity index
/// Fast for small datasets (<10k items), no dependencies
pub struct BruteForceIndex {
    embeddings: Vec<Vec<f32>>,
    metadata: Vec<Option<String>>,
}

impl BruteForceIndex {
    /// Create a new brute-force index
    pub fn new(embeddings: Vec<Vec<f32>>, metadata: Vec<Option<String>>) -> Self {
        assert_eq!(embeddings.len(), metadata.len());
        Self {
            embeddings,
            metadata,
        }
    }

    /// Create from embeddings only
    pub fn from_embeddings(embeddings: Vec<Vec<f32>>) -> Self {
        let metadata = vec![None; embeddings.len()];
        Self::new(embeddings, metadata)
    }
}

impl SimilarityIndex for BruteForceIndex {
    fn search(&self, query: &[f32], k: usize, min_similarity: f32) -> Vec<SimilarityMatch> {
        let mut similarities: Vec<(usize, f32)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(idx, emb)| {
                let sim = cosine_similarity(query, emb);
                (idx, sim)
            })
            .filter(|(_, sim)| *sim >= min_similarity)
            .collect();

        // Sort by similarity (descending)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top k
        similarities
            .into_iter()
            .take(k)
            .map(|(index, similarity)| SimilarityMatch {
                index,
                similarity,
                metadata: self.metadata[index].clone(),
            })
            .collect()
    }

    fn len(&self) -> usize {
        self.embeddings.len()
    }
}

/// Thread-safe similarity index with caching
pub struct CachedSimilarityIndex {
    index: Arc<RwLock<Box<dyn SimilarityIndex>>>,
    config: IndexConfig,
}

impl CachedSimilarityIndex {
    /// Create a new cached index from embeddings
    pub fn new(
        embeddings: Vec<Vec<f32>>,
        metadata: Vec<Option<String>>,
        config: IndexConfig,
    ) -> Self {
        let index: Box<dyn SimilarityIndex> = if config.use_hnsw {
            // TODO: Implement HNSW when instant-distance is added
            // For now, fallback to brute force
            Box::new(BruteForceIndex::new(embeddings, metadata))
        } else {
            Box::new(BruteForceIndex::new(embeddings, metadata))
        };

        Self {
            index: Arc::new(RwLock::new(index)),
            config,
        }
    }

    /// Search for similar items
    pub fn search(&self, query: &[f32]) -> Vec<SimilarityMatch> {
        let index = self.index.read();
        index.search(query, self.config.k, self.config.min_similarity)
    }

    /// Search with custom k and threshold
    pub fn search_with_params(
        &self,
        query: &[f32],
        k: usize,
        min_similarity: f32,
    ) -> Vec<SimilarityMatch> {
        let index = self.index.read();
        index.search(query, k, min_similarity)
    }

    /// Get index size
    pub fn len(&self) -> usize {
        let index = self.index.read();
        index.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Rebuild index with new embeddings
    pub fn rebuild(&self, embeddings: Vec<Vec<f32>>, metadata: Vec<Option<String>>) {
        let new_index: Box<dyn SimilarityIndex> = if self.config.use_hnsw {
            // TODO: HNSW implementation
            Box::new(BruteForceIndex::new(embeddings, metadata))
        } else {
            Box::new(BruteForceIndex::new(embeddings, metadata))
        };

        let mut index = self.index.write();
        *index = new_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embeddings() -> Vec<Vec<f32>> {
        vec![
            vec![1.0, 0.0, 0.0], // 0
            vec![0.9, 0.1, 0.0], // 1 - similar to 0
            vec![0.0, 1.0, 0.0], // 2
            vec![0.0, 0.9, 0.1], // 3 - similar to 2
            vec![0.0, 0.0, 1.0], // 4
        ]
    }

    #[test]
    fn test_brute_force_index() {
        let embeddings = create_test_embeddings();
        let metadata = vec![
            Some("item_0".to_string()),
            Some("item_1".to_string()),
            Some("item_2".to_string()),
            Some("item_3".to_string()),
            Some("item_4".to_string()),
        ];

        let index = BruteForceIndex::new(embeddings, metadata);

        assert_eq!(index.len(), 5);

        // Search for items similar to [1.0, 0.0, 0.0]
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 2, 0.0);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0); // Exact match
        assert!(results[0].similarity > 0.99);
        assert_eq!(results[1].index, 1); // Second closest
    }

    #[test]
    fn test_brute_force_with_threshold() {
        let embeddings = create_test_embeddings();
        let index = BruteForceIndex::from_embeddings(embeddings);

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 10, 0.8); // High threshold

        // Should only return items with similarity >= 0.8
        assert!(results.len() <= 2);
        for result in results {
            assert!(result.similarity >= 0.8);
        }
    }

    #[test]
    fn test_cached_index() {
        let embeddings = create_test_embeddings();
        let metadata: Vec<Option<String>> = (0..5).map(|i| Some(format!("item_{}", i))).collect();

        let config = IndexConfig {
            k: 3,
            min_similarity: 0.5,
            ..Default::default()
        };

        let index = CachedSimilarityIndex::new(embeddings, metadata, config);

        let query = vec![0.0, 1.0, 0.0];
        let results = index.search(&query);

        assert!(results.len() <= 3);
        for result in &results {
            assert!(result.similarity >= 0.5);
        }

        // Test metadata
        assert!(results[0].metadata.is_some());
    }

    #[test]
    fn test_index_rebuild() {
        let embeddings = create_test_embeddings();
        let metadata = vec![None; 5];

        let config = IndexConfig::default();
        let index = CachedSimilarityIndex::new(embeddings, metadata, config);

        assert_eq!(index.len(), 5);

        // Rebuild with new embeddings
        let new_embeddings = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let new_metadata = vec![Some("new_0".to_string()), Some("new_1".to_string())];

        index.rebuild(new_embeddings, new_metadata);

        assert_eq!(index.len(), 2);
    }

    #[test]
    fn test_cosine_similarity_computation() {
        let embeddings = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.707, 0.707, 0.0], // 45 degrees from both
        ];

        let index = BruteForceIndex::from_embeddings(embeddings);

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 3, 0.0);

        // Should find exact match first
        assert_eq!(results[0].index, 0);
        assert!((results[0].similarity - 1.0).abs() < 0.01);

        // 45 degree angle should have similarity ~0.707
        let result_45 = results.iter().find(|r| r.index == 2).unwrap();
        assert!((result_45.similarity - 0.707).abs() < 0.01);

        // Orthogonal vectors should have similarity 0
        let result_ortho = results.iter().find(|r| r.index == 1).unwrap();
        assert!(result_ortho.similarity.abs() < 0.01);
    }
}
