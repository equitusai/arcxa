//! # Shared Components
//!
//! Reusable components used by multiple strategies.

use anyhow::Result;
use parking_lot::RwLock;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::types::{
    DetectedPattern, EmbeddingCache, OntologyCache, OntologyTerm, PatternDetector, PatternType,
};

// ============================================================================
// Pattern Detector Implementation
// ============================================================================

/// Pattern detector implementation
pub struct PatternDetectorImpl {
    patterns: HashMap<PatternType, Regex>,
}

impl PatternDetectorImpl {
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // Email pattern
        patterns.insert(
            PatternType::Email,
            Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap(),
        );

        // Phone pattern (various formats)
        patterns.insert(
            PatternType::Phone,
            Regex::new(r"^[\+]?[(]?[0-9]{1,4}[)]?[-\s\.]?[(]?[0-9]{1,4}[)]?[-\s\.]?[0-9]{1,5}[-\s\.]?[0-9]{1,5}$").unwrap(),
        );

        // URL pattern
        patterns.insert(
            PatternType::URL,
            Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap(),
        );

        // US Postal Code
        patterns.insert(
            PatternType::PostalCode,
            Regex::new(r"^\d{5}(-\d{4})?$").unwrap(),
        );

        // SSN pattern
        patterns.insert(
            PatternType::SSN,
            Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap(),
        );

        // Credit card pattern (basic)
        patterns.insert(
            PatternType::CreditCard,
            Regex::new(r"^\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}$").unwrap(),
        );

        // UUID pattern
        patterns.insert(
            PatternType::UUID,
            Regex::new(
                r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            )
            .unwrap(),
        );

        // IPv4 pattern
        patterns.insert(
            PatternType::IPv4,
            Regex::new(r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$").unwrap(),
        );

        // Date patterns (ISO format)
        patterns.insert(
            PatternType::Date,
            Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(),
        );

        // DateTime patterns (ISO format)
        patterns.insert(
            PatternType::DateTime,
            Regex::new(r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}").unwrap(),
        );

        // Currency pattern
        patterns.insert(
            PatternType::Currency,
            Regex::new(r"^[$£€¥]\s?\d+\.?\d*$").unwrap(),
        );

        // Percentage pattern
        patterns.insert(
            PatternType::Percentage,
            Regex::new(r"^\d+\.?\d*\s?%$").unwrap(),
        );

        Self { patterns }
    }

    fn check_pattern(&self, pattern_type: &PatternType, value: &str) -> bool {
        if let Some(regex) = self.patterns.get(pattern_type) {
            regex.is_match(value)
        } else {
            false
        }
    }
}

impl PatternDetector for PatternDetectorImpl {
    fn detect_patterns(&self, samples: &[String]) -> Vec<DetectedPattern> {
        if samples.is_empty() {
            return vec![];
        }

        let mut pattern_counts: HashMap<PatternType, usize> = HashMap::new();
        let mut pattern_examples: HashMap<PatternType, String> = HashMap::new();

        // Count matches for each pattern
        for sample in samples {
            for (pattern_type, _) in &self.patterns {
                if self.check_pattern(pattern_type, sample) {
                    *pattern_counts.entry(pattern_type.clone()).or_insert(0) += 1;

                    // Store first example
                    pattern_examples
                        .entry(pattern_type.clone())
                        .or_insert_with(|| sample.clone());
                }
            }
        }

        // Convert counts to detected patterns
        let total_count = samples.len();
        let mut detected = Vec::new();

        for (pattern_type, match_count) in pattern_counts {
            let confidence = match_count as f64 / total_count as f64;

            // Only report patterns with reasonable confidence
            if confidence >= 0.5 {
                detected.push(DetectedPattern {
                    pattern_type: pattern_type.clone(),
                    confidence,
                    match_count,
                    total_count,
                    example: pattern_examples.get(&pattern_type).cloned(),
                });
            }
        }

        // Sort by confidence (descending)
        detected.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        detected
    }

