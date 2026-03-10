//! # Confidence Scoring Module
//!
//! Aggregates matches from multiple strategies and computes unified confidence scores.

use std::collections::HashMap;
use tracing::debug;

use super::types::{MappingCandidate, StrategyMatch};

/// Confidence scorer that aggregates strategy results
pub struct ConfidenceScorer {
    /// Weights for each strategy (1.0 = normal weight)
    strategy_weights: HashMap<String, f64>,
}

impl ConfidenceScorer {
    /// Create a new confidence scorer
    pub fn new(strategy_weights: HashMap<String, f64>) -> Self {
        Self { strategy_weights }
    }

    /// Score and aggregate candidates from multiple strategies
    pub fn score_candidates(&self, matches: Vec<StrategyMatch>) -> Vec<MappingCandidate> {
        // Group matches by ontology URI
        let grouped = self.group_by_uri(matches);

        // Compute weighted score for each URI
        let mut candidates = Vec::new();

        for (uri, strategy_matches) in grouped {
            let (confidence, explanation) = self.compute_weighted_score(&strategy_matches);

            candidates.push(MappingCandidate {
                ontology_uri: uri.clone(),
                confidence,
                evidence: strategy_matches,
                transformation: None, // TODO: Infer from evidence
                explanation,
            });
        }

        candidates
    }

    /// Group matches by ontology URI
    fn group_by_uri(&self, matches: Vec<StrategyMatch>) -> HashMap<String, Vec<StrategyMatch>> {
        let mut grouped: HashMap<String, Vec<StrategyMatch>> = HashMap::new();

        for match_item in matches {
            grouped
                .entry(match_item.ontology_uri.clone())
                .or_insert_with(Vec::new)
                .push(match_item);
        }

        grouped
    }

    /// Compute weighted average score for matches to the same URI
    fn compute_weighted_score(&self, matches: &[StrategyMatch]) -> (f64, String) {
        if matches.is_empty() {
            return (0.0, "No evidence".to_string());
        }

        // If there's a manual mapping, use it directly (manual mappings are always 1.0 confidence)
        // Manual mappings should override all other strategies
        if let Some(manual_match) = matches.iter().find(|m| m.strategy_name == "manual") {
            let explanation = format!(
                "manual: {:.2} ({})",
                manual_match.confidence, manual_match.explanation
            );
            debug!(
                "Using manual mapping (overrides other strategies): confidence={:.3}",
                manual_match.confidence
            );
            return (manual_match.confidence, explanation);
        }

        // Calculate weighted sum and total weight
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        let mut explanations = Vec::new();

        for match_item in matches {
            let weight = self
                .strategy_weights
                .get(&match_item.strategy_name)
                .copied()
                .unwrap_or(1.0);

            weighted_sum += match_item.confidence * weight;
            total_weight += weight;

            explanations.push(format!(
                "{}: {:.2} ({})",
                match_item.strategy_name, match_item.confidence, match_item.explanation
            ));
        }

        // Calculate weighted average
        let confidence = if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            // Fallback to simple average if no weights
            matches.iter().map(|m| m.confidence).sum::<f64>() / matches.len() as f64
        };

        // Build explanation
        let explanation = if matches.len() == 1 {
            explanations[0].clone()
        } else {
            format!(
                "Combined evidence from {} strategies: {}",
                matches.len(),
                explanations.join(", ")
            )
        };

        debug!(
            "Scored {} matches: confidence={:.3}, explanation={}",
            matches.len(),
            confidence,
            explanation
        );

        (confidence, explanation)
    }

    /// Update weight for a strategy
    pub fn update_weight(&mut self, strategy: &str, weight: f64) {
        self.strategy_weights.insert(strategy.to_string(), weight);
    }

    /// Remove weight for a strategy
    pub fn remove_weight(&mut self, strategy: &str) {
        self.strategy_weights.remove(strategy);
    }

    /// Get current strategy weights
    pub fn weights(&self) -> &HashMap<String, f64> {
        &self.strategy_weights
    }
}

/// Builder for confidence scorer
pub struct ConfidenceScorerBuilder {
    weights: HashMap<String, f64>,
}

impl ConfidenceScorerBuilder {
    pub fn new() -> Self {
        Self {
            weights: HashMap::new(),
        }
    }

