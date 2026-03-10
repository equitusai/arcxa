//! Detection-specific types and structures
//!
//! This module defines the core types used by the semantic type detection system,
//! including detection evidence, confidence scoring, and result aggregation.

use crate::inference::types::SemanticType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Detection result from a single strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// Detected semantic type
    pub semantic_type: SemanticType,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Evidence supporting this detection
    pub evidence: Vec<DetectionEvidence>,

    /// Strategy that produced this result
    pub strategy: String,
}

impl DetectionResult {
    /// Create a new detection result
    pub fn new(semantic_type: SemanticType, confidence: f64, strategy: impl Into<String>) -> Self {
        Self {
            semantic_type,
            confidence,
            evidence: Vec::new(),
            strategy: strategy.into(),
        }
    }

    /// Add evidence to this detection
    pub fn with_evidence(mut self, evidence: DetectionEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// Add multiple pieces of evidence
    pub fn with_all_evidence(mut self, evidence: Vec<DetectionEvidence>) -> Self {
        self.evidence.extend(evidence);
        self
    }
}

/// Evidence supporting a detection decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionEvidence {
    /// Type of evidence
    pub evidence_type: EvidenceType,

    /// Human-readable description
    pub description: String,

    /// Weight/importance of this evidence (0.0 - 1.0)
    pub weight: f64,

    /// Sample data supporting this evidence (optional, redacted for PII)
    pub sample: Option<String>,
}

impl DetectionEvidence {
    /// Create new evidence
    pub fn new(evidence_type: EvidenceType, description: impl Into<String>, weight: f64) -> Self {
        Self {
            evidence_type,
            description: description.into(),
            weight,
            sample: None,
        }
    }

    /// Add sample data
    pub fn with_sample(mut self, sample: impl Into<String>) -> Self {
        self.sample = Some(sample.into());
        self
    }
}

/// Type of detection evidence
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EvidenceType {
    /// Column name matches pattern
    ColumnName,

    /// Values match regex pattern
    RegexPattern,

    /// Statistical properties (cardinality, distribution)
    Statistical,

    /// Format consistency across values
    FormatConsistency,

    /// Data type compatibility
    DataType,

    /// Range/domain constraints
    ValueRange,

    /// Correlation with other columns
    ColumnCorrelation,

    /// Known enumeration
    KnownEnum,

    /// Machine learning prediction
    MLPrediction,
}

/// Detection context - information about the column being analyzed
#[derive(Debug, Clone)]
pub struct DetectionContext {
    /// Column name
    pub column_name: String,

    /// SQL data type
    pub data_type: String,

    /// Native database type
    pub native_type: String,

    /// Is column nullable
    pub nullable: bool,

    /// Sample values (limited set for analysis)
    pub sample_values: Vec<String>,

    /// Distinct value count (if available)
    pub distinct_count: Option<u64>,

    /// Total row count (if available)
    pub total_rows: Option<u64>,

    /// Null percentage
    pub null_percentage: f64,

    /// Average value length
    pub avg_length: Option<f64>,

    /// Related column names (for context)
    pub related_columns: Vec<String>,
}

impl DetectionContext {
    /// Create a new detection context
    pub fn new(column_name: impl Into<String>, data_type: impl Into<String>) -> Self {
        Self {
            column_name: column_name.into(),
            data_type: data_type.into(),
            native_type: String::new(),
            nullable: false,
            sample_values: Vec::new(),
            distinct_count: None,
            total_rows: None,
            null_percentage: 0.0,
            avg_length: None,
            related_columns: Vec::new(),
        }
    }

