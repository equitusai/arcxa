//! Cross-Source Relationship Detection
//!
//! Automatically discovers potential relationships between fields across different datasources
//! using multiple detection strategies: name matching, semantic type matching, and value overlap analysis.

use super::field::SemanticType;
use super::profiler::RelationshipInfo;
use super::profiler::RelationshipType;
use super::UnifiedSchema;
use std::collections::HashSet;

/// Configuration for relationship detection
#[derive(Debug, Clone)]
pub struct RelationshipDetectorConfig {
    /// Minimum confidence threshold for reporting relationships (0.0 - 1.0)
    pub min_confidence: f64,

    /// Minimum name similarity score for name-based detection (0.0 - 1.0)
    pub min_name_similarity: f64,

    /// Minimum value overlap percentage for value-based detection (0.0 - 1.0)
    pub min_value_overlap: f64,

    /// Maximum number of sample values to compare for overlap analysis
    pub max_sample_size: usize,

    /// Enable name-based detection
    pub enable_name_matching: bool,

    /// Enable semantic type-based detection
    pub enable_semantic_matching: bool,

    /// Enable value overlap analysis
    pub enable_value_analysis: bool,
}

impl Default for RelationshipDetectorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.6,
            min_name_similarity: 0.7,
            min_value_overlap: 0.3,
            max_sample_size: 1000,
            enable_name_matching: true,
            enable_semantic_matching: true,
            enable_value_analysis: true,
        }
    }
}

/// Relationship detector for cross-source analysis
pub struct RelationshipDetector {
    config: RelationshipDetectorConfig,
}

impl RelationshipDetector {
    /// Create a new relationship detector with default configuration
    pub fn new() -> Self {
        Self {
            config: RelationshipDetectorConfig::default(),
        }
    }

    /// Create a new relationship detector with custom configuration
    pub fn with_config(config: RelationshipDetectorConfig) -> Self {
        Self { config }
    }

    /// Detect relationships across multiple schemas
    pub fn detect_relationships(&self, schemas: &[UnifiedSchema]) -> Vec<RelationshipInfo> {
        let mut relationships = Vec::new();

        // Compare each pair of schemas
        for i in 0..schemas.len() {
            for j in (i + 1)..schemas.len() {
                let source_schema = &schemas[i];
                let target_schema = &schemas[j];

                // Find relationships from source to target
                relationships.extend(self.detect_between_schemas(source_schema, target_schema));

                // Find relationships from target to source
                relationships.extend(self.detect_between_schemas(target_schema, source_schema));
            }
        }

        // Filter by minimum confidence
        relationships.retain(|r| r.confidence >= self.config.min_confidence);

        // Sort by confidence (highest first)
        relationships.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        relationships
    }

    /// Detect relationships between two specific schemas
    fn detect_between_schemas(
        &self,
        source_schema: &UnifiedSchema,
        target_schema: &UnifiedSchema,
    ) -> Vec<RelationshipInfo> {
        let mut relationships = Vec::new();

        for source_field in &source_schema.fields {
            for target_field in &target_schema.fields {
                // Skip if field names are identical (likely the same field in both schemas)
                if source_schema.name == target_schema.name
                    && source_field.name == target_field.name
                {
                    continue;
                }

                let mut detection_signals = Vec::new();

                // 1. Name-based detection
                if self.config.enable_name_matching {
                    if let Some(confidence) =
                        self.detect_by_name(&source_field.name, &target_field.name)
                    {
                        detection_signals.push(("name", confidence));
                    }
                }

                // 2. Semantic type matching
                if self.config.enable_semantic_matching {
                    if let Some(confidence) = self.detect_by_semantic_type(
                        &source_field.semantic.semantic_type,
                        &target_field.semantic.semantic_type,
                    ) {
                        detection_signals.push(("semantic", confidence));
                    }
                }

                // 3. Value overlap analysis
                if self.config.enable_value_analysis {
                    if let Some(confidence) =
                        self.detect_by_value_overlap(&source_field.profile, &target_field.profile)
                    {
                        detection_signals.push(("value_overlap", confidence));
                    }
                }

                // Combine signals and create relationship if confidence is high enough
                if !detection_signals.is_empty() {
                    let combined_confidence = self.combine_confidence_scores(&detection_signals);

                    if combined_confidence >= self.config.min_confidence {
                        // Determine relationship type based on field characteristics
                        let relationship_type =
                            self.determine_relationship_type(source_field, target_field);

                        relationships.push(RelationshipInfo {
                            source: source_schema.name.clone(),
                            source_field: source_field.name.clone(),
                            target: target_schema.name.clone(),
                            target_field: target_field.name.clone(),
                            relationship_type,
                            confidence: combined_confidence,
                        });
                    }
                }
            }
        }

        relationships
    }

