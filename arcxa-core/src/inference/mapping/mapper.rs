// graphica-core/src/inference/mapping/mapper.rs
//! Main field mapping engine with multi-dimensional similarity scoring

use crate::inference::mapping::{
    lexical::LexicalSimilarity, statistical::StatisticalSimilarity, types::*,
    vocabulary::DomainVocabulary,
};
use anyhow::Result;
use tracing::{debug, info};

/// Main field mapping engine
#[derive(Debug, Clone)]
pub struct FieldMapper {
    /// Lexical similarity calculator
    lexical: LexicalSimilarity,

    /// Statistical similarity calculator
    statistical: StatisticalSimilarity,

    /// Domain vocabulary for alias recognition
    vocabulary: DomainVocabulary,

    /// Configuration
    config: MapperConfig,
}

impl FieldMapper {
    /// Create a new field mapper with default configuration
    pub fn new() -> Self {
        Self::with_config(MapperConfig::default())
    }

    /// Create a new field mapper with custom configuration
    pub fn with_config(config: MapperConfig) -> Self {
        Self {
            lexical: LexicalSimilarity::new(),
            statistical: StatisticalSimilarity::new(),
            vocabulary: DomainVocabulary::with_defaults(),
            config,
        }
    }

    /// Create a field mapper with custom vocabulary
    pub fn with_vocabulary(config: MapperConfig, vocabulary: DomainVocabulary) -> Self {
        Self {
            lexical: LexicalSimilarity::new(),
            statistical: StatisticalSimilarity::new(),
            vocabulary,
            config,
        }
    }

    /// Find all potential mappings between two datasets
    pub fn find_mappings(
        &self,
        source: &DatasetSchema,
        target: &DatasetSchema,
    ) -> Result<Vec<FieldMapping>> {
        info!(
            "Finding mappings between {} ({} fields) and {} ({} fields)",
            source.dataset_name,
            source.fields.len(),
            target.dataset_name,
            target.fields.len()
        );

        let mut mappings = Vec::new();

        // For each field in source
        for source_field in &source.fields {
            let mut candidates = Vec::new();

            // Compare against all target fields
            for target_field in &target.fields {
                // Skip if data types are incompatible
                if !self
                    .statistical
                    .data_type_compatible(&source_field.data_type, &target_field.data_type)
                {
                    debug!(
                        "Skipping {} → {} (incompatible types: {:?} vs {:?})",
                        source_field.column_name,
                        target_field.column_name,
                        source_field.data_type,
                        target_field.data_type
                    );
                    continue;
                }

                let similarity = self.calculate_similarity(source_field, target_field)?;

                // Only include if confidence above threshold
                if similarity.confidence >= self.config.min_confidence {
                    debug!(
                        "Found candidate: {} → {} (confidence: {:.3})",
                        source_field.column_name, target_field.column_name, similarity.confidence
                    );
                    candidates.push(similarity);
                }
            }

            // Sort by confidence (descending)
            candidates.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Only add if we found candidates
            if !candidates.is_empty() {
                info!(
                    "Found {} candidate(s) for {}",
                    candidates.len(),
                    source_field.column_name
                );
                mappings.push(FieldMapping {
                    source_field: source_field.clone(),
                    candidates,
                });
            }
        }

        info!("Total field mappings found: {}", mappings.len());
        Ok(mappings)
    }