    /// Calculate cardinality if possible
    pub fn cardinality(&self) -> Option<f64> {
        if let (Some(distinct), Some(total)) = (self.distinct_count, self.total_rows) {
            if total > 0 {
                Some(distinct as f64 / total as f64)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if column appears to be low cardinality (categorical)
    pub fn is_low_cardinality(&self) -> bool {
        self.cardinality().map(|c| c < 0.01).unwrap_or(false)
            || self.distinct_count.map(|d| d < 100).unwrap_or(false)
    }

    /// Check if column appears to be high cardinality (unique identifier)
    pub fn is_high_cardinality(&self) -> bool {
        self.cardinality().map(|c| c >= 0.95).unwrap_or(false)
    }
}

/// Aggregated detection results from multiple strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedDetection {
    /// Most likely semantic type
    pub semantic_type: SemanticType,

    /// Overall confidence (weighted average)
    pub confidence: f64,

    /// All candidate detections
    pub candidates: Vec<DetectionResult>,

    /// Aggregation method used
    pub method: AggregationMethod,
}

/// Method for aggregating multiple detection results
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AggregationMethod {
    /// Highest confidence wins
    MaxConfidence,

    /// Weighted average by strategy importance
    WeightedAverage,

    /// Bayesian combination of evidence
    Bayesian,

    /// Voting with confidence weighting
    WeightedVoting,
}

/// Detection rule definition
#[derive(Debug, Clone)]
pub struct DetectionRule {
    /// Semantic type this rule detects
    pub semantic_type: SemanticType,

    /// Column name patterns (regex)
    pub name_patterns: Vec<String>,

    /// Value patterns (regex)
    pub value_patterns: Vec<String>,

    /// Required SQL data types
    pub required_types: Vec<String>,

    /// Statistical constraints
    pub statistical_constraints: Option<StatisticalConstraints>,

    /// Base confidence score for matches
    pub base_confidence: f64,
}

/// Statistical constraints for detection
#[derive(Debug, Clone)]
pub struct StatisticalConstraints {
    /// Minimum percentage of values that must match pattern
    pub min_match_percentage: f64,

    /// Maximum distinct value count (for enums)
    pub max_distinct_count: Option<u64>,

    /// Minimum distinct value count
    pub min_distinct_count: Option<u64>,

    /// Required cardinality range
    pub cardinality_range: Option<(f64, f64)>,

    /// Average length constraints
    pub avg_length_range: Option<(f64, f64)>,
}

/// Detection statistics for monitoring and debugging
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectionStatistics {
    /// Total columns analyzed
    pub columns_analyzed: u64,

    /// Successful detections
    pub detections_made: u64,

    /// Detection counts by type
    pub detections_by_type: HashMap<String, u64>,

    /// Average confidence score
    pub avg_confidence: f64,

    /// Detection time (milliseconds)
    pub total_time_ms: u64,
}

impl DetectionStatistics {
    /// Record a detection
    pub fn record_detection(&mut self, result: &AggregatedDetection, time_ms: u64) {
        self.columns_analyzed += 1;
        self.detections_made += 1;

        let type_name = format!("{:?}", result.semantic_type);
        *self.detections_by_type.entry(type_name).or_insert(0) += 1;

        // Update running average
        let n = self.detections_made as f64;
        self.avg_confidence = ((n - 1.0) * self.avg_confidence + result.confidence) / n;

        self.total_time_ms += time_ms;
    }

    /// Record a failed detection
    pub fn record_no_detection(&mut self) {
        self.columns_analyzed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_context_cardinality() {
        let mut ctx = DetectionContext::new("user_id", "integer");
        ctx.distinct_count = Some(950);
        ctx.total_rows = Some(1000);

        assert_eq!(ctx.cardinality(), Some(0.95));
        assert!(ctx.is_high_cardinality());
        assert!(!ctx.is_low_cardinality());
    }

    #[test]
    fn test_detection_context_low_cardinality() {
        let mut ctx = DetectionContext::new("status", "varchar");
        ctx.distinct_count = Some(5);
        ctx.total_rows = Some(10000);

        assert!(ctx.is_low_cardinality());
        assert!(!ctx.is_high_cardinality());
    }

    #[test]
    fn test_detection_result_with_evidence() {
        let result = DetectionResult::new(SemanticType::Email, 0.95, "regex").with_evidence(
            DetectionEvidence::new(EvidenceType::RegexPattern, "Matched email pattern", 0.9),
        );

        assert_eq!(result.semantic_type, SemanticType::Email);
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.evidence.len(), 1);
    }

    #[test]
    fn test_detection_statistics() {
        let mut stats = DetectionStatistics::default();

        let result = AggregatedDetection {
            semantic_type: SemanticType::Email,
            confidence: 0.9,
            candidates: vec![],
            method: AggregationMethod::MaxConfidence,
        };

        stats.record_detection(&result, 100);
        stats.record_no_detection();

        assert_eq!(stats.columns_analyzed, 2);
        assert_eq!(stats.detections_made, 1);
        assert_eq!(stats.avg_confidence, 0.9);
        assert_eq!(stats.total_time_ms, 100);
    }
}
