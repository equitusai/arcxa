//! Lightweight Text Embeddings
//!
//! Pure Rust implementation of text embeddings for ontology alignment.
//! Uses TF-IDF with character n-grams for semantic similarity without
//! requiring heavy ML dependencies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Minimum n-gram size (characters)
    pub min_ngram: usize,

    /// Maximum n-gram size (characters)
    pub max_ngram: usize,

    /// Include word tokens
    pub include_words: bool,

    /// Embedding dimension (controlled by feature selection)
    pub dimension: usize,

    /// Minimum document frequency for feature selection
    pub min_df: usize,

    /// Maximum document frequency (as fraction of docs)
    pub max_df: f64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            min_ngram: 2,
            max_ngram: 4,
            include_words: true,
            dimension: 384, // Match MiniLM dimension for compatibility
            min_df: 1,
            max_df: 0.8,
        }
    }
}

/// Text embedding generator using TF-IDF
pub struct EmbeddingGenerator {
    config: EmbeddingConfig,
    vocabulary: HashMap<String, usize>,
    idf_weights: Vec<f32>,
    document_count: usize,
}

impl EmbeddingGenerator {
    /// Create a new embedding generator
    pub fn new(config: EmbeddingConfig) -> Self {
        Self {
            config,
            vocabulary: HashMap::new(),
            idf_weights: Vec::new(),
            document_count: 0,
        }
    }

    /// Fit the embedding generator on a corpus of documents
    pub fn fit(&mut self, documents: &[String]) {
        self.document_count = documents.len();

        // Extract features from all documents
        let mut doc_term_matrix: Vec<HashMap<String, usize>> = Vec::new();
        let mut document_frequencies: HashMap<String, usize> = HashMap::new();

        for doc in documents {
            let features = self.extract_features(doc);
            let mut term_counts = HashMap::new();

            for feature in &features {
                *term_counts.entry(feature.clone()).or_insert(0) += 1;
            }

            // Update document frequencies
            for term in term_counts.keys() {
                *document_frequencies.entry(term.clone()).or_insert(0) += 1;
            }

            doc_term_matrix.push(term_counts);
        }

        // Filter features by document frequency
        let mut valid_features: Vec<(String, usize)> = document_frequencies
            .into_iter()
            .filter(|(_, df)| {
                let df_ratio = *df as f64 / self.document_count as f64;
                *df >= self.config.min_df && df_ratio <= self.config.max_df
            })
            .collect();

        // Sort by document frequency (descending) and take top features
        valid_features.sort_by(|a, b| b.1.cmp(&a.1));
        valid_features.truncate(self.config.dimension);

        // Build vocabulary
        self.vocabulary = valid_features
            .iter()
            .enumerate()
            .map(|(idx, (term, _))| (term.clone(), idx))
            .collect();

        // Compute IDF weights
        self.idf_weights = valid_features
            .iter()
            .map(|(_, df)| {
                let idf = ((self.document_count as f32 / *df as f32).ln() + 1.0).max(1.0);
                idf
            })
            .collect();
    }

    /// Transform text into an embedding vector
    pub fn transform(&self, text: &str) -> Vec<f32> {
        let features = self.extract_features(text);
        let mut term_counts: HashMap<String, usize> = HashMap::new();

        for feature in features {
            *term_counts.entry(feature).or_insert(0) += 1;
        }

        // Create TF-IDF vector
        let mut embedding = vec![0.0; self.vocabulary.len()];

        let total_terms = term_counts.values().sum::<usize>() as f32;

        for (term, count) in term_counts {
            if let Some(&idx) = self.vocabulary.get(&term) {
                let tf = count as f32 / total_terms;
                let idf = self.idf_weights[idx];
                embedding[idx] = tf * idf;
            }
        }

        // L2 normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        embedding
    }

    /// Extract features from text (words + character n-grams)
    fn extract_features(&self, text: &str) -> Vec<String> {
        let mut features = Vec::new();

        let normalized = text.to_lowercase();

        // Word tokens
        if self.config.include_words {
            let words: Vec<&str> = normalized
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect();

            for word in &words {
                features.push(format!("w:{}", word));
            }

            // Bigrams
            for i in 0..words.len().saturating_sub(1) {
                features.push(format!("b:{}_{}", words[i], words[i + 1]));
            }
        }

        // Character n-grams
        let chars: Vec<char> = normalized.chars().collect();
        for n in self.config.min_ngram..=self.config.max_ngram {
            // Skip if n-gram size is larger than text
            if n > chars.len() {
                continue;
            }

            for i in 0..=(chars.len() - n) {
                let ngram: String = chars[i..i + n].iter().collect();
                features.push(format!("c{n}:{}", ngram));
            }
        }

        features
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.vocabulary.len()
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.vocabulary.len()
    }
}

/// Pre-fitted embedding generator for common English text
pub struct PretrainedEmbeddings {
    generator: EmbeddingGenerator,
}

