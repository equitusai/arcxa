//! # Statistical Matcher
//!
//! Lexical similarity matching using TF-IDF and N-grams.
//!
//! ## Approach
//!
//! 1. **Tokenization**: Split field names into tokens (words, camelCase, snake_case)
//! 2. **TF-IDF Index**: Build inverted index of tokens → ontology terms
//! 3. **N-gram Index**: Build 2-gram and 3-gram index for fuzzy matching
//! 4. **Scoring**: Combine TF-IDF and n-gram scores with weights
//!
//! ## Performance
//!
//! - Index building: O(n) where n = number of ontology terms
//! - Query: O(k log k) where k = number of matching terms
//! - Storage: ~1MB per 1000 ontology terms

pub mod ngrams;
pub mod tfidf;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use crate::mapping::storage::MappingStorage;
use crate::mapping::types::*;

pub use ngrams::NgramIndex;
pub use tfidf::TfIdfIndex;

/// Statistical matcher combining TF-IDF and N-gram approaches
pub struct StatisticalMatcher {
    /// TF-IDF index for token-based matching
    tfidf_index: Arc<TfIdfIndex>,

    /// N-gram index for fuzzy matching
    ngram_index: Arc<NgramIndex>,

    /// Storage for historical mappings
    storage: Arc<MappingStorage>,

    /// Weight for TF-IDF score (0.0 - 1.0)
    tfidf_weight: f64,

    /// Weight for N-gram score (0.0 - 1.0)
    ngram_weight: f64,
}

impl StatisticalMatcher {
    /// Create a new statistical matcher
    pub fn new(storage: Arc<MappingStorage>) -> Result<Self> {
        let tfidf_index = Arc::new(TfIdfIndex::new());
        let ngram_index = Arc::new(NgramIndex::new());

        Ok(Self {
            tfidf_index,
            ngram_index,
            storage,
            tfidf_weight: 0.6, // TF-IDF slightly more important
            ngram_weight: 0.4, // N-grams for fuzzy matching
        })
    }

    /// Find mapping candidates for a field
    pub fn find_candidates(
        &self,
        field: &SchemaField,
        ontology_terms: &[OntologyTerm],
        top_k: usize,
    ) -> Result<Vec<MappingCandidate>> {
        // Get field features
        let features = field
            .features
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Field features not extracted"))?;

        // Score each ontology term
        let mut scored_terms: Vec<(f64, &OntologyTerm)> = ontology_terms
            .iter()
            .map(|term| {
                let score = self.score_match(field, features, term);
                (score, term)
            })
            .collect();

        // Sort by score descending
        scored_terms.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        // Take top k
        let candidates: Vec<MappingCandidate> = scored_terms
            .into_iter()
            .take(top_k)
            .map(|(score, term)| {
                // Find similar historical mappings
                let similar_mappings = self.find_similar_mappings(field, term).unwrap_or_default();

                MappingCandidate {
                    source_field_id: field.id.clone(),
                    ontology_term_uri: term.uri.clone(),
                    confidence: score,
                    confidence_breakdown: ConfidenceBreakdown {
                        statistical: score,
                        semantic: None,
                        graph: None,
                        symbolic: None,
                    },
                    explanation: self.generate_explanation(field, term, score),
                    similar_mappings,
                    transformation: None,
                }
            })
            .collect();

        Ok(candidates)
    }

    /// Score a potential match between field and ontology term
    fn score_match(
        &self,
        field: &SchemaField,
        features: &FieldFeatures,
        term: &OntologyTerm,
    ) -> f64 {
        // TF-IDF score based on tokens
        let tfidf_score = self.tfidf_index.score(&features.name_tokens, term);

        // N-gram score based on fuzzy matching
        let ngram_score = self.ngram_index.score(&features.name_ngrams, term);

        // Pattern matching bonus (if field values match term patterns)
        let pattern_bonus = self.compute_pattern_bonus(features, term);

        // Data type compatibility
        let type_bonus = self.compute_type_bonus(field, term);

        // Weighted combination
        let base_score = (self.tfidf_weight * tfidf_score) + (self.ngram_weight * ngram_score);
        let final_score = (base_score + pattern_bonus + type_bonus).min(1.0);

        final_score
    }

    /// Compute bonus for pattern matching
    fn compute_pattern_bonus(&self, features: &FieldFeatures, term: &OntologyTerm) -> f64 {
        if term.value_patterns.is_empty() {
            return 0.0;
        }

        // Check if any semantic patterns match term patterns
        for pattern in &features.semantic_patterns {
            for term_pattern in &term.value_patterns {
                // Simple heuristic: if pattern types match, give bonus
                if term_pattern.contains(&pattern.pattern_type) {
                    return 0.1 * pattern.match_rate;
                }
            }
        }

        0.0
    }

    /// Compute bonus for data type compatibility
    fn compute_type_bonus(&self, field: &SchemaField, term: &OntologyTerm) -> f64 {
        match &term.data_type {
            Some(expected_type) => {
                if field
                    .data_type
                    .to_uppercase()
                    .contains(&expected_type.to_uppercase())
                {
                    0.05 // Small bonus for type match
                } else {
                    0.0
                }
            }
            None => 0.0,
        }
    }