    fn matches_pattern(&self, samples: &[String], pattern_type: PatternType) -> bool {
        if samples.is_empty() {
            return false;
        }

        let match_count = samples
            .iter()
            .filter(|s| self.check_pattern(&pattern_type, s))
            .count();

        // At least 80% should match
        match_count as f64 / samples.len() as f64 >= 0.8
    }
}

// ============================================================================
// Ontology Cache Implementation
// ============================================================================

/// Ontology cache implementation
pub struct OntologyCacheImpl {
    terms: Arc<RwLock<Vec<OntologyTerm>>>,
    registry_client: Option<Arc<crate::mapping::ontology_registry::RegistryClient>>,
    last_refresh: Arc<RwLock<Instant>>,
    ttl: Duration,
}

impl OntologyCacheImpl {
    pub fn new(
        registry_client: Option<Arc<crate::mapping::ontology_registry::RegistryClient>>,
        ttl_seconds: u64,
    ) -> Result<Self> {
        let cache = Self {
            terms: Arc::new(RwLock::new(Vec::new())),
            registry_client,
            last_refresh: Arc::new(RwLock::new(Instant::now())),
            ttl: Duration::from_secs(ttl_seconds),
        };

        // Initial load
        cache.refresh()?;

        Ok(cache)
    }

    fn should_refresh(&self) -> bool {
        let last = *self.last_refresh.read();
        last.elapsed() > self.ttl
    }
}

impl OntologyCache for OntologyCacheImpl {
    fn get_terms(&self) -> Vec<OntologyTerm> {
        // Check if refresh needed
        if self.should_refresh() {
            let _ = self.refresh(); // Ignore errors, use cached data
        }

        self.terms.read().clone()
    }

    fn get_terms_by_namespace(&self, namespace: &str) -> Vec<OntologyTerm> {
        self.get_terms()
            .into_iter()
            .filter(|t| t.namespace == namespace)
            .collect()
    }

    fn refresh(&self) -> Result<()> {
        let new_terms = if let Some(client) = &self.registry_client {
            // Get from registry
            let registry_terms = client.get_ontology_terms()?;

            // Convert to our format
            registry_terms
                .into_iter()
                .map(|t| {
                    // Extract namespace from URI (e.g., "http://schema.org/email" -> "schema.org")
                    let namespace = extract_namespace_from_uri(&t.uri);

                    OntologyTerm {
                        uri: t.uri,
                        label: t.label,
                        namespace,
                        term_type: super::types::OntologyTermType::Property,
                        description: t.description,
                        data_type: None,
                        alt_labels: vec![],
                    }
                })
                .collect()
        } else {
            // Use default terms
            get_default_ontology_terms()
        };

        *self.terms.write() = new_terms;
        *self.last_refresh.write() = Instant::now();

        Ok(())
    }
}

// ============================================================================
// Embedding Cache Implementation (Stub)
// ============================================================================

/// Simple in-memory embedding cache
pub struct EmbeddingCacheImpl {
    cache: Arc<RwLock<HashMap<String, Vec<f32>>>>,
    max_size: usize,
}

impl EmbeddingCacheImpl {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }
}

impl EmbeddingCache for EmbeddingCacheImpl {
    fn get(&self, text: &str) -> Option<Vec<f32>> {
        self.cache.read().get(text).cloned()
    }