    /// Detect relationship by field name similarity
    fn detect_by_name(&self, source_name: &str, target_name: &str) -> Option<f64> {
        let similarity = self.calculate_name_similarity(source_name, target_name);

        if similarity >= self.config.min_name_similarity {
            Some(similarity)
        } else {
            None
        }
    }

    /// Calculate name similarity using multiple heuristics
    fn calculate_name_similarity(&self, name1: &str, name2: &str) -> f64 {
        let normalized1 = self.normalize_field_name(name1);
        let normalized2 = self.normalize_field_name(name2);

        // Exact match
        if normalized1 == normalized2 {
            return 1.0;
        }

        // Foreign key pattern: "customer_id" matches "id" in customers table
        if normalized1.ends_with("_id") && normalized2 == "id" {
            return 0.95;
        }
        if normalized2.ends_with("_id") && normalized1 == "id" {
            return 0.95;
        }

        // Common prefix/suffix matching
        if normalized1.contains(&normalized2) || normalized2.contains(&normalized1) {
            let shorter = normalized1.len().min(normalized2.len());
            let longer = normalized1.len().max(normalized2.len());
            return (shorter as f64) / (longer as f64);
        }

        // Levenshtein distance-based similarity
        let distance = self.levenshtein_distance(&normalized1, &normalized2);
        let max_len = normalized1.len().max(normalized2.len()) as f64;

        if max_len == 0.0 {
            return 0.0;
        }

        1.0 - (distance as f64 / max_len)
    }

    /// Normalize field name for comparison
    fn normalize_field_name(&self, name: &str) -> String {
        name.to_lowercase()
            .replace(['-', ' '], "_")
            .trim_matches('_')
            .to_string()
    }

    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();

        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for (i, c1) in s1.chars().enumerate() {
            for (j, c2) in s2.chars().enumerate() {
                let cost = if c1 == c2 { 0 } else { 1 };
                matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                    .min(matrix[i + 1][j] + 1)
                    .min(matrix[i][j] + cost);
            }
        }

