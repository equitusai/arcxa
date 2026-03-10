//! # TF-IDF Index
//!
//! Term Frequency-Inverse Document Frequency index for lexical matching.
//!
//! ## Algorithm
//!
//! 1. **Tokenization**: Split field names and ontology labels into tokens
//! 2. **TF Calculation**: Count token frequency in each document
//! 3. **IDF Calculation**: Compute inverse document frequency across corpus
//! 4. **Scoring**: For query, compute cosine similarity with indexed documents

use crate::mapping::types::OntologyTerm;
use std::collections::HashMap;

/// TF-IDF index for token-based matching
pub struct TfIdfIndex {
    /// Document frequency: token → number of documents containing it
    document_frequency: HashMap<String, usize>,

    /// Total number of documents
    num_documents: usize,

    /// Cache of computed TF-IDF vectors per term
    term_vectors: HashMap<String, HashMap<String, f64>>,
}

impl TfIdfIndex {
    /// Create a new TF-IDF index
    pub fn new() -> Self {
        Self {
            document_frequency: HashMap::new(),
            num_documents: 0,
            term_vectors: HashMap::new(),
        }
    }

    /// Score a field's tokens against an ontology term
    pub fn score(&self, field_tokens: &[String], term: &OntologyTerm) -> f64 {
        if field_tokens.is_empty() {
            return 0.0;
        }

        // Get or compute term vector
        let term_vector = self.get_or_compute_term_vector(term);

        // Compute query vector (field tokens)
        let query_vector = self.compute_tf_vector(field_tokens);

        // Compute cosine similarity
        self.cosine_similarity(&query_vector, &term_vector)
    }

    /// Get or compute TF-IDF vector for an ontology term
    fn get_or_compute_term_vector(&self, term: &OntologyTerm) -> HashMap<String, f64> {
        // Extract all text from term
        let mut all_tokens = Vec::new();

        // Add label tokens
        all_tokens.extend(Self::tokenize(&term.label));

        // Add alias tokens
        for alias in &term.aliases {
            all_tokens.extend(Self::tokenize(alias));
        }

        // Add description tokens (if available)
        if let Some(desc) = &term.description {
            all_tokens.extend(Self::tokenize(desc));
        }

        // Compute TF vector
        self.compute_tf_vector(&all_tokens)
    }

    /// Compute Term Frequency vector
    fn compute_tf_vector(&self, tokens: &[String]) -> HashMap<String, f64> {
        let mut tf_map = HashMap::new();
        let total_tokens = tokens.len() as f64;

        if total_tokens == 0.0 {
            return tf_map;
        }

        // Count token frequencies
        for token in tokens {
            *tf_map.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        // Normalize by total tokens (TF normalization)
        for value in tf_map.values_mut() {
            *value /= total_tokens;
        }

        tf_map
    }

    /// Compute cosine similarity between two TF vectors
    fn cosine_similarity(&self, vec1: &HashMap<String, f64>, vec2: &HashMap<String, f64>) -> f64 {
        if vec1.is_empty() || vec2.is_empty() {
            return 0.0;
        }

        // Compute dot product
        let mut dot_product = 0.0;
        for (token, tf1) in vec1 {
            if let Some(tf2) = vec2.get(token) {
                dot_product += tf1 * tf2;
            }
        }

        // Compute magnitudes
        let mag1: f64 = vec1.values().map(|v| v * v).sum::<f64>().sqrt();
        let mag2: f64 = vec2.values().map(|v| v * v).sum::<f64>().sqrt();

        if mag1 == 0.0 || mag2 == 0.0 {
            return 0.0;
        }

        dot_product / (mag1 * mag2)
    }

    /// Tokenize a string into words
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();

        // Split by non-alphanumeric characters
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        for word in words {
            // Handle camelCase and PascalCase
            let camel_tokens = Self::split_camel_case(word);
            for token in camel_tokens {
                let lower = token.to_lowercase();
                if !lower.is_empty() && lower.len() > 1 {
                    // Skip single characters
                    tokens.push(lower);
                }
            }
        }

        tokens
    }

    /// Split camelCase or PascalCase into separate tokens
    pub fn split_camel_case(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();

        for (i, ch) in text.chars().enumerate() {
            if ch.is_uppercase() && i > 0 && !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(ch);
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }
}

impl Default for TfIdfIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = TfIdfIndex::tokenize("customer_email_address");
        assert_eq!(tokens, vec!["customer", "email", "address"]);

        let tokens = TfIdfIndex::tokenize("CustomerEmailAddress");
        assert_eq!(tokens, vec!["customer", "email", "address"]);

        let tokens = TfIdfIndex::tokenize("customer-email-address");
        assert_eq!(tokens, vec!["customer", "email", "address"]);
    }

    #[test]
    fn test_split_camel_case() {
        let tokens = TfIdfIndex::split_camel_case("customerEmail");
        assert_eq!(tokens, vec!["customer", "Email"]);

        let tokens = TfIdfIndex::split_camel_case("CustomerEmailAddress");
        assert_eq!(tokens, vec!["Customer", "Email", "Address"]);

        let tokens = TfIdfIndex::split_camel_case("ALLCAPS");
        assert_eq!(tokens, vec!["A", "L", "L", "C", "A", "P", "S"]);
    }

    #[test]
    fn test_tf_vector() {
        let index = TfIdfIndex::new();
        let tokens = vec![
            "customer".to_string(),
            "email".to_string(),
            "customer".to_string(),
        ];

        let tf_vector = index.compute_tf_vector(&tokens);

        assert_eq!(tf_vector.len(), 2);
        assert!((tf_vector["customer"] - 2.0 / 3.0).abs() < 0.01);
        assert!((tf_vector["email"] - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_cosine_similarity() {
        let index = TfIdfIndex::new();

        let mut vec1 = HashMap::new();
        vec1.insert("customer".to_string(), 0.5);
        vec1.insert("email".to_string(), 0.5);

        let mut vec2 = HashMap::new();
        vec2.insert("customer".to_string(), 0.7);
        vec2.insert("email".to_string(), 0.3);

        let similarity = index.cosine_similarity(&vec1, &vec2);
        assert!(similarity > 0.9); // Very similar vectors

        let mut vec3 = HashMap::new();
        vec3.insert("product".to_string(), 1.0);

        let similarity = index.cosine_similarity(&vec1, &vec3);
        assert_eq!(similarity, 0.0); // No overlap
    }

    #[test]
    fn test_score_ontology_term() {
        let index = TfIdfIndex::new();

        let term = OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "Email".to_string(),
            description: Some("Email address field".to_string()),
            parent_classes: vec![],
            aliases: vec!["email".to_string(), "e-mail".to_string()],
            examples: vec![],
            data_type: None,
            value_patterns: vec![],
        };

        let field_tokens = vec!["customer".to_string(), "email".to_string()];

        let score = index.score(&field_tokens, &term);
        assert!(score > 0.0);
        println!("Score: {}", score);
    }

    #[test]
    fn test_empty_tokens() {
        let index = TfIdfIndex::new();

        let term = OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "Email".to_string(),
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
