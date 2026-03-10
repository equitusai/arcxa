//! Field Profiling Types
//!
//! Statistical and analytical profile information for fields.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistical profile of a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldProfile {
    /// Number of distinct values
    pub distinct_count: u64,

    /// Total number of rows
    pub total_rows: u64,

    /// Number of null values
    pub null_count: u64,

    /// Percentage of null values (0.0 - 1.0)
    pub null_percentage: f64,

    /// Value distribution statistics
    pub distribution: ValueDistribution,

    /// Sample values for analysis
    pub samples: Vec<String>,

    /// Frequency distribution of top values
    pub top_values: Option<Vec<ValueFrequency>>,

    /// Pattern analysis results
    pub patterns: Option<Vec<PatternInfo>>,

    /// Data quality metrics
    pub quality: DataQualityMetrics,
}

/// Value distribution statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValueDistribution {
    pub min: Option<String>,
    pub max: Option<String>,
    pub mean: Option<f64>,
    pub median: Option<String>,
    pub mode: Option<String>,
    pub stddev: Option<f64>,
    pub variance: Option<f64>,

    // Percentiles
    pub p01: Option<String>,
    pub p05: Option<String>,
    pub p25: Option<String>,
    pub p50: Option<String>, // Same as median
    pub p75: Option<String>,
    pub p95: Option<String>,
    pub p99: Option<String>,

    // Additional statistics
    pub sum: Option<f64>,
    pub skewness: Option<f64>,
    pub kurtosis: Option<f64>,
}

/// Frequency of a value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueFrequency {
    pub value: String,
    pub count: u64,
    pub percentage: f64,
}

/// Pattern information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternInfo {
    /// Regular expression pattern
    pub pattern: String,

    /// Number of values matching this pattern
    pub match_count: u64,

    /// Percentage of values matching
    pub match_percentage: f64,

    /// Example values matching this pattern
    pub examples: Vec<String>,
}

/// Data quality metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataQualityMetrics {
    /// Completeness score (0.0 - 1.0)
    pub completeness: f64,

    /// Uniqueness score (0.0 - 1.0)
    pub uniqueness: f64,

    /// Validity score (0.0 - 1.0)
    pub validity: f64,

    /// Consistency score (0.0 - 1.0)
    pub consistency: f64,

    /// Overall quality score (0.0 - 1.0)
    pub overall_score: f64,

    /// Quality issues detected
    pub issues: Vec<QualityIssue>,
}

/// Quality issue detected in data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub issue_type: QualityIssueType,
    pub severity: IssueSeverity,
    pub description: String,
    pub affected_count: u64,
    pub examples: Vec<String>,
}

/// Types of quality issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityIssueType {
    MissingValues,
    InvalidFormat,
    OutOfRange,
    Duplicate,
    InconsistentCase,
    LeadingTrailingSpaces,
    InvalidDate,
    InvalidEmail,
    InvalidPhone,
    SuspiciousPattern,
    Other(String),
}

/// Severity levels for issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl FieldProfile {
    /// Create a new empty field profile
    pub fn new() -> Self {
        Self {
            distinct_count: 0,
            total_rows: 0,
            null_count: 0,
            null_percentage: 0.0,
            distribution: ValueDistribution::default(),
            samples: Vec::new(),
            top_values: None,
            patterns: None,
            quality: DataQualityMetrics::default(),
        }
    }

    /// Calculate cardinality ratio (distinct values / total rows)
    pub fn cardinality_ratio(&self) -> f64 {
        if self.total_rows == 0 {
            0.0
        } else {
            self.distinct_count as f64 / self.total_rows as f64
        }
    }

    /// Check if field is likely a unique identifier
    pub fn is_likely_unique(&self) -> bool {
        self.cardinality_ratio() >= 0.95 && self.null_percentage < 0.1
    }

    /// Check if field is likely categorical
    pub fn is_likely_categorical(&self) -> bool {
        self.distinct_count < 100 && self.cardinality_ratio() < 0.1
    }

    /// Check if field has high null percentage
    pub fn has_high_nulls(&self) -> bool {
        self.null_percentage > 0.5
    }

    /// Get completeness score
    pub fn completeness_score(&self) -> f64 {
        1.0 - self.null_percentage
    }

    /// Get uniqueness score
    pub fn uniqueness_score(&self) -> f64 {
        if self.total_rows == 0 {
            0.0
        } else {
            self.distinct_count as f64 / (self.total_rows - self.null_count) as f64
        }
    }
}