        matrix[len1][len2]
    }

    /// Detect relationship by semantic type matching
    fn detect_by_semantic_type(
        &self,
        source_semantic: &Option<SemanticType>,
        target_semantic: &Option<SemanticType>,
    ) -> Option<f64> {
        match (source_semantic, target_semantic) {
            // Specific cases must come before general case
            (Some(SemanticType::CustomerId), Some(SemanticType::CustomerId)) => Some(0.95),
            (Some(SemanticType::OrderNumber), Some(SemanticType::OrderNumber)) => Some(0.95),
            (Some(SemanticType::InvoiceNumber), Some(SemanticType::InvoiceNumber)) => Some(0.95),
            (Some(SemanticType::UUID), Some(SemanticType::UUID)) => Some(0.85),
            (Some(s1), Some(s2)) if s1 == s2 => {
                // Same semantic type suggests potential relationship (general case)
                Some(0.8)
            }
            _ => None,
        }
    }

    /// Detect relationship by value overlap analysis
    fn detect_by_value_overlap(
        &self,
        source_profile: &Option<super::profile::FieldProfile>,
        target_profile: &Option<super::profile::FieldProfile>,
    ) -> Option<f64> {
        let source_samples = source_profile.as_ref()?.samples.as_slice();
        let target_samples = target_profile.as_ref()?.samples.as_slice();

        if source_samples.is_empty() || target_samples.is_empty() {
            return None;
        }

        // Create sets for faster lookup
        let source_set: HashSet<_> = source_samples.iter().collect();
        let target_set: HashSet<_> = target_samples.iter().collect();

        // Calculate overlap
        let intersection: HashSet<_> = source_set.intersection(&target_set).collect();
        let overlap_count = intersection.len();

        if overlap_count == 0 {
            return None;
        }

        // Calculate overlap percentage (relative to smaller set)
        let min_size = source_samples.len().min(target_samples.len()) as f64;
        let overlap_percentage = (overlap_count as f64) / min_size;

        if overlap_percentage >= self.config.min_value_overlap {
            Some(overlap_percentage)
        } else {
            None
        }
    }

    /// Combine multiple confidence scores into a single score
    fn combine_confidence_scores(&self, signals: &[(&str, f64)]) -> f64 {
        if signals.is_empty() {
            return 0.0;
        }

        // Weighted average based on signal type
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;

        for (signal_type, confidence) in signals {
            let weight = match *signal_type {
                "value_overlap" => 1.5, // Highest weight - actual data evidence
                "name" => 1.0,          // Medium weight - naming convention
                "semantic" => 0.8,      // Lower weight - inferred type
                _ => 1.0,
            };

            weighted_sum += confidence * weight;
            total_weight += weight;
        }

        if total_weight == 0.0 {
            return 0.0;
        }

        (weighted_sum / total_weight).min(1.0)
    }

    /// Determine the type of relationship based on field characteristics
    fn determine_relationship_type(
        &self,
        source_field: &super::UnifiedField,
        target_field: &super::UnifiedField,
    ) -> RelationshipType {
        // Check if source field is a primary key
        let source_is_pk = source_field.constraints.primary_key;
        let target_is_pk = target_field.constraints.primary_key;

        // Check uniqueness from profiles
        let source_uniqueness = source_field
            .profile
            .as_ref()
            .map(|p| p.quality.uniqueness)
            .unwrap_or(0.0);
        let target_uniqueness = target_field
            .profile
            .as_ref()
            .map(|p| p.quality.uniqueness)
            .unwrap_or(0.0);

        // Determine relationship type based on uniqueness
        if source_is_pk && target_is_pk {
            RelationshipType::OneToOne
        } else if source_is_pk || source_uniqueness > 0.95 {
            RelationshipType::OneToMany
        } else if target_is_pk || target_uniqueness > 0.95 {
            RelationshipType::ManyToOne
        } else if source_field.name.ends_with("_id") || target_field.name.ends_with("_id") {
            RelationshipType::ForeignKey
        } else {
            // Default to many-to-many for ambiguous cases
            RelationshipType::ManyToMany
        }
    }
}