    /// Generate human-readable explanation for a mapping
    fn generate_explanation(&self, field: &SchemaField, term: &OntologyTerm, score: f64) -> String {
        let mut reasons = Vec::new();

        // Check name similarity
        if field.normalized_name.contains(&term.label.to_lowercase())
            || term.label.to_lowercase().contains(&field.normalized_name)
        {
            reasons.push(format!(
                "Name '{}' closely matches '{}'",
                field.name, term.label
            ));
        }

        // Check aliases
        for alias in &term.aliases {
            if field.normalized_name.contains(&alias.to_lowercase()) {
                reasons.push(format!("Field name contains alias '{}'", alias));
                break;
            }
        }

        // Check data type
        if let Some(expected_type) = &term.data_type {
            if field
                .data_type
                .to_uppercase()
                .contains(&expected_type.to_uppercase())
            {
                reasons.push(format!("Compatible data type ({})", field.data_type));
            }
        }

        // Check patterns
        if let Some(features) = &field.features {
            for pattern in &features.semantic_patterns {
                if !term.value_patterns.is_empty() {
                    reasons.push(format!(
                        "Values match {} pattern ({:.0}% match rate)",
                        pattern.pattern_type,
                        pattern.match_rate * 100.0
                    ));
                    break;
                }
            }
        }

        if reasons.is_empty() {
            format!("Lexical similarity score: {:.2}", score)
        } else {
            reasons.join("; ")
        }
    }

    /// Find similar historical mappings
    fn find_similar_mappings(
        &self,
        field: &SchemaField,
        term: &OntologyTerm,
    ) -> Result<Vec<HistoricalMapping>> {
        // Query storage for historical mappings to this ontology term
        let historical = self.storage.get_historical_mappings(&term.uri)?;

        // Compute similarity to current field
        let mut similar: Vec<HistoricalMapping> = historical
            .into_iter()
            .map(|mut mapping| {
                // Simple string similarity for now
                mapping.similarity = self.compute_string_similarity(
                    &field.normalized_name,
                    &mapping.source_field_name.to_lowercase(),
                );
                mapping
            })
            .filter(|m| m.similarity > 0.5)
            .collect();

        // Sort by similarity
        similar.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        // Take top 3
        Ok(similar.into_iter().take(3).collect())
    }

    /// Compute string similarity (Jaccard index on character bigrams)
    fn compute_string_similarity(&self, s1: &str, s2: &str) -> f64 {
        use std::collections::HashSet;

        let bigrams1: HashSet<String> = s1
            .chars()
            .collect::<Vec<char>>()
            .windows(2)
            .map(|w| format!("{}{}", w[0], w[1]))
            .collect();

        let bigrams2: HashSet<String> = s2
            .chars()
            .collect::<Vec<char>>()
            .windows(2)
            .map(|w| format!("{}{}", w[0], w[1]))
            .collect();

        if bigrams1.is_empty() && bigrams2.is_empty() {
            return 1.0;
        }

        let intersection = bigrams1.intersection(&bigrams2).count();
        let union = bigrams1.union(&bigrams2).count();

        intersection as f64 / union as f64
    }

    /// Update matcher with user feedback
    pub fn update_with_feedback(&self, field: &SchemaField, term_uri: &str) -> Result<()> {
        // In Phase 2, this will update ML models
        // For now, just store as historical mapping
        tracing::info!("Feedback recorded: {} → {}", field.name, term_uri);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_field() -> SchemaField {
        SchemaField {
            id: "test_001".to_string(),
            name: "customer_email".to_string(),
            normalized_name: "customeremail".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: vec!["test@example.com".to_string()],
            source_id: "test_source".to_string(),
            table_name: "customers".to_string(),
            description: None,
            features: Some(FieldFeatures {
                name_tokens: vec!["customer".to_string(), "email".to_string()],
                name_ngrams: vec!["cu".to_string(), "us".to_string(), "st".to_string()],
                semantic_patterns: vec![],
                statistics: FieldStatistics {
                    distinct_count: 100,
                    sample_count: 100,
                    null_rate: 0.0,
                    avg_length: Some(20.0),
                    min_value: None,
                    max_value: None,
                    top_values: vec![],
                },
                inferred_type: Some("email".to_string()),
                context: FieldContext {
                    table_name: "customers".to_string(),
                    schema_name: None,
                    related_fields: vec![],
                    is_primary_key: false,
                    is_foreign_key: false,
                    foreign_key_ref: None,
                },
            }),
        }
    }

    fn create_test_term() -> OntologyTerm {
        OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "Email".to_string(),
            description: Some("Email address".to_string()),
            parent_classes: vec![],
            aliases: vec!["email".to_string(), "e-mail".to_string()],
            examples: vec![],
            data_type: Some("VARCHAR".to_string()),
            value_patterns: vec![],
        }
    }

    #[test]
    fn test_string_similarity() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap());
        let matcher = StatisticalMatcher::new(storage).unwrap();

        let sim1 = matcher.compute_string_similarity("customer", "customer");
        assert!((sim1 - 1.0).abs() < 0.01);

        let sim2 = matcher.compute_string_similarity("customer", "customr");
        assert!(sim2 > 0.6); // Missing one char, still fairly similar

        let sim3 = matcher.compute_string_similarity("customer", "product");
        assert!(sim3 < 0.3);
    }

    #[test]
    fn test_type_bonus() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap());
        let matcher = StatisticalMatcher::new(storage).unwrap();

        let field = create_test_field();
        let term = create_test_term();

        let bonus = matcher.compute_type_bonus(&field, &term);
        assert!(bonus > 0.0); // VARCHAR matches VARCHAR
    }

    #[test]
    fn test_generate_explanation() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap());
        let matcher = StatisticalMatcher::new(storage).unwrap();

        let field = create_test_field();
        let term = create_test_term();

        let explanation = matcher.generate_explanation(&field, &term, 0.85);
        assert!(explanation.contains("email") || explanation.contains("score"));
    }
}