    fn put(&self, text: &str, embedding: Vec<f32>) {
        let mut cache = self.cache.write();

        // Simple size limit (could use LRU in production)
        if cache.len() >= self.max_size {
            // Remove a random entry
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }

        cache.insert(text.to_string(), embedding);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract namespace from URI (e.g., "http://schema.org/email" -> "schema.org")
fn extract_namespace_from_uri(uri: &str) -> String {
    // Try to parse as URL and extract host
    if let Some(start) = uri.find("://") {
        let after_scheme = &uri[start + 3..];
        if let Some(slash_pos) = after_scheme.find('/') {
            return after_scheme[..slash_pos].to_string();
        } else {
            return after_scheme.to_string();
        }
    }

    // Fallback: try to find domain-like pattern
    if let Some(start) = uri.rfind('/') {
        if let Some(domain_start) = uri[..start].rfind('/') {
            let potential_domain = &uri[domain_start + 1..start];
            if potential_domain.contains('.') {
                return potential_domain.to_string();
            }
        }
    }

    // Default fallback
    "unknown".to_string()
}

/// Get default ontology terms (fallback)
fn get_default_ontology_terms() -> Vec<OntologyTerm> {
    vec![
        OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "email".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some("Email address".to_string()),
            data_type: Some("Text".to_string()),
            alt_labels: vec!["e-mail".to_string(), "emailAddress".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/name".to_string(),
            label: "name".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some("The name of the item".to_string()),
            data_type: Some("Text".to_string()),
            alt_labels: vec!["fullName".to_string(), "displayName".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/telephone".to_string(),
            label: "telephone".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some("The telephone number".to_string()),
            data_type: Some("Text".to_string()),
            alt_labels: vec!["phone".to_string(), "phoneNumber".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/PostalAddress".to_string(),
            label: "PostalAddress".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Class,
            description: Some("The mailing address".to_string()),
            data_type: None,
            alt_labels: vec!["address".to_string(), "mailingAddress".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/identifier".to_string(),
            label: "identifier".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some(
                "The identifier property represents any kind of identifier".to_string(),
            ),
            data_type: Some("Text".to_string()),
            alt_labels: vec!["id".to_string(), "ID".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/price".to_string(),
            label: "price".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some("The offer price of a product".to_string()),
            data_type: Some("Number".to_string()),
            alt_labels: vec!["cost".to_string(), "amount".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/age".to_string(),
            label: "age".to_string(),
            namespace: "schema.org".to_string(),
            term_type: super::types::OntologyTermType::Property,
            description: Some("Age of a person or thing".to_string()),
            data_type: Some("Number".to_string()),
            alt_labels: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_detection_email() {
        let detector = PatternDetectorImpl::new();
        let samples = vec![
            "john@example.com".to_string(),
            "jane.doe@company.org".to_string(),
            "support@test.net".to_string(),
        ];

        let patterns = detector.detect_patterns(&samples);

        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Email);
        assert_eq!(patterns[0].confidence, 1.0);
        assert_eq!(patterns[0].match_count, 3);
    }

    #[test]
    fn test_pattern_detection_mixed() {
        let detector = PatternDetectorImpl::new();
        let samples = vec![
            "john@example.com".to_string(),
            "not an email".to_string(),
            "another@email.org".to_string(),
        ];

        let patterns = detector.detect_patterns(&samples);

        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Email);
        assert!((patterns[0].confidence - 0.67).abs() < 0.01);
    }

    #[test]
    fn test_pattern_detection_phone() {
        let detector = PatternDetectorImpl::new();
        let samples = vec![
            "555-123-4567".to_string(),
            "+1-555-987-6543".to_string(),
            "(555) 555-5555".to_string(),
        ];

        let patterns = detector.detect_patterns(&samples);

        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Phone);
    }

    #[test]
    fn test_pattern_matches() {
        let detector = PatternDetectorImpl::new();
        let email_samples = vec!["john@example.com".to_string(), "jane@test.org".to_string()];

        assert!(detector.matches_pattern(&email_samples, PatternType::Email));
        assert!(!detector.matches_pattern(&email_samples, PatternType::Phone));
    }

    #[test]
    fn test_embedding_cache() {
        let cache = EmbeddingCacheImpl::new(2);

        // Add embeddings
        cache.put("test1", vec![1.0, 2.0, 3.0]);
        cache.put("test2", vec![4.0, 5.0, 6.0]);

        // Retrieve
        assert!(cache.get("test1").is_some());
        assert!(cache.get("test2").is_some());
        assert!(cache.get("test3").is_none());

        // Add third (should evict one)
        cache.put("test3", vec![7.0, 8.0, 9.0]);
        assert!(cache.get("test3").is_some());
    }
}
