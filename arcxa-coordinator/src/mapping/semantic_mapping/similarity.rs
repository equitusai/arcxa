//! # Vector Similarity Module
//!
//! Computes similarity between embedding vectors using cosine distance.
//!
//! ## Algorithm
//!
//! For normalized vectors (unit length):
//! ```text
//! cosine_similarity(A, B) = A · B = Σ(A[i] * B[i])
//! ```
//!
//! For non-normalized vectors:
//! ```text
//! cosine_similarity(A, B) = (A · B) / (||A|| * ||B||)
//! ```

use ndarray::{Array1, ArrayView1};

/// Vector similarity computer
pub struct CosineSimilarity;

impl CosineSimilarity {
    /// Compute cosine similarity between two vectors
    ///
    /// Returns a score from -1.0 (opposite) to 1.0 (identical)
    /// For semantic similarity, typical range is 0.3 to 0.95
    pub fn similarity(vec1: &Array1<f32>, vec2: &Array1<f32>) -> f64 {
        assert_eq!(
            vec1.len(),
            vec2.len(),
            "Vectors must have the same dimensionality"
        );

        let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();

        let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm1 == 0.0 || norm2 == 0.0 {
            return 0.0;
        }

        (dot_product / (norm1 * norm2)) as f64
    }

    /// Compute cosine similarity for normalized (unit) vectors
    ///
    /// Faster version when vectors are already L2-normalized
    pub fn similarity_normalized(vec1: &Array1<f32>, vec2: &Array1<f32>) -> f64 {
        assert_eq!(
            vec1.len(),
            vec2.len(),
            "Vectors must have the same dimensionality"
        );

        let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();

        dot_product as f64
    }

    /// Normalize a vector to unit length (L2 normalization)
    pub fn normalize(vec: &mut Array1<f32>) {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm > 0.0 {
            vec.mapv_inplace(|x| x / norm);
        }
    }

    /// Compute pairwise similarities between a query and multiple candidates
    ///
    /// Returns a vector of (index, similarity) pairs sorted by similarity (descending)
    pub fn batch_similarity(
        query: &Array1<f32>,
        candidates: &[Array1<f32>],
        top_k: usize,
    ) -> Vec<(usize, f64)> {
        let mut scores: Vec<(usize, f64)> = candidates
            .iter()
            .enumerate()
            .map(|(idx, candidate)| (idx, Self::similarity(query, candidate)))
            .collect();

        // Sort by similarity (descending)
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top_k results
        scores.into_iter().take(top_k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::arr1;

    #[test]
    fn test_identical_vectors() {
        let vec1 = arr1(&[1.0, 2.0, 3.0]);
        let vec2 = arr1(&[1.0, 2.0, 3.0]);

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert!((similarity - 1.0).abs() < 0.001, "Identical vectors should have similarity ~1.0");
    }

    #[test]
    fn test_orthogonal_vectors() {
        let vec1 = arr1(&[1.0, 0.0, 0.0]);
        let vec2 = arr1(&[0.0, 1.0, 0.0]);

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert!((similarity - 0.0).abs() < 0.001, "Orthogonal vectors should have similarity ~0.0");
    }

    #[test]
    fn test_opposite_vectors() {
        let vec1 = arr1(&[1.0, 2.0, 3.0]);
        let vec2 = arr1(&[-1.0, -2.0, -3.0]);

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert!((similarity + 1.0).abs() < 0.001, "Opposite vectors should have similarity ~-1.0");
    }

    #[test]
    fn test_similar_vectors() {
        let vec1 = arr1(&[1.0, 2.0, 3.0]);
        let vec2 = arr1(&[1.1, 2.1, 2.9]);

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert!(similarity > 0.99, "Very similar vectors should have high similarity");
    }

    #[test]
    fn test_normalize() {
        let mut vec = arr1(&[3.0, 4.0]);  // Length = 5.0

        CosineSimilarity::normalize(&mut vec);

        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001, "Normalized vector should have unit length");

        // Check values
        assert!((vec[0] - 0.6).abs() < 0.001);  // 3/5
        assert!((vec[1] - 0.8).abs() < 0.001);  // 4/5
    }

    #[test]
    fn test_similarity_normalized() {
        let mut vec1 = arr1(&[1.0, 2.0, 3.0]);
        let mut vec2 = arr1(&[1.0, 2.0, 3.0]);

        CosineSimilarity::normalize(&mut vec1);
        CosineSimilarity::normalize(&mut vec2);

        let similarity = CosineSimilarity::similarity_normalized(&vec1, &vec2);

        assert!((similarity - 1.0).abs() < 0.001, "Normalized identical vectors should have similarity ~1.0");
    }

    #[test]
    fn test_batch_similarity() {
        let query = arr1(&[1.0, 0.0, 0.0]);

        let candidates = vec![
            arr1(&[1.0, 0.0, 0.0]),  // Perfect match
            arr1(&[0.9, 0.1, 0.0]),  // Close match
            arr1(&[0.0, 1.0, 0.0]),  // Orthogonal
            arr1(&[-1.0, 0.0, 0.0]), // Opposite
        ];

        let results = CosineSimilarity::batch_similarity(&query, &candidates, 3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 0);  // Index of perfect match
        assert!(results[0].1 > 0.99);  // High similarity

        assert_eq!(results[1].0, 1);  // Index of close match
        assert!(results[1].1 > 0.9);

        assert_eq!(results[2].0, 2);  // Index of orthogonal
        assert!(results[2].1 < 0.1);
    }

    #[test]
    fn test_zero_vector() {
        let vec1 = arr1(&[1.0, 2.0, 3.0]);
        let vec2 = arr1(&[0.0, 0.0, 0.0]);

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert_eq!(similarity, 0.0, "Zero vector should have 0 similarity");
    }

    #[test]
    fn test_high_dimensional() {
        // Test with 384-dimensional vectors (MiniLM embedding size)
        let vec1 = Array1::from_vec((0..384).map(|i| (i as f32) / 384.0).collect());
        let vec2 = Array1::from_vec((0..384).map(|i| (i as f32) / 384.0).collect());

        let similarity = CosineSimilarity::similarity(&vec1, &vec2);

        assert!((similarity - 1.0).abs() < 0.001, "High-dimensional identical vectors should have similarity ~1.0");
    }
}
