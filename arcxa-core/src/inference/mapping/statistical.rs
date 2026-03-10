// graphica-core/src/inference/mapping/statistical.rs
//! Statistical similarity for comparing field profiles

use crate::inference::mapping::types::{DataType, FieldProfile, ValueDistribution};
use anyhow::Result;

/// Statistical similarity calculator
#[derive(Debug, Clone)]
pub struct StatisticalSimilarity;

impl StatisticalSimilarity {
    pub fn new() -> Self {
        Self
    }

    /// Compare two field profiles statistically
    /// Returns a score from 0.0 (no match) to 1.0 (perfect match)
    pub fn compare(&self, source: &FieldProfile, target: &FieldProfile) -> Result<f64> {
        let mut scores = Vec::new();

        // 1. Cardinality similarity (distinct count ratio)
        let cardinality_score = self.cardinality_similarity(source, target);
        scores.push((cardinality_score, 0.40)); // Weight: 0.40

        // 2. Null percentage similarity
        let null_score = self.null_percentage_similarity(source, target);
        scores.push((null_score, 0.20)); // Weight: 0.20

        // 3. Value range overlap (for numeric/string data)
        let range_score = self.value_range_overlap(&source.distribution, &target.distribution);
        scores.push((range_score, 0.40)); // Weight: 0.40

        // Weighted average
        let total_weight: f64 = scores.iter().map(|(_, w)| w).sum();
        let weighted_sum: f64 = scores.iter().map(|(s, w)| s * w).sum();

        Ok(weighted_sum / total_weight)
    }

    /// Compare data types for compatibility
    pub fn data_type_compatible(&self, source: &DataType, target: &DataType) -> bool {
        use DataType::*;

        match (source, target) {
            // Exact matches
            (Integer, Integer)
            | (Float, Float)
            | (String, String)
            | (Boolean, Boolean)
            | (Date, Date)
            | (DateTime, DateTime)
            | (Time, Time)
            | (Json, Json)
            | (Binary, Binary)
            | (Unknown, Unknown) => true,

            // Numeric compatibility
            (Integer, Float) | (Float, Integer) => true,
            (Integer, Decimal { .. }) | (Decimal { .. }, Integer) => true,
            (Float, Decimal { .. }) | (Decimal { .. }, Float) => true,
            (Decimal { .. }, Decimal { .. }) => true,

            // DateTime compatibility
            (Date, DateTime) | (DateTime, Date) => true,

            // Unknown is compatible with everything (no type info)
            (Unknown, _) | (_, Unknown) => true,

            // Everything else is incompatible
            _ => false,
        }
    }

    /// Compare cardinality (distinct count ratio)
    fn cardinality_similarity(&self, source: &FieldProfile, target: &FieldProfile) -> f64 {
        // Calculate cardinality ratio (distinct / total)
        let source_card = if source.total_rows > 0 {
            source.distinct_count as f64 / source.total_rows as f64
        } else {
            0.0
        };

        let target_card = if target.total_rows > 0 {
            target.distinct_count as f64 / target.total_rows as f64
        } else {
            0.0
        };

        // Similarity = 1 - |difference|
        1.0 - (source_card - target_card).abs()
    }

    /// Compare null percentages
    fn null_percentage_similarity(&self, source: &FieldProfile, target: &FieldProfile) -> f64 {
        // Similarity = 1 - |difference|
        1.0 - (source.null_percentage - target.null_percentage).abs()
    }

    /// Calculate value range overlap
    fn value_range_overlap(
        &self,
        source_dist: &ValueDistribution,
        target_dist: &ValueDistribution,
    ) -> f64 {
        // Try numeric comparison first
        if let Some(score) = self.numeric_range_overlap(source_dist, target_dist) {
            return score;
        }

        // Fall back to lexicographic comparison for strings
        if let Some(score) = self.lexicographic_range_overlap(source_dist, target_dist) {
            return score;
        }

        // If no range info available, return neutral score
        0.5
    }

