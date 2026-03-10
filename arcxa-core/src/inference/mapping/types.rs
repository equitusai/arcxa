// graphica-core/src/inference/mapping/types.rs
//! Core types for field mapping and similarity analysis

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Field comparison result with multi-dimensional similarity scores
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldSimilarity {
    /// Source field metadata
    pub source: FieldMetadata,

    /// Target field metadata
    pub target: FieldMetadata,

    /// Multi-dimensional similarity scores
    pub scores: SimilarityScores,

    /// Overall confidence (weighted aggregate, 0.0 - 1.0)
    pub confidence: f64,

    /// Suggested relationship type
    pub relationship_type: RelationshipType,

    /// Evidence supporting this mapping
    pub evidence: Vec<MappingEvidence>,
}

/// Multi-dimensional similarity scores (all values 0.0 - 1.0)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SimilarityScores {
    /// Lexical similarity (string distance algorithms)
    pub lexical: f64,

    /// Semantic similarity (NLP embeddings) - Phase 2
    pub semantic: Option<f64>,

    /// Statistical similarity (value distribution comparison)
    pub statistical: f64,

    /// Schema context similarity (position, neighbors)
    pub schema_context: f64,

    /// Domain knowledge match - Phase 5
    pub domain_knowledge: Option<f64>,

    /// ML classifier prediction - Phase 4
    pub ml_prediction: Option<f64>,
}

/// Type of relationship between two fields
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum RelationshipType {
    /// Primary key → Foreign key join
    PrimaryForeignKey {
        direction: JoinDirection,
        cardinality: Cardinality,
    },

    /// Same field in both datasets (duplicate)
    Duplicate,

    /// Derived field (one computed from the other)
    Derived { formula: Option<String> },

    /// Correlated fields (statistical relationship)
    Correlated { correlation_coefficient: f64 },

    /// No relationship detected
    Unrelated,
}

/// Direction of join relationship
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum JoinDirection {
    /// source → target (source has FK to target PK)
    LeftToRight,

    /// target → source (target has FK to source PK)
    RightToLeft,

    /// Bidirectional (many-to-many)
    Bidirectional,
}

/// Cardinality of relationship
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

/// Complete metadata for a field
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldMetadata {
    /// Fully qualified name (dataset.table.column or just column)
    pub qualified_name: String,

    /// Short column name
    pub column_name: String,

    /// Dataset/source identifier
    pub source_id: String,

    /// Data type (Integer, String, Date, etc.)
    pub data_type: DataType,

    /// Statistical profile
    pub profile: FieldProfile,

    /// Semantic type (if detected)
    pub semantic_type: Option<String>,

    /// Position in schema (0-indexed)
    pub position: usize,

    /// Neighboring column names (for context analysis)
    pub neighbors: Vec<String>,
}

/// Data type for a field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum DataType {
    Integer,
    Float,
    String,
    Boolean,
    Date,
    DateTime,
    Time,
    Decimal { precision: u32, scale: u32 },
    Binary,
    Json,
    Unknown,
}

/// Statistical profile of a field
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldProfile {
    /// Number of distinct values
    pub distinct_count: u64,

    /// Total number of rows
    pub total_rows: u64,

    /// Percentage of null values (0.0 - 1.0)
    pub null_percentage: f64,

    /// Value distribution (min, max, percentiles)
    pub distribution: ValueDistribution,

    /// Sample values for analysis (up to 100)
    pub samples: Vec<String>,
}

/// Value distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ValueDistribution {
    pub min: Option<String>,
    pub max: Option<String>,
    pub median: Option<String>,
    pub p25: Option<String>,
    pub p75: Option<String>,
    pub p95: Option<String>,
    pub p99: Option<String>,
}

/// Evidence supporting a mapping suggestion
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingEvidence {
    /// Type of evidence
    pub evidence_type: EvidenceType,

    /// Score for this evidence (0.0 - 1.0)
    pub score: f64,

    /// Human-readable description
    pub description: String,
}