    pub fn with_weight(mut self, strategy: &str, weight: f64) -> Self {
        self.weights.insert(strategy.to_string(), weight);
        self
    }

    pub fn with_default_weights(mut self) -> Self {
        self.weights.insert("pattern".to_string(), 1.5);
        self.weights.insert("semantic".to_string(), 1.2);
        self.weights.insert("statistical".to_string(), 1.0);
        self.weights.insert("lexical".to_string(), 0.8);
        self.weights.insert("registry".to_string(), 1.1);
        self.weights.insert("heuristic".to_string(), 0.6);
        self
    }

    pub fn build(self) -> ConfidenceScorer {
        ConfidenceScorer::new(self.weights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_match(strategy: &str, uri: &str, confidence: f64) -> StrategyMatch {
        StrategyMatch {
            strategy_name: strategy.to_string(),
            ontology_uri: uri.to_string(),
            confidence,
            explanation: format!("{} match", strategy),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_single_strategy_scoring() {
        let scorer = ConfidenceScorerBuilder::new()
            .with_weight("pattern", 1.0)
            .build();

        let matches = vec![create_test_match("pattern", "http://schema.org/email", 0.9)];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ontology_uri, "http://schema.org/email");
        assert_eq!(candidates[0].confidence, 0.9);
    }

    #[test]
    fn test_multiple_strategies_same_uri() {
        let scorer = ConfidenceScorerBuilder::new()
            .with_weight("pattern", 1.5)
            .with_weight("semantic", 1.0)
            .build();

        let matches = vec![
            create_test_match("pattern", "http://schema.org/email", 0.9),
            create_test_match("semantic", "http://schema.org/email", 0.8),
        ];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ontology_uri, "http://schema.org/email");

        // Weighted average: (0.9 * 1.5 + 0.8 * 1.0) / (1.5 + 1.0) = 0.86
        assert!((candidates[0].confidence - 0.86).abs() < 0.01);
    }

    #[test]
    fn test_multiple_uris() {
        let scorer = ConfidenceScorerBuilder::new()
            .with_default_weights()
            .build();

        let matches = vec![
            create_test_match("pattern", "http://schema.org/email", 0.95),
            create_test_match("lexical", "http://schema.org/email", 0.7),
            create_test_match("heuristic", "http://schema.org/name", 0.75),
        ];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 2);

        // Find email candidate
        let email_candidate = candidates
            .iter()
            .find(|c| c.ontology_uri == "http://schema.org/email")
            .unwrap();

        // Find name candidate
        let name_candidate = candidates
            .iter()
            .find(|c| c.ontology_uri == "http://schema.org/name")
            .unwrap();

        // Email should have higher confidence (pattern + lexical evidence)
        assert!(email_candidate.confidence > name_candidate.confidence);
    }

    #[test]
    fn test_no_weights_fallback() {
        let scorer = ConfidenceScorer::new(HashMap::new());

        let matches = vec![
            create_test_match("pattern", "http://schema.org/email", 0.9),
            create_test_match("semantic", "http://schema.org/email", 0.8),
        ];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 1);
        // Should use simple average: (0.9 + 0.8) / 2 = 0.85
        assert!((candidates[0].confidence - 0.85).abs() < 0.0001);
    }

    #[test]
    fn test_weight_update() {
        let mut scorer = ConfidenceScorerBuilder::new()
            .with_weight("pattern", 1.0)
            .build();

        // Update weight
        scorer.update_weight("pattern", 2.0);

        let matches = vec![create_test_match("pattern", "http://schema.org/email", 0.9)];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates[0].confidence, 0.9); // Weight doesn't change single match
    }

    #[test]
    fn test_explanation_generation() {
        let scorer = ConfidenceScorerBuilder::new()
            .with_default_weights()
            .build();

        // Single strategy
        let single_match = vec![create_test_match("pattern", "http://schema.org/email", 0.9)];

        let candidates = scorer.score_candidates(single_match);
        assert!(candidates[0].explanation.contains("pattern"));

        // Multiple strategies
        let multi_match = vec![
            create_test_match("pattern", "http://schema.org/email", 0.9),
            create_test_match("semantic", "http://schema.org/email", 0.8),
        ];

        let candidates = scorer.score_candidates(multi_match);
        assert!(candidates[0].explanation.contains("Combined evidence"));
        assert!(candidates[0].explanation.contains("2 strategies"));
    }
}
