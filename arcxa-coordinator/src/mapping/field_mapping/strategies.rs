//! # Matching Strategies
//!
//! Collection of pluggable matching strategies for ontology mapping.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, instrument};

use super::types::*;
use crate::mapping::manual::store::UsageStatType;
use crate::mapping::manual::{ManualMappingStore, SourceContext};
use crate::mapping::similarity::StringSimilarity;

// ============================================================================
// Manual Strategy (1.0 confidence - HIGHEST PRIORITY)
// ============================================================================

/// Manual mapping strategy - returns user-defined mappings with 100% confidence
pub struct ManualStrategy {
    store: Arc<ManualMappingStore>,
}

impl ManualStrategy {
    pub fn new(store: Arc<ManualMappingStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl MatchingStrategy for ManualStrategy {
    fn name(&self) -> &str {
        "manual"
    }

    fn min_confidence(&self) -> f64 {
        1.0 // Manual mappings always have 100% confidence
    }

    fn max_confidence(&self) -> f64 {
        1.0 // Manual mappings always have 100% confidence
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        // Manual strategy applies to all fields
        // We'll check if a manual mapping exists in find_matches
        true
    }

    #[instrument(skip(self, field, ontology_terms, _context))]
    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        debug!(
            "Manual strategy checking for mapping: table={}, field={}",
            field.table_name, field.name
        );

        // Build source context from field descriptor
        let source_context = SourceContext {
            source_id: Some(field.source_id.clone()),
            table_name: field.table_name.clone(),
            field_name: field.name.clone(),
            field_metadata: None, // Field descriptor doesn't have detailed metadata
        };

        // Check if manual mapping exists
        if let Some(manual_mapping) = self.store.find_by_source(&source_context).await? {
            debug!(
                "Found manual mapping: {} -> {}",
                field.name, manual_mapping.target_field_uri
            );

            // Find the corresponding ontology term
            if let Some(ontology_term) = ontology_terms
                .iter()
                .find(|t| t.uri == manual_mapping.target_field_uri)
            {
                let strategy_match = StrategyMatch {
                    strategy_name: self.name().to_string(),
                    ontology_uri: ontology_term.uri.clone(),
                    confidence: 1.0, // Manual mappings always have 100% confidence
                    explanation: format!(
                        "Manual mapping defined by user '{}' at {}",
                        manual_mapping.created_by,
                        manual_mapping.created_at.format("%Y-%m-%d %H:%M:%S")
                    ),
                    metadata: vec![
                        ("mapping_id".to_string(), manual_mapping.id.clone()),
                        ("created_by".to_string(), manual_mapping.created_by.clone()),
                        (
                            "apply_count".to_string(),
                            manual_mapping.usage_stats.apply_count.to_string(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                };

                // Update usage statistics (fire and forget)
                let store_clone = self.store.clone();
                let mapping_id = manual_mapping.id.clone();
                tokio::spawn(async move {
                    if let Err(e) = store_clone
                        .update_usage_stats(&mapping_id, UsageStatType::Applied)
                        .await
                    {
                        debug!("Failed to update usage stats for {}: {}", mapping_id, e);
                    }
                });

                return Ok(vec![strategy_match]);
            } else {
                debug!(
                    "Manual mapping target URI '{}' not found in ontology terms",
                    manual_mapping.target_field_uri
                );
            }
        }

        Ok(vec![])
    }
}

// ============================================================================
// Pattern Strategy (0.85-0.95 confidence)
// ============================================================================

/// Pattern-based matching strategy
pub struct PatternStrategy {
    detector: Arc<dyn PatternDetector>,
}

impl PatternStrategy {
    pub fn new(detector: Arc<dyn PatternDetector>) -> Self {
        Self { detector }
    }
}

#[async_trait]
impl MatchingStrategy for PatternStrategy {
    fn name(&self) -> &str {
        "pattern"
    }

    fn min_confidence(&self) -> f64 {
        0.85
    }

    fn max_confidence(&self) -> f64 {
        0.95
    }

    fn applies_to(&self, field: &FieldDescriptor) -> bool {
        // Apply to fields with sample values
        !field.sample_values.is_empty()
    }

    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        _ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        let patterns = self.detector.detect_patterns(&field.sample_values);
        let mut matches = Vec::new();

        for pattern in patterns {
            if pattern.confidence < 0.8 {
                continue; // Skip low confidence patterns
            }

            let (uri, confidence) = match pattern.pattern_type {
                PatternType::Email => ("http://schema.org/email", 0.95),
                PatternType::Phone => ("http://schema.org/telephone", 0.92),
                PatternType::URL => ("http://schema.org/url", 0.90),
                PatternType::PostalCode => ("http://schema.org/postalCode", 0.88),
                PatternType::Date => ("http://schema.org/Date", 0.87),
                PatternType::Currency => ("http://schema.org/price", 0.85),
                _ => continue,
            };

            matches.push(StrategyMatch {
                strategy_name: self.name().to_string(),
                ontology_uri: uri.to_string(),
                confidence,
                explanation: format!(
                    "Pattern detected: {:?} ({}/{} samples)",
                    pattern.pattern_type, pattern.match_count, pattern.total_count
                ),
                metadata: HashMap::new(),
            });
        }

        Ok(matches)
    }
}

// ============================================================================
// Semantic Strategy (0.80-0.90 confidence)
// ============================================================================

/// Semantic similarity strategy using transformer embeddings
pub struct SemanticStrategy {
    // client: Arc<crate::mapping::semantic::SemanticMatcherClient>, // PRE-EXISTING ISSUE
}

impl SemanticStrategy {
    // PRE-EXISTING ISSUE: Semantic module doesn't exist
    // pub fn new(client: Arc<crate::mapping::semantic::SemanticMatcherClient>) -> Self {
    //     Self { client }
    // }
}

#[async_trait]
impl MatchingStrategy for SemanticStrategy {
    fn name(&self) -> &str {
        "semantic"
    }

    fn min_confidence(&self) -> f64 {
        0.80
    }

    fn max_confidence(&self) -> f64 {
        0.90
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        true // Apply to all fields
    }

    async fn find_matches(
        &self,
        _field: &FieldDescriptor,
        _ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        // PRE-EXISTING ISSUE: Semantic module doesn't exist - returning empty matches
        Ok(Vec::new())
        /* ORIGINAL CODE - disabled due to missing semantic module
        let mut matches = Vec::new();

        for term in ontology_terms {
            // Compute semantic similarity
            let similarity = self.client.similarity(&field.name, &term.label).await?;

            if similarity >= 0.7 {
                // Map similarity to confidence range (0.7-1.0 -> 0.80-0.90)
                let confidence = 0.80 + (similarity - 0.7) * 0.33;

                matches.push(StrategyMatch {
                    strategy_name: self.name().to_string(),
                    ontology_uri: term.uri.clone(),
                    confidence,
                    explanation: format!("Semantic similarity: {:.2}", similarity),
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(matches)
        */
    }
}

// ============================================================================
// Statistical Strategy (0.70-0.85 confidence)
// ============================================================================

/// Statistical matching using TF-IDF and N-grams
pub struct StatisticalStrategy {
    tfidf_index: crate::mapping::statistical::tfidf::TfIdfIndex,
}

impl StatisticalStrategy {
    pub fn new() -> Self {
        Self {
            tfidf_index: crate::mapping::statistical::tfidf::TfIdfIndex::new(),
        }
    }
}

#[async_trait]
impl MatchingStrategy for StatisticalStrategy {
    fn name(&self) -> &str {
        "statistical"
    }

    fn min_confidence(&self) -> f64 {
        0.70
    }

    fn max_confidence(&self) -> f64 {
        0.85
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        true
    }

    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        let mut matches = Vec::new();

        // Tokenize field name for TF-IDF using shared tokenization
        let field_tokens = crate::mapping::statistical::tfidf::TfIdfIndex::tokenize(&field.name);

        for term in ontology_terms {
            // Convert unified OntologyTerm to legacy format for TF-IDF
            let legacy_term = crate::mapping::types::OntologyTerm {
                uri: term.uri.clone(),
                label: term.label.clone(),
                description: term.description.clone(),
                parent_classes: vec![],
                aliases: vec![],
                examples: vec![],
                data_type: None,
                value_patterns: vec![],
            };

            // Compute TF-IDF similarity score
            let similarity = self.tfidf_index.score(&field_tokens, &legacy_term);

            // Only consider matches above threshold
            if similarity >= 0.3 {
                // Map similarity [0.3, 1.0] to confidence range [0.70, 0.85]
                let normalized_similarity = (similarity - 0.3) / 0.7; // Normalize to [0, 1]
                let confidence = self.min_confidence()
                    + (normalized_similarity * (self.max_confidence() - self.min_confidence()));

                matches.push(StrategyMatch {
                    strategy_name: self.name().to_string(),
                    ontology_uri: term.uri.clone(),
                    confidence,
                    explanation: format!("TF-IDF similarity: {:.2}", similarity),
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(matches)
    }
}

// No helper methods needed - using TfIdfIndex::tokenize()

// ============================================================================
// Lexical Strategy (0.65-0.80 confidence)
// ============================================================================

/// Lexical similarity using edit distance, Jaro-Winkler, etc.
pub struct LexicalStrategy {
    // TODO: Add similarity calculators
}

impl LexicalStrategy {
    pub fn new() -> Self {
        Self {}
    }

    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        // Delegate to shared StringSimilarity
        StringSimilarity::edit_distance(s1, s2)
    }
}

#[async_trait]
impl MatchingStrategy for LexicalStrategy {
    fn name(&self) -> &str {
        "lexical"
    }

    fn min_confidence(&self) -> f64 {
        0.65
    }

    fn max_confidence(&self) -> f64 {
        0.80
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        true
    }

    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        let mut matches = Vec::new();

        for term in ontology_terms {
            let distance =
                self.levenshtein_distance(&field.normalized_name, &term.label.to_lowercase());
            let max_len = field.normalized_name.len().max(term.label.len());

            if max_len > 0 {
                let similarity = 1.0 - (distance as f64 / max_len as f64);

                if similarity >= 0.6 {
                    // Map similarity to confidence range
                    let confidence = 0.65 + (similarity - 0.6) * 0.375;

                    matches.push(StrategyMatch {
                        strategy_name: self.name().to_string(),
                        ontology_uri: term.uri.clone(),
                        confidence,
                        explanation: format!("Lexical similarity: {:.2}", similarity),
                        metadata: HashMap::new(),
                    });
                }
            }
        }

        Ok(matches)
    }
}

// ============================================================================
// Registry Strategy (0.75-0.90 confidence)
// ============================================================================

/// Custom ontology matching from registry with multi-level matching
pub struct RegistryStrategy {
    tfidf_index: crate::mapping::statistical::tfidf::TfIdfIndex,
}

impl RegistryStrategy {
    pub fn new() -> Self {
        Self {
            tfidf_index: crate::mapping::statistical::tfidf::TfIdfIndex::new(),
        }
    }

    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        // Delegate to shared StringSimilarity
        StringSimilarity::edit_distance(s1, s2)
    }
}

#[async_trait]
impl MatchingStrategy for RegistryStrategy {
    fn name(&self) -> &str {
        "registry"
    }

    fn min_confidence(&self) -> f64 {
        0.75
    }

    fn max_confidence(&self) -> f64 {
        0.90
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        true
    }

    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        let mut matches = Vec::new();
        let field_lower = field.normalized_name.to_lowercase();
        let field_tokens = crate::mapping::statistical::tfidf::TfIdfIndex::tokenize(&field.name);

        for term in ontology_terms {
            // Focus on custom ontologies (non-schema.org)
            if term.namespace.starts_with("schema.org") {
                continue;
            }

            let term_lower = term.label.to_lowercase();
            let mut match_info: Option<(f64, String)> = None;

            // 1. Exact match (highest confidence)
            if field_lower == term_lower {
                match_info = Some((0.90, "Exact match".to_string()));
            }

            // 2. TF-IDF similarity for token-based matching
            if match_info.is_none() {
                let legacy_term = crate::mapping::types::OntologyTerm {
                    uri: term.uri.clone(),
                    label: term.label.clone(),
                    description: term.description.clone(),
                    parent_classes: vec![],
                    aliases: vec![],
                    examples: vec![],
                    data_type: None,
                    value_patterns: vec![],
                };

                let tfidf_similarity = self.tfidf_index.score(&field_tokens, &legacy_term);
                if tfidf_similarity >= 0.4 {
                    let confidence = 0.75 + (tfidf_similarity - 0.4) * 0.25; // Map [0.4, 1.0] -> [0.75, 0.90]
                    match_info = Some((
                        confidence,
                        format!("Token similarity: {:.2}", tfidf_similarity),
                    ));
                }
            }

            // 4. Levenshtein distance for fuzzy matching
            if match_info.is_none() {
                let distance = self.levenshtein_distance(&field_lower, &term_lower);
                let max_len = field_lower.len().max(term_lower.len());

                if max_len > 0 {
                    let similarity = 1.0 - (distance as f64 / max_len as f64);
                    if similarity >= 0.7 {
                        let confidence = 0.75 + (similarity - 0.7) * 0.5; // Map [0.7, 1.0] -> [0.75, 0.90]
                        match_info = Some((
                            confidence,
                            format!("Fuzzy match (similarity: {:.2})", similarity),
                        ));
                    }
                }
            }

            // 5. Substring match as fallback (lowest confidence)
            if match_info.is_none() {
                if field_lower.contains(&term_lower) || term_lower.contains(&field_lower) {
                    match_info = Some((0.76, "Substring match".to_string()));
                }
            }

            // Add match if found
            if let Some((confidence, explanation)) = match_info {
                matches.push(StrategyMatch {
                    strategy_name: self.name().to_string(),
                    ontology_uri: term.uri.clone(),
                    confidence,
                    explanation: format!("{} in custom ontology ({})", explanation, term.namespace),
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(matches)
    }
}

// ============================================================================
// Heuristic Strategy (0.60-0.75 confidence)
// ============================================================================

/// Rule-based heuristic matching with pattern variants and context awareness
pub struct HeuristicStrategy {
    rules: Vec<HeuristicRule>,
}

#[derive(Clone)]
struct HeuristicRule {
    patterns: Vec<String>, // Multiple pattern variants
    ontology_uri: String,
    base_confidence: f64,
    requires_word_boundary: bool, // Must be a complete word
}

impl HeuristicStrategy {
    pub fn new() -> Self {
        let rules = vec![
            // Email patterns
            HeuristicRule {
                patterns: vec![
                    "email".to_string(),
                    "emailaddress".to_string(),
                    "mail".to_string(),
                ],
                ontology_uri: "http://schema.org/email".to_string(),
                base_confidence: 0.72,
                requires_word_boundary: false,
            },
            // Name patterns
            HeuristicRule {
                patterns: vec!["name".to_string(), "fullname".to_string()],
                ontology_uri: "http://schema.org/name".to_string(),
                base_confidence: 0.68,
                requires_word_boundary: true,
            },
            HeuristicRule {
                patterns: vec![
                    "firstname".to_string(),
                    "givenname".to_string(),
                    "fname".to_string(),
                ],
                ontology_uri: "http://schema.org/givenName".to_string(),
                base_confidence: 0.70,
                requires_word_boundary: false,
            },
            HeuristicRule {
                patterns: vec![
                    "lastname".to_string(),
                    "surname".to_string(),
                    "familyname".to_string(),
                    "lname".to_string(),
                ],
                ontology_uri: "http://schema.org/familyName".to_string(),
                base_confidence: 0.70,
                requires_word_boundary: false,
            },
            // Phone patterns
            HeuristicRule {
                patterns: vec![
                    "phone".to_string(),
                    "telephone".to_string(),
                    "phonenumber".to_string(),
                    "tel".to_string(),
                    "mobile".to_string(),
                ],
                ontology_uri: "http://schema.org/telephone".to_string(),
                base_confidence: 0.71,
                requires_word_boundary: false,
            },
            // Address patterns
            HeuristicRule {
                patterns: vec![
                    "address".to_string(),
                    "streetaddress".to_string(),
                    "street".to_string(),
                ],
                ontology_uri: "http://schema.org/streetAddress".to_string(),
                base_confidence: 0.68,
                requires_word_boundary: false,
            },
            HeuristicRule {
                patterns: vec!["city".to_string()],
                ontology_uri: "http://schema.org/addressLocality".to_string(),
                base_confidence: 0.67,
                requires_word_boundary: true,
            },
            HeuristicRule {
                patterns: vec![
                    "state".to_string(),
                    "province".to_string(),
                    "region".to_string(),
                ],
                ontology_uri: "http://schema.org/addressRegion".to_string(),
                base_confidence: 0.66,
                requires_word_boundary: true,
            },
            HeuristicRule {
                patterns: vec![
                    "zip".to_string(),
                    "zipcode".to_string(),
                    "postalcode".to_string(),
                    "postcode".to_string(),
                ],
                ontology_uri: "http://schema.org/postalCode".to_string(),
                base_confidence: 0.70,
                requires_word_boundary: false,
            },
            HeuristicRule {
                patterns: vec!["country".to_string()],
                ontology_uri: "http://schema.org/addressCountry".to_string(),
                base_confidence: 0.67,
                requires_word_boundary: true,
            },
            // Price/monetary patterns
            HeuristicRule {
                patterns: vec![
                    "price".to_string(),
                    "cost".to_string(),
                    "amount".to_string(),
                ],
                ontology_uri: "http://schema.org/price".to_string(),
                base_confidence: 0.65,
                requires_word_boundary: true,
            },
            // Date/time patterns
            HeuristicRule {
                patterns: vec!["date".to_string(), "datetime".to_string()],
                ontology_uri: "http://schema.org/Date".to_string(),
                base_confidence: 0.66,
                requires_word_boundary: false,
            },
            HeuristicRule {
                patterns: vec!["createdat".to_string(), "created".to_string()],
                ontology_uri: "http://schema.org/dateCreated".to_string(),
                base_confidence: 0.68,
                requires_word_boundary: false,
            },
            HeuristicRule {
                patterns: vec![
                    "updatedat".to_string(),
                    "modified".to_string(),
                    "lastmodified".to_string(),
                ],
                ontology_uri: "http://schema.org/dateModified".to_string(),
                base_confidence: 0.68,
                requires_word_boundary: false,
            },
            // Identifier patterns
            HeuristicRule {
                patterns: vec!["identifier".to_string()],
                ontology_uri: "http://schema.org/identifier".to_string(),
                base_confidence: 0.67,
                requires_word_boundary: false,
            },
            // URL patterns
            HeuristicRule {
                patterns: vec![
                    "url".to_string(),
                    "website".to_string(),
                    "webpage".to_string(),
                    "link".to_string(),
                ],
                ontology_uri: "http://schema.org/url".to_string(),
                base_confidence: 0.69,
                requires_word_boundary: false,
            },
            // Description patterns
            HeuristicRule {
                patterns: vec![
                    "description".to_string(),
                    "desc".to_string(),
                    "summary".to_string(),
                ],
                ontology_uri: "http://schema.org/description".to_string(),
                base_confidence: 0.65,
                requires_word_boundary: false,
            },
        ];

        Self { rules }
    }

    /// Extract word tokens from field name
    fn extract_tokens(&self, text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Calculate confidence based on match quality
    fn calculate_confidence(
        &self,
        base: f64,
        is_exact: bool,
        is_suffix: bool,
        is_prefix: bool,
    ) -> f64 {
        let mut conf = base;

        if is_exact {
            conf += 0.03; // Exact match bonus
        } else if is_suffix {
            conf += 0.02; // Suffix match (e.g., "customer_email" matches "email")
        } else if is_prefix {
            conf += 0.01; // Prefix match (e.g., "email_address" matches "email")
        }

        // Clamp to strategy range
        conf.min(0.75).max(0.60)
    }
}

#[async_trait]
impl MatchingStrategy for HeuristicStrategy {
    fn name(&self) -> &str {
        "heuristic"
    }

    fn min_confidence(&self) -> f64 {
        0.60
    }

    fn max_confidence(&self) -> f64 {
        0.75
    }

    fn applies_to(&self, _field: &FieldDescriptor) -> bool {
        true
    }

    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        _ontology_terms: &[OntologyTerm],
        _context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>> {
        let mut matches = Vec::new();
        let field_lower = field.normalized_name.to_lowercase();
        let tokens = self.extract_tokens(&field.name);

        for rule in &self.rules {
            for pattern in &rule.patterns {
                let mut match_quality = None;

                // Check for exact match
                if field_lower == *pattern {
                    match_quality = Some(("exact match", true, false, false));
                }
                // Check for exact token match
                else if tokens.contains(pattern) {
                    match_quality = Some(("exact token match", true, false, false));
                }
                // Check for suffix match
                else if field_lower.ends_with(pattern) {
                    match_quality = Some(("suffix match", false, true, false));
                }
                // Check for prefix match
                else if field_lower.starts_with(pattern) {
                    match_quality = Some(("prefix match", false, false, true));
                }
                // Check for substring match (if word boundary not required)
                else if !rule.requires_word_boundary && field_lower.contains(pattern) {
                    match_quality = Some(("substring match", false, false, false));
                }

                if let Some((match_type, is_exact, is_suffix, is_prefix)) = match_quality {
                    let confidence = self.calculate_confidence(
                        rule.base_confidence,
                        is_exact,
                        is_suffix,
                        is_prefix,
                    );

                    matches.push(StrategyMatch {
                        strategy_name: self.name().to_string(),
                        ontology_uri: rule.ontology_uri.clone(),
                        confidence,
                        explanation: format!(
                            "Heuristic rule: {} on pattern '{}'",
                            match_type, pattern
                        ),
                        metadata: HashMap::new(),
                    });

                    break; // Only one match per rule
                }
            }
        }

        Ok(matches)
    }
}
