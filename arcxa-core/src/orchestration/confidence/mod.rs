//! Confidence aggregation algorithms
//!
//! Provides weighted average, Bayesian fusion, and voting mechanisms for
//! aggregating confidence scores from multiple sources

use serde::{Deserialize, Serialize};

/// Confidence aggregator
pub struct ConfidenceAggregator {
    method: AggregationMethod,
}

impl ConfidenceAggregator {
    pub fn new(method: AggregationMethod) -> Self {
        Self { method }
    }

    /// Aggregate multiple confidence scores
    pub fn aggregate(&self, scores: &[ConfidenceScore]) -> f64 {
        if scores.is_empty() {
            return 0.0;
        }

        match self.method {
            AggregationMethod::WeightedAverage => self.weighted_average(scores),
            AggregationMethod::Bayesian => self.bayesian_fusion(scores),
            AggregationMethod::Voting => self.voting(scores),
        }
    }

    fn weighted_average(&self, scores: &[ConfidenceScore]) -> f64 {
        let total_weight: f64 = scores.iter().map(|s| s.weight).sum();
        if total_weight == 0.0 {
            return 0.0;
        }

        scores.iter().map(|s| s.confidence * s.weight).sum::<f64>() / total_weight
    }

    fn bayesian_fusion(&self, scores: &[ConfidenceScore]) -> f64 {
        // Simplified Bayesian fusion using product
        // Full implementation would use Beta distributions
        let product: f64 = scores.iter().map(|s| s.confidence).product();

        product.powf(1.0 / scores.len() as f64)
    }

    fn voting(&self, scores: &[ConfidenceScore]) -> f64 {
        let above_threshold = scores.iter().filter(|s| s.confidence >= 0.5).count();

        if above_threshold as f64 > scores.len() as f64 / 2.0 {
            0.9
        } else {
            0.1
        }
    }
}

/// Aggregation method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod {
    WeightedAverage,
    Bayesian,
    Voting,
}

/// Confidence score with weight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceScore {
    pub source: String,
    pub confidence: f64,
    pub weight: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_average() {
        let aggregator = ConfidenceAggregator::new(AggregationMethod::WeightedAverage);

        let scores = vec![
            ConfidenceScore {
                source: "ml".to_string(),
                confidence: 0.9,
                weight: 0.7,
            },
            ConfidenceScore {
                source: "heuristic".to_string(),
                confidence: 0.6,
                weight: 0.3,
            },
        ];

        let result = aggregator.aggregate(&scores);
        assert!((result - 0.81).abs() < 0.01); // 0.9 * 0.7 + 0.6 * 0.3 = 0.81
    }

    #[test]
    fn test_bayesian_fusion() {
        let aggregator = ConfidenceAggregator::new(AggregationMethod::Bayesian);

        let scores = vec![
            ConfidenceScore {
                source: "ml".to_string(),
                confidence: 0.9,
                weight: 1.0,
            },
            ConfidenceScore {
                source: "heuristic".to_string(),
                confidence: 0.81,
                weight: 1.0,
            },
        ];

        let result = aggregator.aggregate(&scores);
        assert!((result - 0.855).abs() < 0.01); // sqrt(0.9 * 0.81) ≈ 0.855
    }

    #[test]
    fn test_voting() {
        let aggregator = ConfidenceAggregator::new(AggregationMethod::Voting);

        let scores = vec![
            ConfidenceScore {
                source: "ml".to_string(),
                confidence: 0.9,
                weight: 1.0,
            },
            ConfidenceScore {
                source: "heuristic".to_string(),
                confidence: 0.8,
                weight: 1.0,
            },
            ConfidenceScore {
                source: "wasm".to_string(),
                confidence: 0.2,
                weight: 1.0,
            },
        ];

        let result = aggregator.aggregate(&scores);
        assert_eq!(result, 0.9); // 2 out of 3 above 0.5
    }
}
