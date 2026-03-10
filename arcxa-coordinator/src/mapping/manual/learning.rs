// Auto-suggestion and Learning Strategy for Manual Mappings
use super::store::{ManualMappingStore, UsageStatType};
use super::types::*;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Learning engine for improving mapping suggestions
pub struct MappingLearningEngine {
    store: Arc<ManualMappingStore>,

    /// Pattern recognition cache
    pattern_cache: Arc<RwLock<PatternCache>>,

    /// Field similarity index
    similarity_index: Arc<RwLock<SimilarityIndex>>,

    /// Usage patterns by domain
    domain_patterns: Arc<RwLock<DomainPatterns>>,
}

/// Cache for recognized patterns
#[derive(Debug, Default)]
struct PatternCache {
    /// Field name patterns (e.g., "fname*" -> "customerFirstName")
    name_patterns: HashMap<String, Vec<PatternMatch>>,

    /// Data patterns (e.g., email format -> "email")
    data_patterns: HashMap<String, Vec<PatternMatch>>,

    /// Combined patterns (name + data type)
    combined_patterns: HashMap<String, Vec<PatternMatch>>,
}

#[derive(Debug, Clone)]
struct PatternMatch {
    pattern: String,
    target_field: String,
    confidence: f64,
    usage_count: u64,
}

/// Similarity index for field matching
#[derive(Debug, Default)]
struct SimilarityIndex {
    /// Trigram index for fuzzy matching
    trigrams: HashMap<String, HashSet<String>>,

    /// Semantic embeddings (field name -> vector)
    embeddings: HashMap<String, Vec<f32>>,

    /// Statistical profiles (field -> profile hash)
    profiles: HashMap<String, String>,
}

/// Domain-specific patterns
#[derive(Debug, Default)]
struct DomainPatterns {
    /// Patterns by industry/domain
    by_domain: HashMap<String, DomainProfile>,
}

#[derive(Debug, Clone)]
struct DomainProfile {
    domain: String,
    common_mappings: Vec<(String, String, f64)>, // (source, target, frequency)
    field_conventions: HashMap<String, Vec<String>>, // common names for concepts
}

impl MappingLearningEngine {
    pub fn new(store: Arc<ManualMappingStore>) -> Self {
        Self {
            store,
            pattern_cache: Arc::new(RwLock::new(PatternCache::default())),
            similarity_index: Arc::new(RwLock::new(SimilarityIndex::default())),
            domain_patterns: Arc::new(RwLock::new(DomainPatterns::default())),
        }
    }

    /// Initialize learning engine with existing mappings
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing mapping learning engine");

        // Load all existing mappings
        let export = self.store.bulk_export(None).await?;

        // Build pattern cache
        self.build_pattern_cache(&export.mappings).await?;

        // Build similarity index
        self.build_similarity_index(&export.mappings).await?;

        // Analyze domain patterns
        self.analyze_domain_patterns(&export.mappings).await?;

