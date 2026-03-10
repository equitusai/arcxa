//! # Shared Similarity Module
//!
//! Consolidated similarity functions used across manual, statistical, and semantic mapping.
//!
//! ## String Similarity
//! - **N-grams**: Character-level n-gram generation for fuzzy matching
//! - **Jaccard Similarity**: Set-based similarity using n-gram overlap
//! - **Edit Distance**: Levenshtein distance for string comparison
//! - **Normalized Edit Distance**: Edit distance normalized by string length
//!
//! ## Vector Similarity
//! - **Cosine Similarity**: Vector similarity for semantic embeddings
//! - **Batch Similarity**: Efficient pairwise similarity computation
//!
//! ## Usage
//!
//! ```rust
//! use graphica_coordinator::mapping::similarity::StringSimilarity;
//!
//! // String similarity
//! let ngrams = StringSimilarity::generate_ngrams("email", 3);
//! let distance = StringSimilarity::edit_distance("email", "emai");
//! let similarity = StringSimilarity::jaccard_similarity_strings("email", "e-mail", 2);
//!
//! // Vector similarity
//! # #[cfg(feature = "ndarray")]
//! # {
//! use graphica_coordinator::mapping::similarity::VectorSimilarity;
//! use ndarray::arr1;
//! let vec1 = arr1(&[1.0, 2.0, 3.0]);
//! let vec2 = arr1(&[1.0, 2.0, 3.0]);
//! let score = VectorSimilarity::cosine_similarity(&vec1, &vec2);
//! # }
//! ```

use std::collections::HashSet;

// ============================================================================
// String Similarity Functions
// ============================================================================

/// String-based similarity metrics
pub struct StringSimilarity;

impl StringSimilarity {
    /// Generate character-level n-grams from text
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `n` - N-gram size (2 for bigrams, 3 for trigrams)
    ///
    /// # Example
    /// ```
    /// use graphica_coordinator::mapping::similarity::StringSimilarity;
    /// let ngrams = StringSimilarity::generate_ngrams("email", 2);
    /// // Returns: ["em", "ma", "ai", "il"]
    /// ```
    pub fn generate_ngrams(text: &str, n: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();

        if chars.len() < n {
            return vec![];
        }

        chars
            .windows(n)
            .map(|window| window.iter().collect())
            .collect()
    }

    /// Generate n-grams with padding to capture start/end of string
    ///
    /// Adds ^ prefix and $ suffix for boundary-aware matching
    ///
    /// # Example
    /// ```
    /// use graphica_coordinator::mapping::similarity::StringSimilarity;
    /// let ngrams = StringSimilarity::generate_ngrams_with_padding("hi", 2);
    /// // Returns: ["^h", "hi", "i$"]
    /// ```
    pub fn generate_ngrams_with_padding(text: &str, n: usize) -> Vec<String> {
        let padded = format!("^{}$", text);
        Self::generate_ngrams(&padded, n)
    }

    /// Compute Jaccard similarity between two sets of strings
    ///
    /// Jaccard similarity = |A ∩ B| / |A ∪ B|
    ///
    /// Returns value in range [0.0, 1.0]:
    /// - 0.0 = completely different
    /// - 1.0 = identical
    pub fn jaccard_similarity(set1: &HashSet<String>, set2: &HashSet<String>) -> f64 {
        if set1.is_empty() && set2.is_empty() {
            return 1.0;
        }

        if set1.is_empty() || set2.is_empty() {
            return 0.0;
        }

        let intersection = set1.intersection(set2).count();
        let union = set1.union(set2).count();

        intersection as f64 / union as f64
    }

    /// Compute Jaccard similarity between two strings using n-grams
    ///
    /// Convenience method that generates n-grams and computes Jaccard similarity
    ///
    /// # Example
    /// ```
    /// use graphica_coordinator::mapping::similarity::StringSimilarity;
    /// let similarity = StringSimilarity::jaccard_similarity_strings("email", "e-mail", 2);
    /// ```
    pub fn jaccard_similarity_strings(s1: &str, s2: &str, n: usize) -> f64 {
        let ngrams1: HashSet<String> = Self::generate_ngrams(s1, n).into_iter().collect();
        let ngrams2: HashSet<String> = Self::generate_ngrams(s2, n).into_iter().collect();

        Self::jaccard_similarity(&ngrams1, &ngrams2)
    }

