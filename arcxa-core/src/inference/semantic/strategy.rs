//! Detection strategy trait and implementations
//!
//! This module defines the core `DetectionStrategy` trait that all semantic
//! type detectors must implement, enabling a plugin-like architecture.

use super::types::{DetectionContext, DetectionResult};
use anyhow::Result;
use async_trait::async_trait;

/// Detection strategy trait
///
/// Implementations of this trait can detect semantic types using different
/// approaches (regex patterns, statistical analysis, ML models, etc.).
///
/// ## Design Principles
/// - **Single Responsibility**: Each strategy focuses on one detection approach
/// - **Composability**: Strategies can be combined in pipelines
/// - **Testability**: Each strategy is independently testable
/// - **Extensibility**: New strategies can be added without modifying existing code
#[async_trait]
pub trait DetectionStrategy: Send + Sync {
    /// Human-readable name of this strategy
    fn name(&self) -> &str;

    /// Priority/weight of this strategy (0.0 - 1.0)
    ///
    /// Higher priority strategies have more influence in aggregation.
    /// Typical values:
    /// - 0.9-1.0: High confidence strategies (exact matches, strong patterns)
    /// - 0.5-0.8: Medium confidence (heuristics, statistical)
    /// - 0.1-0.4: Low confidence (weak signals, fallbacks)
    fn priority(&self) -> f64;

    /// Detect semantic type for a column
    ///
    /// Returns `Ok(Some(result))` if detection succeeded,
    /// `Ok(None)` if no type detected, or `Err` on failure.
    async fn detect(&self, context: &DetectionContext) -> Result<Option<DetectionResult>>;

    /// Check if this strategy is applicable for the given context
    ///
    /// This allows strategies to opt-out early based on data type,
    /// sample size, or other factors.
    fn is_applicable(&self, context: &DetectionContext) -> bool {
        // Default: all strategies are applicable
        let _ = context;
        true
    }

    /// Minimum sample size required for reliable detection
    fn min_sample_size(&self) -> usize {
        10 // Default minimum
    }

    /// Can this strategy run in parallel with others?
    fn is_parallelizable(&self) -> bool {
        true // Most strategies are CPU-bound and parallelizable
    }
}

/// Strategy builder for easy construction
pub struct StrategyBuilder {
    name: String,
    priority: f64,
}

impl StrategyBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            priority: 0.5,
        }
    }

    pub fn priority(mut self, priority: f64) -> Self {
        self.priority = priority.clamp(0.0, 1.0);
        self
    }
}

/// Composite strategy that runs multiple strategies and aggregates results
pub struct CompositeStrategy {
    strategies: Vec<Box<dyn DetectionStrategy>>,
    name: String,
}

impl CompositeStrategy {
    /// Create a new composite strategy
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            strategies: Vec::new(),
            name: name.into(),
        }
    }

    /// Add a strategy to the composite
    pub fn add_strategy(mut self, strategy: Box<dyn DetectionStrategy>) -> Self {
        self.strategies.push(strategy);
        self
    }

    /// Get all child strategies
    pub fn strategies(&self) -> &[Box<dyn DetectionStrategy>] {
        &self.strategies
    }
}

#[async_trait]
impl DetectionStrategy for CompositeStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> f64 {
        // Composite has average priority of children
        if self.strategies.is_empty() {
            0.5
        } else {
            self.strategies.iter().map(|s| s.priority()).sum::<f64>() / self.strategies.len() as f64
        }
    }

    async fn detect(&self, context: &DetectionContext) -> Result<Option<DetectionResult>> {
        // Run all applicable strategies
        let mut results = Vec::new();

        for strategy in &self.strategies {
            if !strategy.is_applicable(context) {
                continue;
            }

            if let Some(result) = strategy.detect(context).await? {
                results.push(result);
            }
        }

        if results.is_empty() {
            return Ok(None);
        }

        // Return highest confidence result
        // (More sophisticated aggregation in composite.rs)
        let best = results
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
            .unwrap();

        Ok(Some(best))
    }

    fn is_applicable(&self, context: &DetectionContext) -> bool {
        // Composite is applicable if any child is applicable
        self.strategies.iter().any(|s| s.is_applicable(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::SemanticType;

    /// Mock strategy for testing
    struct MockStrategy {
        name: String,
        priority: f64,
        result: Option<SemanticType>,
    }

    impl MockStrategy {
        fn new(name: impl Into<String>, priority: f64, result: Option<SemanticType>) -> Self {
            Self {
                name: name.into(),
                priority,
                result,
            }
        }
    }

    #[async_trait]
    impl DetectionStrategy for MockStrategy {
        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> f64 {
            self.priority
        }

        async fn detect(&self, _context: &DetectionContext) -> Result<Option<DetectionResult>> {
            Ok(self
                .result
                .as_ref()
                .map(|st| DetectionResult::new(st.clone(), self.priority, self.name.clone())))
        }
    }

    #[tokio::test]
    async fn test_composite_strategy() {
        let composite = CompositeStrategy::new("test_composite")
            .add_strategy(Box::new(MockStrategy::new(
                "strategy1",
                0.8,
                Some(SemanticType::Email),
            )))
            .add_strategy(Box::new(MockStrategy::new(
                "strategy2",
                0.6,
                Some(SemanticType::Email),
            )));

        assert_eq!(composite.priority(), 0.7); // Average of 0.8 and 0.6

        let context = DetectionContext::new("email", "varchar");
        let result = composite.detect(&context).await.unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::Email);
        assert_eq!(result.confidence, 0.8); // Highest confidence
    }

    #[tokio::test]
    async fn test_composite_no_results() {
        let composite = CompositeStrategy::new("empty")
            .add_strategy(Box::new(MockStrategy::new("strategy1", 0.5, None)))
            .add_strategy(Box::new(MockStrategy::new("strategy2", 0.5, None)));

        let context = DetectionContext::new("unknown", "varchar");
        let result = composite.detect(&context).await.unwrap();

        assert!(result.is_none());
    }
}