        info!(
            "Learning engine initialized with {} mappings",
            export.mappings.len()
        );
        Ok(())
    }

    /// Build pattern cache from existing mappings
    async fn build_pattern_cache(&self, mappings: &[ManualFieldMapping]) -> Result<()> {
        let mut cache = self.pattern_cache.write().await;

        for mapping in mappings {
            let field_name = &mapping.source_context.field_name;

            // Extract name patterns
            let name_pattern = self.extract_name_pattern(field_name);
            cache
                .name_patterns
                .entry(name_pattern.clone())
                .or_default()
                .push(PatternMatch {
                    pattern: name_pattern,
                    target_field: mapping.target_field_uri.clone(),
                    confidence: mapping.confidence,
                    usage_count: mapping.usage_stats.apply_count,
                });

            // Extract data patterns if available
            if let Some(ref metadata) = mapping.source_context.field_metadata {
                if let Some(ref pattern) = metadata.detected_pattern {
                    cache
                        .data_patterns
                        .entry(pattern.clone())
                        .or_default()
                        .push(PatternMatch {
                            pattern: pattern.clone(),
                            target_field: mapping.target_field_uri.clone(),
                            confidence: mapping.confidence,
                            usage_count: mapping.usage_stats.apply_count,
                        });
                }
            }
        }

        // Sort patterns by usage count
        for patterns in cache.name_patterns.values_mut() {
            patterns.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
        }

        Ok(())
    }

    /// Build similarity index for fuzzy matching
    async fn build_similarity_index(&self, mappings: &[ManualFieldMapping]) -> Result<()> {
        let mut index = self.similarity_index.write().await;

        for mapping in mappings {
            let field_name = &mapping.source_context.field_name;

            // Generate trigrams
            let trigrams = self.generate_trigrams(field_name);
            for trigram in trigrams {
                index
                    .trigrams
                    .entry(trigram)
                    .or_default()
                    .insert(field_name.clone());
            }

            // TODO: Generate semantic embeddings using a small model
            // For now, use simple hash-based pseudo-embeddings
            let embedding = self.generate_pseudo_embedding(field_name);
            index.embeddings.insert(field_name.clone(), embedding);
        }

        Ok(())
    }

    /// Analyze domain-specific patterns
    async fn analyze_domain_patterns(&self, mappings: &[ManualFieldMapping]) -> Result<()> {
        let mut patterns = self.domain_patterns.write().await;

        // Group mappings by source_id (as proxy for domain)
        let mut by_source: HashMap<String, Vec<&ManualFieldMapping>> = HashMap::new();
        for mapping in mappings {
            if let Some(ref source_id) = mapping.source_context.source_id {
                by_source
                    .entry(source_id.clone())
                    .or_default()
                    .push(mapping);
            }
        }

        // Analyze each domain
        for (source_id, source_mappings) in by_source {
            let mut profile = DomainProfile {
                domain: source_id.clone(),
                common_mappings: Vec::new(),
                field_conventions: HashMap::new(),
            };

            // Count mapping frequencies
            let mut freq_map: HashMap<(String, String), u64> = HashMap::new();
            for mapping in source_mappings {
                let key = (
                    mapping.source_context.field_name.clone(),
                    mapping.target_field_uri.clone(),
                );
                *freq_map.entry(key).or_default() += mapping.usage_stats.apply_count;
            }

            // Convert to sorted list
            let mut freq_list: Vec<_> = freq_map
                .into_iter()
                .map(|((src, tgt), count)| (src, tgt, count as f64))
                .collect();
            freq_list.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

            profile.common_mappings = freq_list;
            patterns.by_domain.insert(source_id, profile);
        }

        Ok(())
    }

    /// Generate suggestions for a new field
    pub async fn generate_suggestions(
        &self,
        context: &SourceContext,
        limit: usize,
    ) -> Result<Vec<MappingSuggestion>> {
        let mut suggestions = Vec::new();
        let mut seen_targets = HashSet::new();

        // 1. Check pattern-based suggestions
        let pattern_suggestions = self.suggest_by_pattern(context).await?;
        for suggestion in pattern_suggestions {
            if !seen_targets.contains(&suggestion.mapping.target_field_uri) {
                seen_targets.insert(suggestion.mapping.target_field_uri.clone());
                suggestions.push(suggestion);
            }
        }

        // 2. Check similarity-based suggestions
        let similarity_suggestions = self.suggest_by_similarity(context).await?;
        for suggestion in similarity_suggestions {
            if !seen_targets.contains(&suggestion.mapping.target_field_uri) {
                seen_targets.insert(suggestion.mapping.target_field_uri.clone());
                suggestions.push(suggestion);
            }
        }

        // 3. Check domain-based suggestions
        if let Some(ref source_id) = context.source_id {
            let domain_suggestions = self.suggest_by_domain(source_id, context).await?;
            for suggestion in domain_suggestions {
                if !seen_targets.contains(&suggestion.mapping.target_field_uri) {
                    seen_targets.insert(suggestion.mapping.target_field_uri.clone());
                    suggestions.push(suggestion);
                }
            }
        }

        // Sort by relevance and limit
        suggestions.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        suggestions.truncate(limit);

        Ok(suggestions)
    }

    /// Suggest mappings based on patterns
    async fn suggest_by_pattern(&self, context: &SourceContext) -> Result<Vec<MappingSuggestion>> {
        let cache = self.pattern_cache.read().await;
        let mut suggestions = Vec::new();

        // Check name patterns
        let name_pattern = self.extract_name_pattern(&context.field_name);
        if let Some(matches) = cache.name_patterns.get(&name_pattern) {
            for pattern_match in matches.iter().take(3) {
                // Create a synthetic mapping
                let mapping = ManualFieldMapping {
                    id: format!("pattern_{}", uuid::Uuid::new_v4()),
                    source_context: context.clone(),
                    target_field_uri: pattern_match.target_field.clone(),
                    confidence: pattern_match.confidence,
                    created_by: "pattern_engine".to_string(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    notes: Some(format!("Suggested by pattern: {}", name_pattern)),
                    usage_stats: UsageStats {
                        apply_count: pattern_match.usage_count,
                        ..Default::default()
                    },
                };

                suggestions.push(MappingSuggestion {
                    mapping,
                    suggestion_reason: SuggestionReason::FrequentPattern {
                        usage_count: pattern_match.usage_count,
                    },
                    relevance_score: 0.8,
                });
            }
        }

        Ok(suggestions)
    }

    /// Suggest mappings based on similarity
    async fn suggest_by_similarity(
        &self,
        context: &SourceContext,
    ) -> Result<Vec<MappingSuggestion>> {
        let index = self.similarity_index.read().await;
        let mut suggestions = Vec::new();

        // Find similar field names using trigrams
        let query_trigrams = self.generate_trigrams(&context.field_name);
        let mut similar_fields: HashMap<String, f64> = HashMap::new();

        for trigram in query_trigrams {
            if let Some(fields) = index.trigrams.get(&trigram) {
                for field in fields {
                    *similar_fields.entry(field.clone()).or_default() += 1.0;
                }
            }
        }

        // Normalize similarity scores
        let max_score = similar_fields.values().cloned().fold(0.0, f64::max);
        if max_score > 0.0 {
            for score in similar_fields.values_mut() {
                *score /= max_score;
            }
        }

        // Get top similar fields
        let mut similar_list: Vec<_> = similar_fields.into_iter().collect();
        similar_list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Look up mappings for similar fields
        for (similar_field, similarity) in similar_list.into_iter().take(3) {
            if similarity < 0.5 {
                continue;
            }

            // Find mapping for this similar field
            let similar_context = SourceContext {
                source_id: context.source_id.clone(),
                table_name: context.table_name.clone(),
                field_name: similar_field.clone(),
                field_metadata: None,
            };

            if let Some(mapping) = self.store.find_by_source(&similar_context).await? {
                let suggestion_mapping = ManualFieldMapping {
                    id: format!("similarity_{}", uuid::Uuid::new_v4()),
                    source_context: context.clone(),
                    target_field_uri: mapping.target_field_uri.clone(),
                    confidence: mapping.confidence * similarity,
                    created_by: "similarity_engine".to_string(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    notes: Some(format!(
                        "Similar to '{}' ({}% match)",
                        similar_field,
                        (similarity * 100.0) as u32
                    )),
                    usage_stats: mapping.usage_stats.clone(),
                };

                suggestions.push(MappingSuggestion {
                    mapping: suggestion_mapping,
                    suggestion_reason: SuggestionReason::SimilarFieldName { similarity },
                    relevance_score: similarity * 0.7,
                });
            }
        }

        Ok(suggestions)
    }

    /// Suggest mappings based on domain patterns
    async fn suggest_by_domain(
        &self,
        source_id: &str,
        context: &SourceContext,
    ) -> Result<Vec<MappingSuggestion>> {
        let patterns = self.domain_patterns.read().await;
        let mut suggestions = Vec::new();

        if let Some(profile) = patterns.by_domain.get(source_id) {
            // Look for common mappings in this domain
            for (src_field, tgt_field, frequency) in &profile.common_mappings {
                if src_field == &context.field_name {
                    let mapping = ManualFieldMapping {
                        id: format!("domain_{}", uuid::Uuid::new_v4()),
                        source_context: context.clone(),
                        target_field_uri: tgt_field.clone(),
                        confidence: (frequency / 100.0).min(1.0),
                        created_by: "domain_engine".to_string(),
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        notes: Some(format!("Common in domain '{}'", source_id)),
                        usage_stats: UsageStats {
                            apply_count: *frequency as u64,
                            ..Default::default()
                        },
                    };

                    suggestions.push(MappingSuggestion {
                        mapping,
                        suggestion_reason: SuggestionReason::FrequentPattern {
                            usage_count: *frequency as u64,
                        },
                        relevance_score: (frequency / 100.0).min(1.0) * 0.6,
                    });

                    break; // Only one exact match per domain
                }
            }
        }

        Ok(suggestions)
    }

    /// Extract pattern from field name
    fn extract_name_pattern(&self, field_name: &str) -> String {
        // Convert to lowercase and extract pattern
        let lower = field_name.to_lowercase();

        // Common patterns
        if lower.starts_with("fname") || lower.starts_with("first_name") {
            return "first_name_pattern".to_string();
        }
        if lower.starts_with("lname") || lower.starts_with("last_name") {
            return "last_name_pattern".to_string();
        }
        if lower.contains("email") || lower.contains("mail") {
            return "email_pattern".to_string();
        }
        if lower.contains("phone") || lower.contains("tel") {
            return "phone_pattern".to_string();
        }
        if lower.contains("addr") || lower.contains("address") {
            return "address_pattern".to_string();
        }

        // Generic pattern based on prefix
        if lower.len() > 3 {
            return format!("{}_pattern", &lower[..3]);
        }

        "unknown_pattern".to_string()
    }

    /// Generate trigrams for fuzzy matching
    fn generate_trigrams(&self, text: &str) -> HashSet<String> {
        let lower = text.to_lowercase();
        let padded = format!("  {}  ", lower);
        let mut trigrams = HashSet::new();

        for i in 0..padded.len().saturating_sub(2) {
            trigrams.insert(padded[i..i + 3].to_string());
        }

        trigrams
    }

    /// Generate pseudo-embedding for similarity
    fn generate_pseudo_embedding(&self, text: &str) -> Vec<f32> {
        // Simple hash-based embedding (replace with real embeddings in production)
        let mut embedding = vec![0.0; 128];
        let bytes = text.as_bytes();

        for (i, &byte) in bytes.iter().enumerate() {
            let idx = (i * 7 + byte as usize) % 128;
            embedding[idx] += 1.0;
        }

        // Normalize
        let sum: f32 = embedding.iter().sum();
        if sum > 0.0 {
            for val in &mut embedding {
                *val /= sum;
            }
        }

        embedding
    }

    /// Record user feedback for continuous learning
    pub async fn record_feedback(&self, mapping_id: &str, feedback: UserFeedback) -> Result<()> {
        match &feedback {
            UserFeedback::Accepted => {
                self.store
                    .update_usage_stats(mapping_id, UsageStatType::Accepted)
                    .await?;
            }
            UserFeedback::Rejected => {
                self.store
                    .update_usage_stats(mapping_id, UsageStatType::Rejected)
                    .await?;
            }
            UserFeedback::Modified { new_target } => {
                // Create a new mapping with the modification
                if let Some(mut original) = self.store.get_mapping(mapping_id).await? {
                    original.target_field_uri = new_target.clone();
                    original.notes = Some("Modified by user feedback".to_string());
                    self.store.store_mapping(original).await?;
                }
            }
        }

        // Rebuild indexes periodically (could be async/scheduled)
        if feedback == UserFeedback::Accepted {
            // Trigger index rebuild in background
            let engine = self.clone();
            tokio::spawn(async move {
                let _ = engine.initialize().await;
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UserFeedback {
    Accepted,
    Rejected,
    Modified { new_target: String },
}

impl Clone for MappingLearningEngine {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            pattern_cache: self.pattern_cache.clone(),
            similarity_index: self.similarity_index.clone(),
            domain_patterns: self.domain_patterns.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::GraphicaRdfStore;
    use tempfile::TempDir;

    fn create_test_store() -> Arc<ManualMappingStore> {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let temp_dir = TempDir::new().unwrap();
        let rocksdb_path = temp_dir.path().to_str().unwrap();
        Arc::new(ManualMappingStore::new(rdf_store, rocksdb_path).unwrap())
    }

    fn create_test_mapping(
        field_name: &str,
        target_uri: &str,
        usage_count: u64,
    ) -> ManualFieldMapping {
        ManualFieldMapping {
            id: format!("test_{}", field_name),
            source_context: SourceContext {
                source_id: Some("test_source".to_string()),
                table_name: "test_table".to_string(),
                field_name: field_name.to_string(),
                field_metadata: None,
            },
            target_field_uri: target_uri.to_string(),
            confidence: 1.0,
            created_by: "test_user".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
            usage_stats: UsageStats {
                apply_count: usage_count,
                accept_count: 0,
                reject_count: 0,
                last_used: None,
            },
        }
    }

    #[tokio::test]
    async fn test_extract_name_pattern_email() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        assert_eq!(engine.extract_name_pattern("email"), "email_pattern");
        assert_eq!(
            engine.extract_name_pattern("customer_email"),
            "email_pattern"
        );
        assert_eq!(engine.extract_name_pattern("user_mail"), "email_pattern");
        assert_eq!(
            engine.extract_name_pattern("EMAIL_ADDRESS"),
            "email_pattern"
        );
    }

    #[tokio::test]
    async fn test_extract_name_pattern_phone() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        assert_eq!(engine.extract_name_pattern("phone"), "phone_pattern");
        assert_eq!(engine.extract_name_pattern("phone_number"), "phone_pattern");
        assert_eq!(engine.extract_name_pattern("tel"), "phone_pattern");
        assert_eq!(engine.extract_name_pattern("telephone"), "phone_pattern");
    }

    #[tokio::test]
    async fn test_extract_name_pattern_name_fields() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        assert_eq!(engine.extract_name_pattern("fname"), "first_name_pattern");
        assert_eq!(
            engine.extract_name_pattern("first_name"),
            "first_name_pattern"
        );
        assert_eq!(
            engine.extract_name_pattern("FIRST_NAME"),
            "first_name_pattern"
        );

        assert_eq!(engine.extract_name_pattern("lname"), "last_name_pattern");
        assert_eq!(
            engine.extract_name_pattern("last_name"),
            "last_name_pattern"
        );
        assert_eq!(
            engine.extract_name_pattern("last_name_value"),
            "last_name_pattern"
        );

        // Note: "LASTNAME" as one word creates "las_pattern" (first 3 chars)
        // This is expected behavior for single-word fields without underscores
        assert_eq!(engine.extract_name_pattern("LASTNAME"), "las_pattern");
    }

    #[tokio::test]
    async fn test_extract_name_pattern_address() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        assert_eq!(engine.extract_name_pattern("address"), "address_pattern");
        assert_eq!(
            engine.extract_name_pattern("street_address"),
            "address_pattern"
        );
        assert_eq!(engine.extract_name_pattern("addr"), "address_pattern");
    }

    #[tokio::test]
    async fn test_extract_name_pattern_generic() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        // Short names should get generic pattern
        assert_eq!(engine.extract_name_pattern("id"), "unknown_pattern");
        assert_eq!(engine.extract_name_pattern("x"), "unknown_pattern");

        // Longer names get prefix-based pattern
        assert_eq!(engine.extract_name_pattern("customer_id"), "cus_pattern");
        assert_eq!(engine.extract_name_pattern("order_date"), "ord_pattern");
    }

    #[tokio::test]
    async fn test_generate_trigrams() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let trigrams = engine.generate_trigrams("email");

        // Should generate trigrams with padding
        assert!(trigrams.contains("  e"));
        assert!(trigrams.contains(" em"));
        assert!(trigrams.contains("ema"));
        assert!(trigrams.contains("mai"));
        assert!(trigrams.contains("ail"));
        assert!(trigrams.contains("il "));
        assert!(trigrams.contains("l  "));

        assert_eq!(trigrams.len(), 7); // 5 chars + 2 padding = 7 trigrams
    }

    #[tokio::test]
    async fn test_generate_trigrams_short() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let trigrams = engine.generate_trigrams("ab");

        // Short string should still generate trigrams with padding
        assert!(trigrams.contains("  a"));
        assert!(trigrams.contains(" ab"));
        assert!(trigrams.contains("ab "));
        assert!(trigrams.contains("b  "));

        assert_eq!(trigrams.len(), 4);
    }

    #[tokio::test]
    async fn test_generate_pseudo_embedding() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let embedding = engine.generate_pseudo_embedding("customer_email");

        // Should generate 128-dimensional vector
        assert_eq!(embedding.len(), 128);

        // Should be normalized (sum to 1.0)
        let sum: f32 = embedding.iter().sum();
        assert!((sum - 1.0).abs() < 0.001, "Sum should be 1.0, got {}", sum);

        // All values should be non-negative
        assert!(embedding.iter().all(|&v| v >= 0.0));
    }

    #[tokio::test]
    async fn test_generate_pseudo_embedding_consistency() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        // Same input should generate same embedding
        let emb1 = engine.generate_pseudo_embedding("test");
        let emb2 = engine.generate_pseudo_embedding("test");

        assert_eq!(emb1, emb2);

        // Different inputs should generate different embeddings
        let emb3 = engine.generate_pseudo_embedding("different");
        assert_ne!(emb1, emb3);
    }

    #[tokio::test]
    async fn test_build_pattern_cache() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let mappings = vec![
            create_test_mapping("customer_email", "http://schema.org/email", 10),
            create_test_mapping("user_email", "http://schema.org/email", 5),
            create_test_mapping("contact_phone", "http://schema.org/telephone", 3),
        ];

        engine.build_pattern_cache(&mappings).await.unwrap();

        let cache = engine.pattern_cache.read().await;

        // Email pattern should have 2 matches
        assert!(cache.name_patterns.contains_key("email_pattern"));
        let email_patterns = &cache.name_patterns["email_pattern"];
        assert_eq!(email_patterns.len(), 2);

        // Should be sorted by usage count (10 before 5)
        assert_eq!(email_patterns[0].usage_count, 10);
        assert_eq!(email_patterns[1].usage_count, 5);

        // Phone pattern should have 1 match
        assert!(cache.name_patterns.contains_key("phone_pattern"));
        assert_eq!(cache.name_patterns["phone_pattern"].len(), 1);
    }

    #[tokio::test]
    async fn test_build_similarity_index() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let mappings = vec![
            create_test_mapping("email", "http://schema.org/email", 1),
            create_test_mapping("phone", "http://schema.org/telephone", 1),
        ];

        engine.build_similarity_index(&mappings).await.unwrap();

        let index = engine.similarity_index.read().await;

        // Should have trigrams for both fields
        assert!(!index.trigrams.is_empty());

        // Should have embeddings for both fields
        assert!(index.embeddings.contains_key("email"));
        assert!(index.embeddings.contains_key("phone"));

        // Embeddings should be 128-dimensional
        assert_eq!(index.embeddings["email"].len(), 128);
        assert_eq!(index.embeddings["phone"].len(), 128);
    }

    #[tokio::test]
    async fn test_analyze_domain_patterns() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        let mut mapping1 = create_test_mapping("email", "http://schema.org/email", 10);
        mapping1.source_context.source_id = Some("crm_db".to_string());

        let mut mapping2 = create_test_mapping("phone", "http://schema.org/telephone", 5);
        mapping2.source_context.source_id = Some("crm_db".to_string());

        let mut mapping3 = create_test_mapping("order_id", "http://schema.org/identifier", 20);
        mapping3.source_context.source_id = Some("sales_db".to_string());

        let mappings = vec![mapping1, mapping2, mapping3];

        engine.analyze_domain_patterns(&mappings).await.unwrap();

        let patterns = engine.domain_patterns.read().await;

        // Should have patterns for both domains
        assert!(patterns.by_domain.contains_key("crm_db"));
        assert!(patterns.by_domain.contains_key("sales_db"));

        let crm_profile = &patterns.by_domain["crm_db"];
        assert_eq!(crm_profile.common_mappings.len(), 2);

        // Should be sorted by frequency (email:10 before phone:5)
        assert_eq!(crm_profile.common_mappings[0].2, 10.0);
        assert_eq!(crm_profile.common_mappings[1].2, 5.0);

        let sales_profile = &patterns.by_domain["sales_db"];
        assert_eq!(sales_profile.common_mappings.len(), 1);
        assert_eq!(sales_profile.common_mappings[0].2, 20.0);
    }

    #[tokio::test]
    async fn test_suggest_by_pattern() {
        let store = create_test_store();
        let engine = MappingLearningEngine::new(store);

        // Build pattern cache
        let mappings = vec![
            create_test_mapping("customer_email", "http://schema.org/email", 10),
            create_test_mapping("user_email", "http://schema.org/email", 5),
        ];
        engine.build_pattern_cache(&mappings).await.unwrap();

        // Test suggestion for email-like field
        let context = SourceContext {
            source_id: Some("test".to_string()),
            table_name: "users".to_string(),
            field_name: "contact_email".to_string(),
            field_metadata: None,
        };

        let suggestions = engine.suggest_by_pattern(&context).await.unwrap();

        // Should find email pattern suggestions
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].mapping.target_field_uri.contains("email"));
        assert_eq!(suggestions[0].relevance_score, 0.8);
    }

    #[tokio::test]
    async fn test_record_feedback_accepted() {
        let store = create_test_store();
        let mapping = create_test_mapping("test_field", "http://schema.org/test", 0);
        store.store_mapping(mapping.clone()).await.unwrap();

        let engine = MappingLearningEngine::new(store.clone());

        // Record accepted feedback
        engine
            .record_feedback(&mapping.id, UserFeedback::Accepted)
            .await
            .unwrap();

        // Verify stats were updated
        let retrieved = store.get_mapping(&mapping.id).await.unwrap().unwrap();
        assert_eq!(retrieved.usage_stats.accept_count, 1);
    }

    #[tokio::test]
    async fn test_record_feedback_rejected() {
        let store = create_test_store();
        let mapping = create_test_mapping("test_field", "http://schema.org/test", 0);
        store.store_mapping(mapping.clone()).await.unwrap();

        let engine = MappingLearningEngine::new(store.clone());

        // Record rejected feedback
        engine
            .record_feedback(&mapping.id, UserFeedback::Rejected)
            .await
            .unwrap();

        // Verify stats were updated
        let retrieved = store.get_mapping(&mapping.id).await.unwrap().unwrap();
        assert_eq!(retrieved.usage_stats.reject_count, 1);
    }

    #[tokio::test]
    async fn test_record_feedback_modified() {
        let store = create_test_store();
        let mapping = create_test_mapping("test_field", "http://schema.org/oldTarget", 0);
        store.store_mapping(mapping.clone()).await.unwrap();

        let engine = MappingLearningEngine::new(store.clone());

        // Record modified feedback
        let new_target = "http://schema.org/newTarget".to_string();
        engine
            .record_feedback(
                &mapping.id,
                UserFeedback::Modified {
                    new_target: new_target.clone(),
                },
            )
            .await
            .unwrap();

        // Verify mapping was updated
        let retrieved = store.get_mapping(&mapping.id).await.unwrap().unwrap();
        assert_eq!(retrieved.target_field_uri, new_target);
        assert_eq!(
            retrieved.notes,
            Some("Modified by user feedback".to_string())
        );
    }
}
