//! # N-gram Index
//!
//! Character-level n-gram index for fuzzy string matching.
//!
//! ## Algorithm
//!
//! 1. **Extract N-grams**: Generate 2-grams and 3-grams from field names
//! 2. **Jaccard Similarity**: Compute overlap between query and term n-grams
//! 3. **Fuzzy Matching**: Handle typos, abbreviations, partial matches
//!
//! This module provides a wrapper around the shared similarity functions.

use crate::mapping::similarity::StringSimilarity;
use crate::mapping::types::OntologyTerm;
use std::collections::HashSet;

/// N-gram index for fuzzy matching
pub struct NgramIndex {
    /// N-gram size (default: 2 for bigrams, 3 for trigrams)
    n: usize,
}

impl NgramIndex {
    /// Create a new n-gram index (default n=2 for bigrams)
    pub fn new() -> Self {
        Self { n: 2 }
    }

    /// Create with specific n-gram size
    pub fn with_size(n: usize) -> Self {
        Self { n }
    }

    /// Score a field's n-grams against an ontology term
    pub fn score(&self, field_ngrams: &[String], term: &OntologyTerm) -> f64 {
        if field_ngrams.is_empty() {
            return 0.0;
        }

        // Extract all searchable text from term
        let mut term_text = String::new();
        term_text.push_str(&term.label.to_lowercase());

        for alias in &term.aliases {
            term_text.push(' ');
            term_text.push_str(&alias.to_lowercase());
        }

        // Generate n-grams from term text
        let term_ngrams = self.generate_ngrams(&term_text);

        // Compute Jaccard similarity
        let field_set: HashSet<String> = field_ngrams.iter().cloned().collect();
        let term_set: HashSet<String> = term_ngrams.into_iter().collect();

        self.jaccard_similarity(&field_set, &term_set)
    }

    /// Generate n-grams from text (delegates to shared StringSimilarity)
    pub fn generate_ngrams(&self, text: &str) -> Vec<String> {
        StringSimilarity::generate_ngrams(text, self.n)
    }

    /// Generate n-grams from field name with padding (delegates to shared StringSimilarity)
    pub fn generate_ngrams_with_padding(&self, text: &str) -> Vec<String> {
        StringSimilarity::generate_ngrams_with_padding(text, self.n)
    }

    /// Compute Jaccard similarity between two sets (delegates to shared StringSimilarity)
    fn jaccard_similarity(&self, set1: &HashSet<String>, set2: &HashSet<String>) -> f64 {
        StringSimilarity::jaccard_similarity(set1, set2)
    }

    /// Compute edit distance (Levenshtein) for very short strings (delegates to shared StringSimilarity)
    pub fn edit_distance(&self, s1: &str, s2: &str) -> usize {
        StringSimilarity::edit_distance(s1, s2)
    }

    /// Compute normalized edit distance (delegates to shared StringSimilarity)
    pub fn normalized_edit_distance(&self, s1: &str, s2: &str) -> f64 {
        StringSimilarity::normalized_edit_distance(s1, s2)
    }
}

impl Default for NgramIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bigrams() {
        let index = NgramIndex::new();
        let bigrams = index.generate_ngrams("email");

        assert_eq!(bigrams, vec!["em", "ma", "ai", "il"]);
    }

    #[test]
    fn test_generate_trigrams() {
        let index = NgramIndex::with_size(3);
        let trigrams = index.generate_ngrams("email");

        assert_eq!(trigrams, vec!["ema", "mai", "ail"]);
    }

    #[test]
    fn test_ngrams_with_padding() {
        let index = NgramIndex::new();
        let ngrams = index.generate_ngrams_with_padding("email");

        // Should have ^ at start and $ at end
        assert!(ngrams[0].starts_with('^'));
        assert!(ngrams.last().unwrap().ends_with('$'));
    }

    #[test]
    fn test_jaccard_similarity() {
        let index = NgramIndex::new();

        let set1: HashSet<String> = vec!["ab".to_string(), "bc".to_string(), "cd".to_string()]
            .into_iter()
            .collect();

        let set2: HashSet<String> = vec!["ab".to_string(), "bc".to_string(), "de".to_string()]
            .into_iter()
            .collect();

        let similarity = index.jaccard_similarity(&set1, &set2);

        // Intersection: {ab, bc} = 2
        // Union: {ab, bc, cd, de} = 4
        // Jaccard: 2/4 = 0.5
        assert!((similarity - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_score_exact_match() {
        let index = NgramIndex::new();

        let term = OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "email".to_string(),
            description: None,
            parent_classes: vec![],
            aliases: vec![],
            examples: vec![],
            data_type: None,
            value_patterns: vec![],
        };

        let field_ngrams = index.generate_ngrams("email");
        let score = index.score(&field_ngrams, &term);

        assert!(score > 0.8); // High score for exact match
    }

    #[test]
    fn test_score_fuzzy_match() {
        let index = NgramIndex::new();

        let term = OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "email".to_string(),
            description: None,
            parent_classes: vec![],
            aliases: vec!["e-mail".to_string()],
            examples: vec![],
            data_type: None,
            value_patterns: vec![],
        };

        let field_ngrams = index.generate_ngrams("emai"); // Typo (missing 'l')
        let score = index.score(&field_ngrams, &term);

        // "emai" vs "email e-mail" should have reasonable Jaccard similarity
        // Bigrams: em, ma, ai vs em, ma, ai, il (email) + e-, -m, ma, ai, il (e-mail)
        // Lower threshold since fuzzy matching is approximate
        assert!(score > 0.3); // Reasonable threshold for typo matching
    }

    #[test]
    fn test_edit_distance() {
        let index = NgramIndex::new();

        assert_eq!(index.edit_distance("email", "email"), 0);
        assert_eq!(index.edit_distance("email", "emai"), 1); // Delete 'l'
        assert_eq!(index.edit_distance("email", "emali"), 2); // Swap last two chars (2 operations)
        assert_eq!(index.edit_distance("email", "mail"), 1); // Delete 'e' at start
        assert_eq!(index.edit_distance("email", "phone"), 5);
    }

    #[test]
    fn test_normalized_edit_distance() {
        let index = NgramIndex::new();

        let dist1 = index.normalized_edit_distance("email", "email");
        assert_eq!(dist1, 0.0);

        let dist2 = index.normalized_edit_distance("email", "emai");
        assert!((dist2 - 0.2).abs() < 0.01); // 1 edit / 5 chars

        let dist3 = index.normalized_edit_distance("email", "phone");
        assert!(dist3 > 0.8); // Very different
    }

    #[test]
    fn test_empty_ngrams() {
        let index = NgramIndex::new();

        let term = OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "email".to_string(),
            description: None,
            parent_classes: vec![],
            aliases: vec![],
            examples: vec![],
            data_type: None,
            value_patterns: vec![],
        };

        let score = index.score(&[], &term);
        assert_eq!(score, 0.0);
    }
}