    /// Calculate multi-dimensional similarity between two fields
    fn calculate_similarity(
        &self,
        source: &FieldMetadata,
        target: &FieldMetadata,
    ) -> Result<FieldSimilarity> {
        // 1. Lexical similarity (string matching)
        let mut lexical_score = self
            .lexical
            .compare(&source.column_name, &target.column_name);

        // Boost lexical score if domain vocabulary recognizes aliases
        if let Some(vocab_score) = self
            .vocabulary
            .alias_match(&source.column_name, &target.column_name)
        {
            debug!(
                "Domain vocabulary match: {} ↔ {} (score: {:.2})",
                source.column_name, target.column_name, vocab_score
            );
            // Use the higher of lexical or vocabulary score
            lexical_score = lexical_score.max(vocab_score);
        }

        // 2. Statistical similarity (value distribution)
        let statistical_score = self.statistical.compare(&source.profile, &target.profile)?;

        // 3. Schema context similarity (position and neighbors)
        let schema_score = self.calculate_schema_context(source, target)?;

        // Build scores struct (Phase 1: semantic, domain, ML not implemented yet)
        let scores = SimilarityScores {
            lexical: lexical_score,
            semantic: None,
            statistical: statistical_score,
            schema_context: schema_score,
            domain_knowledge: None,
            ml_prediction: None,
        };

        // Aggregate into overall confidence
        let confidence = self.aggregate_confidence(&scores);

        // Infer relationship type
        let relationship_type = self.infer_relationship(source, target, &scores)?;

        // Build evidence
        let evidence = self.build_evidence(&scores, source, target);

        Ok(FieldSimilarity {
            source: source.clone(),
            target: target.clone(),
            scores,
            confidence,
            relationship_type,
            evidence,
        })
    }

    /// Calculate schema context similarity (position and neighbors)
    fn calculate_schema_context(
        &self,
        source: &FieldMetadata,
        target: &FieldMetadata,
    ) -> Result<f64> {
        let mut context_scores = Vec::new();

        // 1. Position similarity (normalized difference)
        let max_position = source.position.max(target.position) as f64;
        let position_score = if max_position > 0.0 {
            1.0 - ((source.position as f64 - target.position as f64).abs() / max_position)
        } else {
            1.0 // Both at position 0
        };
        context_scores.push(position_score * 0.3);

        // 2. Neighbor name similarity
        let neighbor_score = self.neighbor_similarity(&source.neighbors, &target.neighbors);
        context_scores.push(neighbor_score * 0.7);

        Ok(context_scores.iter().sum())
    }

    /// Calculate similarity between neighboring fields
    fn neighbor_similarity(&self, source_neighbors: &[String], target_neighbors: &[String]) -> f64 {
        if source_neighbors.is_empty() || target_neighbors.is_empty() {
            return 0.5; // Neutral if no neighbor info
        }

        let mut total_similarity = 0.0;
        let mut count = 0;

        // Compare each source neighbor with each target neighbor
        for s_neighbor in source_neighbors {
            for t_neighbor in target_neighbors {
                total_similarity += self.lexical.compare(s_neighbor, t_neighbor);
                count += 1;
            }
        }

        if count > 0 {
            total_similarity / count as f64
        } else {
            0.5
        }
    }

    /// Aggregate individual scores into overall confidence
    fn aggregate_confidence(&self, scores: &SimilarityScores) -> f64 {
        let weights = &self.config.score_weights;

        // Only use weights for implemented features (Phase 1)
        let total_weight = weights.lexical + weights.statistical + weights.schema_context;

        let weighted_sum = scores.lexical * weights.lexical
            + scores.statistical * weights.statistical
            + scores.schema_context * weights.schema_context;

        weighted_sum / total_weight
    }

    /// Infer relationship type between fields
    fn infer_relationship(
        &self,
        source: &FieldMetadata,
        target: &FieldMetadata,
        scores: &SimilarityScores,
    ) -> Result<RelationshipType> {
        // High lexical similarity suggests duplicate/same field
        if scores.lexical > 0.95 {
            return Ok(RelationshipType::Duplicate);
        }

        // Estimate cardinality from statistical profiles
        let cardinality_est = self
            .statistical
            .estimate_cardinality_relationship(&source.profile, &target.profile);

        // Convert to our enum
        use crate::inference::mapping::statistical::CardinalityEstimate;
        let cardinality = match cardinality_est {
            CardinalityEstimate::OneToOne => Cardinality::OneToOne,
            CardinalityEstimate::OneToMany => Cardinality::OneToMany,
            CardinalityEstimate::ManyToOne => Cardinality::ManyToOne,
            CardinalityEstimate::ManyToMany => Cardinality::ManyToMany,
        };

        // Determine join direction based on cardinality
        let direction = match cardinality {
            Cardinality::OneToOne => JoinDirection::Bidirectional,
            Cardinality::OneToMany => JoinDirection::LeftToRight,
            Cardinality::ManyToOne => JoinDirection::RightToLeft,
            Cardinality::ManyToMany => JoinDirection::Bidirectional,
        };

        Ok(RelationshipType::PrimaryForeignKey {
            direction,
            cardinality,
        })
    }