impl PretrainedEmbeddings {
    /// Create a pretrained embeddings generator
    /// Uses common database/ontology terminology
    pub fn new() -> Self {
        let config = EmbeddingConfig::default();
        let mut generator = EmbeddingGenerator::new(config);

        // Training corpus of common database/ontology terms
        let corpus = vec![
            "customer identifier".to_string(),
            "customer id".to_string(),
            "customer number".to_string(),
            "email address".to_string(),
            "email".to_string(),
            "phone number".to_string(),
            "telephone".to_string(),
            "first name".to_string(),
            "given name".to_string(),
            "last name".to_string(),
            "surname".to_string(),
            "family name".to_string(),
            "full name".to_string(),
            "product name".to_string(),
            "product code".to_string(),
            "product identifier".to_string(),
            "sku".to_string(),
            "stock keeping unit".to_string(),
            "upc code".to_string(),
            "barcode".to_string(),
            "price".to_string(),
            "unit price".to_string(),
            "cost".to_string(),
            "amount".to_string(),
            "total".to_string(),
            "subtotal".to_string(),
            "order number".to_string(),
            "order id".to_string(),
            "purchase order".to_string(),
            "invoice number".to_string(),
            "invoice id".to_string(),
            "transaction id".to_string(),
            "quantity".to_string(),
            "quantity on hand".to_string(),
            "inventory".to_string(),
            "stock level".to_string(),
            "street address".to_string(),
            "address".to_string(),
            "city".to_string(),
            "state".to_string(),
            "province".to_string(),
            "postal code".to_string(),
            "zip code".to_string(),
            "country".to_string(),
            "date created".to_string(),
            "creation date".to_string(),
            "timestamp".to_string(),
            "modified date".to_string(),
            "updated at".to_string(),
            "status".to_string(),
            "state".to_string(),
            "category".to_string(),
            "type".to_string(),
            "description".to_string(),
            "notes".to_string(),
            "comments".to_string(),
            "user id".to_string(),
            "username".to_string(),
            "account id".to_string(),
            "tracking number".to_string(),
            "shipping id".to_string(),
            "carrier".to_string(),
            "loyalty tier".to_string(),
            "membership level".to_string(),
            "customer segment".to_string(),
        ];

        generator.fit(&corpus);

        Self { generator }
    }

    /// Embed text using the pretrained model
    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.generator.transform(text)
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.generator.dimension()
    }
}

impl Default for PretrainedEmbeddings {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate cosine similarity between two embedding vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    // Vectors should already be L2-normalized, so dot product = cosine similarity
    // But we'll be defensive and normalize anyway
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_generator_fit() {
        let documents = vec![
            "customer email address".to_string(),
            "customer phone number".to_string(),
            "product name".to_string(),
            "product price".to_string(),
        ];

        let config = EmbeddingConfig::default();
        let mut generator = EmbeddingGenerator::new(config);
        generator.fit(&documents);

        assert!(generator.vocab_size() > 0);
        assert!(generator.vocab_size() <= 384);
    }

    #[test]
    fn test_embedding_generation() {
        let documents = vec![
            "customer email".to_string(),
            "customer phone".to_string(),
            "product name".to_string(),
        ];

        let config = EmbeddingConfig::default();
        let mut generator = EmbeddingGenerator::new(config);
        generator.fit(&documents);

        let embedding = generator.transform("customer email address");
        assert_eq!(embedding.len(), generator.dimension());

        // Check L2 normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01 || norm == 0.0);
    }

    #[test]
    fn test_similarity() {
        let documents = vec![
            "customer email".to_string(),
            "customer phone".to_string(),
            "product name".to_string(),
        ];

        let config = EmbeddingConfig::default();
        let mut generator = EmbeddingGenerator::new(config);
        generator.fit(&documents);

        let emb1 = generator.transform("customer email");
        let emb2 = generator.transform("customer email address");
        let emb3 = generator.transform("product name");

        let sim_similar = cosine_similarity(&emb1, &emb2);
        let sim_different = cosine_similarity(&emb1, &emb3);

        // Similar texts should have higher similarity
        assert!(sim_similar > sim_different);
    }

    #[test]
    fn test_pretrained_embeddings() {
        let embedder = PretrainedEmbeddings::new();

        assert!(embedder.dimension() > 0);

        let emb = embedder.embed("customer_id");
        assert_eq!(emb.len(), embedder.dimension());
    }

    #[test]
    fn test_pretrained_similarity() {
        let embedder = PretrainedEmbeddings::new();

        let emb1 = embedder.embed("customer_id");
        let emb2 = embedder.embed("customer identifier");
        let emb3 = embedder.embed("product_name");

        let sim_similar = cosine_similarity(&emb1, &emb2);
        let sim_different = cosine_similarity(&emb1, &emb3);

        println!(
            "Similarity (customer_id vs customer identifier): {}",
            sim_similar
        );
        println!(
            "Similarity (customer_id vs product_name): {}",
            sim_different
        );

        // Similar concepts should have higher similarity
        assert!(sim_similar > sim_different);
        assert!(sim_similar > 0.3); // Should have meaningful similarity
    }

    #[test]
    fn test_character_ngrams() {
        let config = EmbeddingConfig {
            min_ngram: 2,
            max_ngram: 3,
            include_words: false,
            ..Default::default()
        };

        let mut generator = EmbeddingGenerator::new(config);
        let corpus = vec!["email".to_string(), "phone".to_string()];
        generator.fit(&corpus);

        // "email" and "e-mail" should be similar due to character n-grams
        let emb1 = generator.transform("email");
        let emb2 = generator.transform("e-mail");

        let similarity = cosine_similarity(&emb1, &emb2);
        assert!(similarity > 0.5); // Should capture character-level similarity
    }
}