    /// Calculate overlap for numeric ranges
    fn numeric_range_overlap(
        &self,
        source_dist: &ValueDistribution,
        target_dist: &ValueDistribution,
    ) -> Option<f64> {
        let s_min = source_dist.min.as_ref()?.parse::<f64>().ok()?;
        let s_max = source_dist.max.as_ref()?.parse::<f64>().ok()?;
        let t_min = target_dist.min.as_ref()?.parse::<f64>().ok()?;
        let t_max = target_dist.max.as_ref()?.parse::<f64>().ok()?;

        // Calculate overlap
        let overlap_min = s_min.max(t_min);
        let overlap_max = s_max.min(t_max);

        if overlap_max < overlap_min {
            // No overlap
            return Some(0.0);
        }

        let overlap_range = overlap_max - overlap_min;
        let total_range = (s_max - s_min).max(t_max - t_min);

        if total_range == 0.0 {
            // Both are single values
            if s_min == t_min {
                Some(1.0)
            } else {
                Some(0.0)
            }
        } else {
            Some(overlap_range / total_range)
        }
    }

    /// Calculate overlap for string ranges (lexicographic)
    fn lexicographic_range_overlap(
        &self,
        source_dist: &ValueDistribution,
        target_dist: &ValueDistribution,
    ) -> Option<f64> {
        let s_min = source_dist.min.as_ref()?;
        let s_max = source_dist.max.as_ref()?;
        let t_min = target_dist.min.as_ref()?;
        let t_max = target_dist.max.as_ref()?;

        // Check if ranges overlap
        if s_max < t_min || t_max < s_min {
            // No overlap
            return Some(0.0);
        }

        // Ranges overlap
        // For strings, we'll use a simple heuristic:
        // If both have the same min and max, perfect match
        if s_min == t_min && s_max == t_max {
            return Some(1.0);
        }

        // If there's any overlap, give a moderate score
        Some(0.7)
    }

    /// Estimate cardinality match between two fields
    pub fn estimate_cardinality_relationship(
        &self,
        source: &FieldProfile,
        target: &FieldProfile,
    ) -> CardinalityEstimate {
        let source_unique_ratio = if source.total_rows > 0 {
            source.distinct_count as f64 / source.total_rows as f64
        } else {
            0.0
        };

        let target_unique_ratio = if target.total_rows > 0 {
            target.distinct_count as f64 / target.total_rows as f64
        } else {
            0.0
        };

        // High uniqueness (> 0.95) suggests primary key
        let source_is_unique = source_unique_ratio > 0.95;
        let target_is_unique = target_unique_ratio > 0.95;

        match (source_is_unique, target_is_unique) {
            (true, true) => CardinalityEstimate::OneToOne,
            (true, false) => CardinalityEstimate::OneToMany,
            (false, true) => CardinalityEstimate::ManyToOne,
            (false, false) => CardinalityEstimate::ManyToMany,
        }
    }
}

impl Default for StatisticalSimilarity {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimated cardinality relationship
#[derive(Debug, Clone, PartialEq)]
pub enum CardinalityEstimate {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile(
        distinct: u64,
        total: u64,
        null_pct: f64,
        min: Option<&str>,
        max: Option<&str>,
    ) -> FieldProfile {
        FieldProfile {
            distinct_count: distinct,
            total_rows: total,
            null_percentage: null_pct,
            distribution: ValueDistribution {
                min: min.map(|s| s.to_string()),
                max: max.map(|s| s.to_string()),
                median: None,
                p25: None,
                p75: None,
                p95: None,
                p99: None,
            },
            samples: vec![],
        }
    }

    #[test]
    fn test_identical_profiles() {
        let stat = StatisticalSimilarity::new();
        let profile = create_test_profile(1000, 10000, 0.05, Some("1"), Some("1000"));

        let score = stat.compare(&profile, &profile).unwrap();
        assert!((score - 1.0).abs() < 0.001, "Score was {}", score);
    }

    #[test]
    fn test_similar_cardinality() {
        let stat = StatisticalSimilarity::new();
        let profile1 = create_test_profile(1000, 10000, 0.0, Some("1"), Some("1000"));
        let profile2 = create_test_profile(950, 10000, 0.0, Some("1"), Some("950"));

        let score = stat.compare(&profile1, &profile2).unwrap();
        assert!(score > 0.9, "Score was {}", score);
    }