/// Type of evidence for a mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum EvidenceType {
    /// Lexical similarity (string matching)
    Lexical,

    /// Semantic similarity (NLP)
    Semantic,

    /// Statistical similarity (distributions)
    Statistical,

    /// Schema context (position, neighbors)
    SchemaContext,

    /// Domain knowledge (ontology)
    DomainKnowledge,

    /// ML classifier prediction
    MLPrediction,
}

/// Complete field mapping with multiple candidates
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldMapping {
    /// Source field
    pub source_field: FieldMetadata,

    /// Candidate target fields sorted by confidence (descending)
    pub candidates: Vec<FieldSimilarity>,
}

/// Dataset schema for mapping
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetSchema {
    /// Dataset identifier
    pub dataset_id: String,

    /// Dataset name
    pub dataset_name: String,

    /// Fields in this dataset
    pub fields: Vec<FieldMetadata>,
}

/// Configuration for the field mapper
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MapperConfig {
    /// Minimum confidence for suggestions (default: 0.5)
    pub min_confidence: f64,

    /// Auto-map threshold for high confidence (default: 0.9)
    pub auto_map_threshold: f64,

    /// Recommend threshold for medium confidence (default: 0.7)
    pub recommend_threshold: f64,

    /// Score weights for aggregation
    pub score_weights: ScoreWeights,
}

/// Weights for aggregating similarity scores
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScoreWeights {
    /// Weight for lexical similarity (default: 0.20)
    pub lexical: f64,

    /// Weight for semantic similarity (default: 0.25, Phase 2)
    pub semantic: f64,

    /// Weight for statistical similarity (default: 0.35)
    pub statistical: f64,

    /// Weight for schema context (default: 0.10)
    pub schema_context: f64,

    /// Weight for domain knowledge (default: 0.10, Phase 5)
    pub domain_knowledge: f64,

    /// Weight for ML prediction (default: 0.00, Phase 4)
    pub ml_prediction: f64,
}

impl Default for MapperConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            auto_map_threshold: 0.9,
            recommend_threshold: 0.7,
            score_weights: ScoreWeights::default(),
        }
    }
}

impl Default for ScoreWeights {
    fn default() -> Self {
        // Phase 1: Only lexical, statistical, and schema context are implemented
        Self {
            lexical: 0.30,          // Increased from 0.20 for Phase 1
            semantic: 0.00,         // Phase 2 - not implemented yet
            statistical: 0.50,      // Increased from 0.35 for Phase 1
            schema_context: 0.20,   // Increased from 0.10 for Phase 1
            domain_knowledge: 0.00, // Phase 5 - not implemented yet
            ml_prediction: 0.00,    // Phase 4 - not implemented yet
        }
    }
}

/// Mapping suggestions response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingSuggestions {
    /// All potential joins found
    pub joins: Vec<FieldSimilarity>,

    /// High confidence mappings (≥ auto_map_threshold)
    pub auto_mapped: Vec<FieldSimilarity>,

    /// Medium confidence mappings (≥ recommend_threshold, < auto_map_threshold)
    pub recommended: Vec<FieldSimilarity>,

    /// Low confidence mappings (≥ min_confidence, < recommend_threshold)
    pub possible: Vec<FieldSimilarity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MapperConfig::default();
        assert_eq!(config.min_confidence, 0.5);
        assert_eq!(config.auto_map_threshold, 0.9);
        assert_eq!(config.recommend_threshold, 0.7);

        // Phase 1 weights should sum to 1.0
        let weights = &config.score_weights;
        let total = weights.lexical + weights.statistical + weights.schema_context;
        assert!((total - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_relationship_type_equality() {
        let rel1 = RelationshipType::PrimaryForeignKey {
            direction: JoinDirection::LeftToRight,
            cardinality: Cardinality::ManyToOne,
        };

        let rel2 = RelationshipType::PrimaryForeignKey {
            direction: JoinDirection::LeftToRight,
            cardinality: Cardinality::ManyToOne,
        };

        assert_eq!(rel1, rel2);
    }
}