    /// Compute Levenshtein edit distance between two strings
    ///
    /// Returns the minimum number of single-character edits (insertions, deletions, substitutions)
    /// required to change one string into the other.
    ///
    /// # Algorithm Complexity
    /// Time: O(m*n), Space: O(m*n) where m, n are string lengths
    ///
    /// # Example
    /// ```
    /// use graphica_coordinator::mapping::similarity::StringSimilarity;
    /// let distance = StringSimilarity::edit_distance("email", "emai");
    /// assert_eq!(distance, 1); // One deletion
    /// ```
    pub fn edit_distance(s1: &str, s2: &str) -> usize {
        let len1 = s1.chars().count();
        let len2 = s2.chars().count();

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        // Initialize first row and column
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();

        // Compute edit distance using dynamic programming
        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };

                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i - 1][j] + 1, // deletion
                        matrix[i][j - 1] + 1, // insertion
                    ),
                    matrix[i - 1][j - 1] + cost, // substitution
                );
            }
        }

        matrix[len1][len2]
    }

    /// Compute normalized edit distance
    ///
    /// Returns edit distance normalized by the maximum string length
    /// Range: [0.0, 1.0]
    /// - 0.0 = identical strings
    /// - 1.0 = completely different
    ///
    /// # Example
    /// ```
    /// use graphica_coordinator::mapping::similarity::StringSimilarity;
    /// let distance = StringSimilarity::normalized_edit_distance("email", "emai");
    /// assert!((distance - 0.2).abs() < 0.01); // 1 edit / 5 chars
    /// ```
    pub fn normalized_edit_distance(s1: &str, s2: &str) -> f64 {
        let distance = Self::edit_distance(s1, s2);
        let max_len = std::cmp::max(s1.len(), s2.len());

        if max_len == 0 {
            return 0.0;
        }

        distance as f64 / max_len as f64
    }

    /// Compute similarity score from edit distance
    ///
    /// Converts edit distance to similarity score in range [0.0, 1.0]
    /// - 1.0 = identical strings
    /// - 0.0 = completely different
    ///
    /// # Formula
    /// ```text
    /// similarity = 1.0 - (edit_distance / max_length)
    /// ```
    pub fn edit_similarity(s1: &str, s2: &str) -> f64 {
        1.0 - Self::normalized_edit_distance(s1, s2)
    }
}

// ============================================================================
// Vector Similarity Functions
// ============================================================================

#[cfg(feature = "ndarray")]
use ndarray::{Array1, ArrayView1};

#[cfg(feature = "ndarray")]
/// Vector-based similarity metrics (requires ndarray feature)
pub struct VectorSimilarity;