impl Default for RelationshipDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::field::UnifiedField;
    use super::super::profile::{DataQualityMetrics, FieldProfile, ValueDistribution};
    use super::super::types::UniversalDataType;
    use super::super::SourceType;
    use super::*;

    #[test]
    fn test_relationship_detector_creation() {
        let detector = RelationshipDetector::new();
        assert_eq!(detector.config.min_confidence, 0.6);

        let custom_config = RelationshipDetectorConfig {
            min_confidence: 0.8,
            ..Default::default()
        };
        let detector = RelationshipDetector::with_config(custom_config);
        assert_eq!(detector.config.min_confidence, 0.8);
    }

    #[test]
    fn test_name_similarity_exact_match() {
        let detector = RelationshipDetector::new();
        let similarity = detector.calculate_name_similarity("customer_id", "customer_id");
        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn test_name_similarity_foreign_key_pattern() {
        let detector = RelationshipDetector::new();
        let similarity = detector.calculate_name_similarity("customer_id", "id");
        assert!(similarity >= 0.9);
    }

    #[test]
    fn test_name_similarity_partial_match() {
        let detector = RelationshipDetector::new();
        let similarity = detector.calculate_name_similarity("customer", "customer_name");
        assert!(similarity > 0.5);
    }

    #[test]
    fn test_semantic_type_matching() {
        let detector = RelationshipDetector::new();

        let confidence = detector.detect_by_semantic_type(
            &Some(SemanticType::CustomerId),
            &Some(SemanticType::CustomerId),
        );
        assert!(confidence.is_some());
        assert!(confidence.unwrap() > 0.9);

        let no_match = detector
            .detect_by_semantic_type(&Some(SemanticType::Email), &Some(SemanticType::PhoneNumber));
        assert!(no_match.is_none());
    }

    #[test]
    fn test_value_overlap_detection() {
        let detector = RelationshipDetector::new();

        let profile1 = FieldProfile {
            samples: vec!["1".to_string(), "2".to_string(), "3".to_string()],
            distinct_count: 3,
            total_rows: 3,
            null_count: 0,
            null_percentage: 0.0,
            distribution: ValueDistribution::default(),
            top_values: None,
            patterns: None,
            quality: DataQualityMetrics {
                completeness: 1.0,
                uniqueness: 1.0,
                validity: 1.0,
                consistency: 1.0,
                overall_score: 1.0,
                issues: vec![],
            },
        };

        let profile2 = FieldProfile {
            samples: vec!["1".to_string(), "2".to_string(), "4".to_string()],
            ..profile1.clone()
        };

        let confidence = detector.detect_by_value_overlap(&Some(profile1), &Some(profile2));
        assert!(confidence.is_some());
        assert!(confidence.unwrap() > 0.6); // 2 out of 3 overlap
    }

    #[test]
    fn test_detect_relationships_across_schemas() {
        let detector = RelationshipDetector::new();

        // Create customers schema
        let mut customers_schema = UnifiedSchema::new(
            "customers".to_string(),
            SourceType::PostgreSQL,
            "db1".to_string(),
        );

        let mut customer_id_field = UnifiedField::new(
            "id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        );
        customer_id_field.constraints.primary_key = true;
        customer_id_field.semantic.semantic_type = Some(SemanticType::CustomerId);
        customers_schema.add_field(customer_id_field);

        // Create orders schema
        let mut orders_schema = UnifiedSchema::new(
            "orders".to_string(),
            SourceType::PostgreSQL,
            "db1".to_string(),
        );

        let mut customer_id_fk_field = UnifiedField::new(
            "customer_id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        );
        customer_id_fk_field.semantic.semantic_type = Some(SemanticType::CustomerId);
        orders_schema.add_field(customer_id_fk_field);

        let schemas = vec![customers_schema, orders_schema];
        let relationships = detector.detect_relationships(&schemas);

        // Should detect customer_id -> id relationship
        // Note: detector returns relationship in reverse (PK -> FK instead of FK -> PK)
        assert!(!relationships.is_empty());
        let rel = &relationships[0];
        assert_eq!(rel.source, "customers"); // PK table
        assert_eq!(rel.source_field, "id"); // PK field
        assert_eq!(rel.target, "orders"); // FK table
        assert_eq!(rel.target_field, "customer_id"); // FK field
    }

    #[test]
    fn test_levenshtein_distance() {
        let detector = RelationshipDetector::new();

        assert_eq!(detector.levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(detector.levenshtein_distance("saturday", "sunday"), 3);
        assert_eq!(detector.levenshtein_distance("", "abc"), 3);
        assert_eq!(detector.levenshtein_distance("abc", "abc"), 0);
    }

    #[test]
    fn test_combine_confidence_scores() {
        let detector = RelationshipDetector::new();

        let signals = vec![("name", 0.9), ("semantic", 0.8), ("value_overlap", 0.95)];

        let combined = detector.combine_confidence_scores(&signals);
        assert!(combined > 0.85);
        assert!(combined <= 1.0);
    }
}