    /// Build evidence list from scores
    fn build_evidence(
        &self,
        scores: &SimilarityScores,
        source: &FieldMetadata,
        target: &FieldMetadata,
    ) -> Vec<MappingEvidence> {
        let mut evidence = Vec::new();

        // Lexical evidence
        evidence.push(MappingEvidence {
            evidence_type: EvidenceType::Lexical,
            score: scores.lexical,
            description: format!(
                "Column names '{}' and '{}' have {:.1}% lexical similarity",
                source.column_name,
                target.column_name,
                scores.lexical * 100.0
            ),
        });

        // Statistical evidence
        evidence.push(MappingEvidence {
            evidence_type: EvidenceType::Statistical,
            score: scores.statistical,
            description: format!(
                "Value distributions have {:.1}% statistical similarity (cardinality: {}/{} vs {}/{})",
                scores.statistical * 100.0,
                source.profile.distinct_count,
                source.profile.total_rows,
                target.profile.distinct_count,
                target.profile.total_rows
            ),
        });

        // Schema context evidence
        evidence.push(MappingEvidence {
            evidence_type: EvidenceType::SchemaContext,
            score: scores.schema_context,
            description: format!(
                "Schema position and context have {:.1}% similarity (positions: {} vs {})",
                scores.schema_context * 100.0,
                source.position,
                target.position
            ),
        });

        evidence
    }

    /// Categorize mappings into confidence tiers
    pub fn categorize_mappings(&self, similarities: Vec<FieldSimilarity>) -> MappingSuggestions {
        let mut auto_mapped = Vec::new();
        let mut recommended = Vec::new();
        let mut possible = Vec::new();

        for sim in &similarities {
            if sim.confidence >= self.config.auto_map_threshold {
                auto_mapped.push(sim.clone());
            } else if sim.confidence >= self.config.recommend_threshold {
                recommended.push(sim.clone());
            } else if sim.confidence >= self.config.min_confidence {
                possible.push(sim.clone());
            }
        }

        MappingSuggestions {
            joins: similarities,
            auto_mapped,
            recommended,
            possible,
        }
    }
}