    #[test]
    fn test_numeric_range_overlap() {
        let stat = StatisticalSimilarity::new();

        // Full overlap
        let dist1 = ValueDistribution {
            min: Some("1".to_string()),
            max: Some("100".to_string()),
            ..Default::default()
        };
        let dist2 = ValueDistribution {
            min: Some("1".to_string()),
            max: Some("100".to_string()),
            ..Default::default()
        };
        let score = stat.value_range_overlap(&dist1, &dist2);
        assert!((score - 1.0).abs() < 0.001);

        // Partial overlap
        let dist3 = ValueDistribution {
            min: Some("50".to_string()),
            max: Some("150".to_string()),
            ..Default::default()
        };
        let score = stat.value_range_overlap(&dist1, &dist3);
        assert!(score > 0.3 && score < 0.7, "Score was {}", score);

        // No overlap
        let dist4 = ValueDistribution {
            min: Some("200".to_string()),
            max: Some("300".to_string()),
            ..Default::default()
        };
        let score = stat.value_range_overlap(&dist1, &dist4);
        assert!(score < 0.1, "Score was {}", score);
    }

    #[test]
    fn test_data_type_compatibility() {
        let stat = StatisticalSimilarity::new();

        // Exact matches
        assert!(stat.data_type_compatible(&DataType::Integer, &DataType::Integer));
        assert!(stat.data_type_compatible(&DataType::String, &DataType::String));

        // Numeric compatibility
        assert!(stat.data_type_compatible(&DataType::Integer, &DataType::Float));
        assert!(stat.data_type_compatible(&DataType::Float, &DataType::Integer));

        // Incompatible types
        assert!(!stat.data_type_compatible(&DataType::Integer, &DataType::String));
        assert!(!stat.data_type_compatible(&DataType::Boolean, &DataType::Date));

        // Unknown is compatible with everything
        assert!(stat.data_type_compatible(&DataType::Unknown, &DataType::Integer));
        assert!(stat.data_type_compatible(&DataType::String, &DataType::Unknown));
    }

    #[test]
    fn test_cardinality_estimation() {
        let stat = StatisticalSimilarity::new();

        // One-to-One (both unique)
        let p1 = create_test_profile(10000, 10000, 0.0, Some("1"), Some("10000"));
        let p2 = create_test_profile(10000, 10000, 0.0, Some("1"), Some("10000"));
        assert_eq!(
            stat.estimate_cardinality_relationship(&p1, &p2),
            CardinalityEstimate::OneToOne
        );

        // One-to-Many (source unique, target not)
        let p3 = create_test_profile(10000, 10000, 0.0, Some("1"), Some("10000"));
        let p4 = create_test_profile(5000, 10000, 0.0, Some("1"), Some("5000"));
        assert_eq!(
            stat.estimate_cardinality_relationship(&p3, &p4),
            CardinalityEstimate::OneToMany
        );

        // Many-to-One (source not unique, target unique)
        assert_eq!(
            stat.estimate_cardinality_relationship(&p4, &p3),
            CardinalityEstimate::ManyToOne
        );

        // Many-to-Many (both not unique)
        let p5 = create_test_profile(5000, 10000, 0.0, Some("1"), Some("5000"));
        let p6 = create_test_profile(6000, 10000, 0.0, Some("1"), Some("6000"));
        assert_eq!(
            stat.estimate_cardinality_relationship(&p5, &p6),
            CardinalityEstimate::ManyToMany
        );
    }

    #[test]
    fn test_null_percentage_similarity() {
        let stat = StatisticalSimilarity::new();

        let profile1 = create_test_profile(1000, 10000, 0.05, None, None);
        let profile2 = create_test_profile(1000, 10000, 0.06, None, None);

        let score = stat.null_percentage_similarity(&profile1, &profile2);
        assert!(score > 0.98, "Score was {}", score);
    }
}