impl Default for FieldProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Profile comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileComparison {
    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,

    /// Individual metric comparisons
    pub metrics: HashMap<String, MetricComparison>,

    /// Detected changes
    pub changes: Vec<ProfileChange>,
}

/// Comparison of a single metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricComparison {
    pub metric_name: String,
    pub source_value: f64,
    pub target_value: f64,
    pub difference: f64,
    pub percentage_change: f64,
}

/// Detected change in profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileChange {
    pub change_type: ProfileChangeType,
    pub description: String,
    pub impact: ChangeImpact,
}

/// Types of profile changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfileChangeType {
    CardinalityChange,
    NullPercentageChange,
    DistributionShift,
    NewValues,
    MissingValues,
    PatternChange,
    QualityDegradation,
}

/// Impact level of changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeImpact {
    Low,
    Medium,
    High,
    Critical,
}

impl FieldProfile {
    /// Compare this profile with another
    pub fn compare_with(&self, other: &FieldProfile) -> ProfileComparison {
        let mut metrics = HashMap::new();
        let mut changes = Vec::new();

        // Compare cardinality
        let cardinality_diff = (self.cardinality_ratio() - other.cardinality_ratio()).abs();
        metrics.insert(
            "cardinality".to_string(),
            MetricComparison {
                metric_name: "Cardinality Ratio".to_string(),
                source_value: self.cardinality_ratio(),
                target_value: other.cardinality_ratio(),
                difference: cardinality_diff,
                percentage_change: if self.cardinality_ratio() > 0.0 {
                    (cardinality_diff / self.cardinality_ratio()) * 100.0
                } else {
                    0.0
                },
            },
        );

        if cardinality_diff > 0.1 {
            changes.push(ProfileChange {
                change_type: ProfileChangeType::CardinalityChange,
                description: format!(
                    "Cardinality changed from {:.2}% to {:.2}%",
                    self.cardinality_ratio() * 100.0,
                    other.cardinality_ratio() * 100.0
                ),
                impact: if cardinality_diff > 0.5 {
                    ChangeImpact::High
                } else {
                    ChangeImpact::Medium
                },
            });
        }

        // Compare null percentage
        let null_diff = (self.null_percentage - other.null_percentage).abs();
        metrics.insert(
            "nulls".to_string(),
            MetricComparison {
                metric_name: "Null Percentage".to_string(),
                source_value: self.null_percentage,
                target_value: other.null_percentage,
                difference: null_diff,
                percentage_change: if self.null_percentage > 0.0 {
                    (null_diff / self.null_percentage) * 100.0
                } else {
                    0.0
                },
            },
        );

        // Calculate overall similarity
        let similarity = 1.0 - (cardinality_diff + null_diff) / 2.0;

        ProfileComparison {
            similarity: similarity.max(0.0).min(1.0),
            metrics,
            changes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_creation() {
        let mut profile = FieldProfile::new();
        profile.distinct_count = 95;
        profile.total_rows = 100;
        profile.null_count = 5;
        profile.null_percentage = 0.05;

        assert_eq!(profile.cardinality_ratio(), 0.95);
        assert!(profile.is_likely_unique());
        assert!(!profile.is_likely_categorical());
        assert!(!profile.has_high_nulls());
        assert_eq!(profile.completeness_score(), 0.95);
    }

    #[test]
    fn test_categorical_detection() {
        let mut profile = FieldProfile::new();
        profile.distinct_count = 5;
        profile.total_rows = 1000;
        profile.null_count = 0;

        assert!(!profile.is_likely_unique());
        assert!(profile.is_likely_categorical());
    }

    #[test]
    fn test_profile_comparison() {
        let mut profile1 = FieldProfile::new();
        profile1.distinct_count = 100;
        profile1.total_rows = 100;
        profile1.null_percentage = 0.0;

        let mut profile2 = FieldProfile::new();
        profile2.distinct_count = 95;
        profile2.total_rows = 100;
        profile2.null_percentage = 0.05;

        let comparison = profile1.compare_with(&profile2);
        assert!(comparison.similarity > 0.9);
        assert_eq!(comparison.metrics.len(), 2);
        assert!(comparison.changes.len() <= 1);
    }
}