impl Default for FieldMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_field(
        name: &str,
        source_id: &str,
        distinct: u64,
        total: u64,
        position: usize,
        neighbors: Vec<String>,
    ) -> FieldMetadata {
        FieldMetadata {
            qualified_name: format!("{}.{}", source_id, name),
            column_name: name.to_string(),
            source_id: source_id.to_string(),
            data_type: DataType::Integer,
            profile: FieldProfile {
                distinct_count: distinct,
                total_rows: total,
                null_percentage: 0.0,
                distribution: ValueDistribution {
                    min: Some("1".to_string()),
                    max: Some(total.to_string()),
                    ..Default::default()
                },
                samples: vec![],
            },
            semantic_type: None,
            position,
            neighbors,
        }
    }

    #[test]
    fn test_exact_field_match() {
        let mapper = FieldMapper::new();

        let source = create_test_field("customer_id", "customers", 10000, 10000, 0, vec![]);
        let target = create_test_field("customer_id", "orders", 8500, 10000, 1, vec![]);

        let similarity = mapper.calculate_similarity(&source, &target).unwrap();

        // Should have very high confidence
        assert!(
            similarity.confidence > 0.8,
            "Confidence was {}",
            similarity.confidence
        );
        assert!(similarity.scores.lexical > 0.99); // Exact name match
    }

    #[test]
    fn test_abbreviated_match() {
        let mapper = FieldMapper::new();

        let source = create_test_field("customer_id", "customers", 10000, 10000, 0, vec![]);
        let target = create_test_field("cust_id", "orders", 8500, 10000, 1, vec![]);

        let similarity = mapper.calculate_similarity(&source, &target).unwrap();

        // Should have decent confidence
        assert!(
            similarity.confidence > 0.5,
            "Confidence was {}",
            similarity.confidence
        );
        assert!(similarity.scores.lexical > 0.5); // Some lexical similarity
        assert!(similarity.scores.statistical > 0.8); // Good statistical match
    }

    #[test]
    fn test_incompatible_types_skipped() {
        let mapper = FieldMapper::new();

        let mut source = create_test_field("customer_id", "customers", 10000, 10000, 0, vec![]);
        source.data_type = DataType::Integer;

        let mut target = create_test_field("customer_name", "orders", 8500, 10000, 1, vec![]);
        target.data_type = DataType::String;

        // Should return no mappings due to type incompatibility
        let source_schema = DatasetSchema {
            dataset_id: "customers".to_string(),
            dataset_name: "Customers".to_string(),
            fields: vec![source],
        };

        let target_schema = DatasetSchema {
            dataset_id: "orders".to_string(),
            dataset_name: "Orders".to_string(),
            fields: vec![target],
        };

        let mappings = mapper
            .find_mappings(&source_schema, &target_schema)
            .unwrap();
        assert!(
            mappings.is_empty(),
            "Should have no mappings for incompatible types"
        );
    }

    #[test]
    fn test_relationship_type_inference() {
        let mapper = FieldMapper::new();

        // One-to-Many: source is unique (PK), target is not (FK)
        let source = create_test_field("customer_id", "customers", 10000, 10000, 0, vec![]);
        let target = create_test_field("cust_id", "orders", 5000, 10000, 1, vec![]);

        let similarity = mapper.calculate_similarity(&source, &target).unwrap();

        match similarity.relationship_type {
            RelationshipType::PrimaryForeignKey {
                direction,
                cardinality,
            } => {
                assert_eq!(cardinality, Cardinality::OneToMany);
            }
            _ => panic!("Expected PrimaryForeignKey relationship"),
        }
    }

    #[test]
    fn test_categorize_mappings() {
        let mapper = FieldMapper::new();

        let source = create_test_field("id", "table1", 100, 100, 0, vec![]);

        let similarities = vec![
            FieldSimilarity {
                source: source.clone(),
                target: create_test_field("id", "table2", 100, 100, 0, vec![]),
                scores: SimilarityScores {
                    lexical: 1.0,
                    semantic: None,
                    statistical: 1.0,
                    schema_context: 1.0,
                    domain_knowledge: None,
                    ml_prediction: None,
                },
                confidence: 0.95, // High confidence
                relationship_type: RelationshipType::Duplicate,
                evidence: vec![],
            },
            FieldSimilarity {
                source: source.clone(),
                target: create_test_field("identifier", "table3", 100, 100, 0, vec![]),
                scores: SimilarityScores {
                    lexical: 0.6,
                    semantic: None,
                    statistical: 0.9,
                    schema_context: 0.7,
                    domain_knowledge: None,
                    ml_prediction: None,
                },
                confidence: 0.75, // Medium confidence
                relationship_type: RelationshipType::Duplicate,
                evidence: vec![],
            },
            FieldSimilarity {
                source: source.clone(),
                target: create_test_field("key", "table4", 100, 100, 0, vec![]),
                scores: SimilarityScores {
                    lexical: 0.3,
                    semantic: None,
                    statistical: 0.8,
                    schema_context: 0.6,
                    domain_knowledge: None,
                    ml_prediction: None,
                },
                confidence: 0.55, // Low confidence
                relationship_type: RelationshipType::Duplicate,
                evidence: vec![],
            },
        ];

        let suggestions = mapper.categorize_mappings(similarities);

        assert_eq!(
            suggestions.auto_mapped.len(),
            1,
            "Should have 1 auto-mapped"
        );
        assert_eq!(
            suggestions.recommended.len(),
            1,
            "Should have 1 recommended"
        );
        assert_eq!(suggestions.possible.len(), 1, "Should have 1 possible");
    }
}