#[cfg(feature = "ndarray")]
impl VectorSimilarity {
    /// Compute cosine similarity between two vectors
    ///
    /// Cosine similarity = (A · B) / (||A|| * ||B||)
    ///
    /// Returns a score from -1.0 (opposite) to 1.0 (identical)
    /// For semantic similarity, typical range is 0.3 to 0.95
    ///
    /// # Example
    /// ```
    /// use ndarray::arr1;
    /// let vec1 = arr1(&[1.0, 2.0, 3.0]);
    /// let vec2 = arr1(&[1.0, 2.0, 3.0]);
    /// let similarity = VectorSimilarity::cosine_similarity(&vec1, &vec2);
    /// assert!((similarity - 1.0).abs() < 0.001);
    /// ```
    pub fn cosine_similarity(vec1: &Array1<f32>, vec2: &Array1<f32>) -> f64 {
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

    /// Compute cosine similarity for pre-normalized (unit) vectors
    ///
    /// Faster version when vectors are already L2-normalized
    /// Simply computes dot product since ||A|| = ||B|| = 1
    pub fn cosine_similarity_normalized(vec1: &Array1<f32>, vec2: &Array1<f32>) -> f64 {
        assert_eq!(
            vec1.len(),
            vec2.len(),
            "Vectors must have the same dimensionality"
        );

        let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();

        dot_product as f64
    }

    /// Normalize a vector to unit length (L2 normalization)
    ///
    /// Modifies vector in-place such that ||vec|| = 1
    pub fn normalize(vec: &mut Array1<f32>) {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm > 0.0 {
            vec.mapv_inplace(|x| x / norm);
        }
    }

    /// Compute pairwise similarities between a query and multiple candidates
    ///
    /// Returns a vector of (index, similarity) pairs sorted by similarity (descending)
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `candidates` - List of candidate vectors
    /// * `top_k` - Number of top results to return
    ///
    /// # Example
    /// ```
    /// use ndarray::arr1;
    /// let query = arr1(&[1.0, 0.0, 0.0]);
    /// let candidates = vec![
    ///     arr1(&[1.0, 0.0, 0.0]),  // Perfect match
    ///     arr1(&[0.9, 0.1, 0.0]),  // Close match
    /// ];
    /// let results = VectorSimilarity::batch_similarity(&query, &candidates, 2);
    /// assert_eq!(results[0].0, 0); // Index of perfect match
    /// ```
    pub fn batch_similarity(
        query: &Array1<f32>,
        candidates: &[Array1<f32>],
        top_k: usize,
    ) -> Vec<(usize, f64)> {
        let mut scores: Vec<(usize, f64)> = candidates
            .iter()
            .enumerate()
            .map(|(idx, candidate)| (idx, Self::cosine_similarity(query, candidate)))
            .collect();

        // Sort by similarity (descending)
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top_k results
        scores.into_iter().take(top_k).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // String Similarity Tests
    #[test]
    fn test_generate_ngrams_bigrams() {
        let bigrams = StringSimilarity::generate_ngrams("email", 2);
        assert_eq!(bigrams, vec!["em", "ma", "ai", "il"]);
    }

    #[test]
    fn test_generate_ngrams_trigrams() {
        let trigrams = StringSimilarity::generate_ngrams("email", 3);
        assert_eq!(trigrams, vec!["ema", "mai", "ail"]);
    }

    #[test]
    fn test_generate_ngrams_with_padding() {
        let ngrams = StringSimilarity::generate_ngrams_with_padding("hi", 2);
        assert!(ngrams[0].starts_with('^'));
        assert!(ngrams.last().unwrap().ends_with('$'));
    }

    #[test]
    fn test_jaccard_similarity() {
        let set1: HashSet<String> = vec!["ab", "bc", "cd"]
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let set2: HashSet<String> = vec!["ab", "bc", "de"]
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let similarity = StringSimilarity::jaccard_similarity(&set1, &set2);

        // Intersection: {ab, bc} = 2, Union: {ab, bc, cd, de} = 4
        assert!((similarity - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_similarity_strings() {
        let similarity = StringSimilarity::jaccard_similarity_strings("email", "email", 2);
        assert_eq!(similarity, 1.0);

        let similarity2 = StringSimilarity::jaccard_similarity_strings("email", "phone", 2);
        assert!(similarity2 < 0.5);
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(StringSimilarity::edit_distance("email", "email"), 0);
        assert_eq!(StringSimilarity::edit_distance("email", "emai"), 1);
        assert_eq!(StringSimilarity::edit_distance("email", "mail"), 1);
        assert_eq!(StringSimilarity::edit_distance("email", "phone"), 5);
    }

    #[test]
    fn test_normalized_edit_distance() {
        let dist = StringSimilarity::normalized_edit_distance("email", "email");
        assert_eq!(dist, 0.0);

        let dist2 = StringSimilarity::normalized_edit_distance("email", "emai");
        assert!((dist2 - 0.2).abs() < 0.01); // 1 edit / 5 chars

        let dist3 = StringSimilarity::normalized_edit_distance("email", "phone");
        assert!(dist3 > 0.8);
    }

    #[test]
    fn test_edit_similarity() {
        let sim = StringSimilarity::edit_similarity("email", "email");
        assert_eq!(sim, 1.0);

        let sim2 = StringSimilarity::edit_similarity("email", "emai");
        assert!((sim2 - 0.8).abs() < 0.01); // 1 - (1/5)
    }

    #[test]
    fn test_empty_strings() {
        assert_eq!(StringSimilarity::edit_distance("", ""), 0);
        assert_eq!(StringSimilarity::normalized_edit_distance("", ""), 0.0);
        assert_eq!(
            StringSimilarity::generate_ngrams("", 2),
            Vec::<String>::new()
        );
    }

    // Vector Similarity Tests (only run with ndarray feature)
    #[cfg(feature = "ndarray")]
    mod vector_tests {
        use super::*;
        use ndarray::arr1;

        #[test]
        fn test_cosine_similarity_identical() {
            let vec1 = arr1(&[1.0, 2.0, 3.0]);
            let vec2 = arr1(&[1.0, 2.0, 3.0]);

            let similarity = VectorSimilarity::cosine_similarity(&vec1, &vec2);
            assert!((similarity - 1.0).abs() < 0.001);
        }

        #[test]
        fn test_cosine_similarity_orthogonal() {
            let vec1 = arr1(&[1.0, 0.0, 0.0]);
            let vec2 = arr1(&[0.0, 1.0, 0.0]);

            let similarity = VectorSimilarity::cosine_similarity(&vec1, &vec2);
            assert!((similarity - 0.0).abs() < 0.001);
        }

        #[test]
        fn test_normalize() {
            let mut vec = arr1(&[3.0, 4.0]);

            VectorSimilarity::normalize(&mut vec);

            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 0.001);
        }

        #[test]
        fn test_batch_similarity() {
            let query = arr1(&[1.0, 0.0, 0.0]);
            let candidates = vec![
                arr1(&[1.0, 0.0, 0.0]), // Perfect match
                arr1(&[0.9, 0.1, 0.0]), // Close match
                arr1(&[0.0, 1.0, 0.0]), // Orthogonal
            ];

            let results = VectorSimilarity::batch_similarity(&query, &candidates, 2);

            assert_eq!(results.len(), 2);
            assert_eq!(results[0].0, 0); // Perfect match first
            assert!(results[0].1 > 0.99);
        }
    }
}
