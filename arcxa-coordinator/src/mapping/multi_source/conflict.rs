//! Conflict Resolution Engine
//!
//! This module provides logic for resolving conflicts when multiple source
//! fields map to the same ontology term and target database column.
//!
//! Resolution strategies:
//! - UsePrimary: Select value from a designated primary source
//! - Merge: Concatenate values from all sources with a separator
//! - Coalesce: Use first non-null value
//! - CustomRule: Apply user-defined transformation rule

use super::types::ConflictResolution;
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Conflict resolver for applying resolution strategies
pub struct ConflictResolver {
    /// Optional custom rules registry
    custom_rules: HashMap<String, Box<dyn Fn(&[SourceValue]) -> Option<String> + Send + Sync>>,
}

/// Source value with metadata
#[derive(Debug, Clone)]
pub struct SourceValue {
    /// Source identifier (e.g., "csv_001.customer_email")
    pub source_id: String,

    /// The actual value
    pub value: Option<String>,

    /// Optional confidence score
    pub confidence: Option<f64>,
}

/// Result of conflict resolution
#[derive(Debug, Clone)]
pub struct ResolvedValue {
    /// The resolved value
    pub value: Option<String>,

    /// Sources that contributed to the resolved value
    pub contributing_sources: Vec<String>,

    /// Resolution strategy used
    pub strategy_used: String,
}

impl ConflictResolver {
    /// Create a new conflict resolver
    pub fn new() -> Self {
        Self {
            custom_rules: HashMap::new(),
        }
    }

    /// Register a custom resolution rule
    pub fn register_custom_rule<F>(&mut self, name: String, rule: F)
    where
        F: Fn(&[SourceValue]) -> Option<String> + Send + Sync + 'static,
    {
        self.custom_rules.insert(name, Box::new(rule));
    }

    /// Resolve conflicting values using the specified strategy
    pub fn resolve(
        &self,
        strategy: &ConflictResolution,
        sources: &[SourceValue],
    ) -> Result<ResolvedValue> {
        match strategy {
            ConflictResolution::UsePrimary { primary_source } => {
                self.resolve_use_primary(primary_source, sources)
            }
            ConflictResolution::Merge { separator } => self.resolve_merge(separator, sources),
            ConflictResolution::Coalesce => self.resolve_coalesce(sources),
            ConflictResolution::CustomRule { rule } => self.resolve_custom_rule(rule, sources),
            ConflictResolution::NoConflict => {
                // No conflict means single source
                if sources.len() != 1 {
                    return Err(anyhow::anyhow!(
                        "NoConflict strategy requires exactly one source, got {}",
                        sources.len()
                    ));
                }

                Ok(ResolvedValue {
                    value: sources[0].value.clone(),
                    contributing_sources: vec![sources[0].source_id.clone()],
                    strategy_used: "NoConflict".to_string(),
                })
            }
        }
    }

    /// UsePrimary strategy: Select value from designated primary source
    fn resolve_use_primary(
        &self,
        primary_source: &str,
        sources: &[SourceValue],
    ) -> Result<ResolvedValue> {
        // Find the primary source
        let primary = sources
            .iter()
            .find(|s| s.source_id == primary_source)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Primary source '{}' not found in source list",
                    primary_source
                )
            })?;

        Ok(ResolvedValue {
            value: primary.value.clone(),
            contributing_sources: vec![primary.source_id.clone()],
            strategy_used: format!("UsePrimary({})", primary_source),
        })
    }

    /// Merge strategy: Concatenate all values with separator
    fn resolve_merge(&self, separator: &str, sources: &[SourceValue]) -> Result<ResolvedValue> {
        let non_null_values: Vec<String> = sources
            .iter()
            .filter_map(|s| s.value.as_ref())
            .cloned()
            .collect();

        if non_null_values.is_empty() {
            return Ok(ResolvedValue {
                value: None,
                contributing_sources: vec![],
                strategy_used: "Merge".to_string(),
            });
        }

        let merged = non_null_values.join(separator);
        let contributing: Vec<String> = sources
            .iter()
            .filter(|s| s.value.is_some())
            .map(|s| s.source_id.clone())
            .collect();

        Ok(ResolvedValue {
            value: Some(merged),
            contributing_sources: contributing,
            strategy_used: format!("Merge('{}')", separator),
        })
    }

    /// Coalesce strategy: Use first non-null value
    fn resolve_coalesce(&self, sources: &[SourceValue]) -> Result<ResolvedValue> {
        // Sort by confidence (descending) if available, otherwise use order
        let mut sorted_sources = sources.to_vec();
        sorted_sources.sort_by(|a, b| match (a.confidence, b.confidence) {
            (Some(conf_a), Some(conf_b)) => conf_b.partial_cmp(&conf_a).unwrap(),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        // Find first non-null value
        let first_non_null = sorted_sources.iter().find(|s| s.value.is_some());

        match first_non_null {
            Some(source) => Ok(ResolvedValue {
                value: source.value.clone(),
                contributing_sources: vec![source.source_id.clone()],
                strategy_used: "Coalesce".to_string(),
            }),
            None => Ok(ResolvedValue {
                value: None,
                contributing_sources: vec![],
                strategy_used: "Coalesce".to_string(),
            }),
        }
    }

    /// CustomRule strategy: Apply user-defined rule
    fn resolve_custom_rule(
        &self,
        rule_name: &str,
        sources: &[SourceValue],
    ) -> Result<ResolvedValue> {
        let rule = self
            .custom_rules
            .get(rule_name)
            .ok_or_else(|| anyhow::anyhow!("Custom rule '{}' not found in registry", rule_name))?;

        let result = rule(sources);

        // Determine contributing sources
        let contributing: Vec<String> = sources
            .iter()
            .filter(|s| s.value.is_some())
            .map(|s| s.source_id.clone())
            .collect();

        Ok(ResolvedValue {
            value: result,
            contributing_sources: contributing,
            strategy_used: format!("CustomRule({})", rule_name),
        })
    }

    /// Suggest a resolution strategy based on source characteristics
    pub fn suggest_resolution(&self, sources: &[SourceValue]) -> ConflictResolution {
        if sources.is_empty() {
            return ConflictResolution::NoConflict;
        }

        if sources.len() == 1 {
            return ConflictResolution::NoConflict;
        }

        // Check if we have confidence scores
        let has_confidence = sources.iter().any(|s| s.confidence.is_some());

        if has_confidence {
            // Find source with highest confidence
            let primary = sources
                .iter()
                .max_by(|a, b| {
                    let conf_a = a.confidence.unwrap_or(0.0);
                    let conf_b = b.confidence.unwrap_or(0.0);
                    conf_a.partial_cmp(&conf_b).unwrap()
                })
                .unwrap();

            return ConflictResolution::UsePrimary {
                primary_source: primary.source_id.clone(),
            };
        }

        // Check if all values are the same
        let non_null_values: Vec<&String> =
            sources.iter().filter_map(|s| s.value.as_ref()).collect();

        if !non_null_values.is_empty() {
            let first = non_null_values[0];
            if non_null_values.iter().all(|v| *v == first) {
                // All values are the same - use coalesce
                return ConflictResolution::Coalesce;
            }
        }

        // Default: Use first source as primary
        ConflictResolution::UsePrimary {
            primary_source: sources[0].source_id.clone(),
        }
    }
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_source(id: &str, value: Option<&str>, confidence: Option<f64>) -> SourceValue {
        SourceValue {
            source_id: id.to_string(),
            value: value.map(|v| v.to_string()),
            confidence,
        }
    }

    #[test]
    fn test_resolve_no_conflict() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![create_source("src1", Some("value1"), None)];

        let result = resolver.resolve(&ConflictResolution::NoConflict, &sources)?;

        assert_eq!(result.value, Some("value1".to_string()));
        assert_eq!(result.contributing_sources, vec!["src1"]);
        assert_eq!(result.strategy_used, "NoConflict");

        Ok(())
    }

    #[test]
    fn test_resolve_no_conflict_multiple_sources_fails() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("value1"), None),
            create_source("src2", Some("value2"), None),
        ];

        let result = resolver.resolve(&ConflictResolution::NoConflict, &sources);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_resolve_use_primary() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("csv_001.email", Some("john@example.com"), Some(0.95)),
            create_source("csv_002.customer_email", Some("john@test.com"), Some(0.80)),
        ];

        let strategy = ConflictResolution::UsePrimary {
            primary_source: "csv_001.email".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, Some("john@example.com".to_string()));
        assert_eq!(result.contributing_sources, vec!["csv_001.email"]);
        assert!(result.strategy_used.contains("UsePrimary"));

        Ok(())
    }

    #[test]
    fn test_resolve_use_primary_not_found() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![create_source(
            "csv_001.email",
            Some("john@example.com"),
            None,
        )];

        let strategy = ConflictResolution::UsePrimary {
            primary_source: "nonexistent".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_resolve_merge() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("John"), None),
            create_source("src2", Some("Doe"), None),
        ];

        let strategy = ConflictResolution::Merge {
            separator: " ".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, Some("John Doe".to_string()));
        assert_eq!(result.contributing_sources.len(), 2);
        assert!(result.strategy_used.contains("Merge"));

        Ok(())
    }

    #[test]
    fn test_resolve_merge_with_nulls() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("John"), None),
            create_source("src2", None, None),
            create_source("src3", Some("Doe"), None),
        ];

        let strategy = ConflictResolution::Merge {
            separator: " ".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, Some("John Doe".to_string()));
        assert_eq!(result.contributing_sources, vec!["src1", "src3"]);

        Ok(())
    }

    #[test]
    fn test_resolve_merge_all_nulls() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", None, None),
            create_source("src2", None, None),
        ];

        let strategy = ConflictResolution::Merge {
            separator: " ".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, None);
        assert_eq!(result.contributing_sources.len(), 0);

        Ok(())
    }

    #[test]
    fn test_resolve_merge_custom_separator() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("alpha"), None),
            create_source("src2", Some("beta"), None),
            create_source("src3", Some("gamma"), None),
        ];

        let strategy = ConflictResolution::Merge {
            separator: " | ".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, Some("alpha | beta | gamma".to_string()));

        Ok(())
    }

    #[test]
    fn test_resolve_coalesce() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", None, None),
            create_source("src2", Some("first_non_null"), Some(0.80)),
            create_source("src3", Some("second_non_null"), Some(0.90)),
        ];

        let strategy = ConflictResolution::Coalesce;

        let result = resolver.resolve(&strategy, &sources)?;

        // Should pick src3 because it has higher confidence
        assert_eq!(result.value, Some("second_non_null".to_string()));
        assert_eq!(result.contributing_sources, vec!["src3"]);

        Ok(())
    }

    #[test]
    fn test_resolve_coalesce_no_confidence() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", None, None),
            create_source("src2", Some("first_non_null"), None),
            create_source("src3", Some("second_non_null"), None),
        ];

        let strategy = ConflictResolution::Coalesce;

        let result = resolver.resolve(&strategy, &sources)?;

        // Should pick first non-null in order (src2)
        assert_eq!(result.value, Some("first_non_null".to_string()));
        assert_eq!(result.contributing_sources, vec!["src2"]);

        Ok(())
    }

    #[test]
    fn test_resolve_coalesce_all_null() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", None, None),
            create_source("src2", None, None),
        ];

        let strategy = ConflictResolution::Coalesce;

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, None);
        assert_eq!(result.contributing_sources.len(), 0);

        Ok(())
    }

    #[test]
    fn test_resolve_custom_rule() -> Result<()> {
        let mut resolver = ConflictResolver::new();

        // Register a custom rule: concatenate values in uppercase
        resolver.register_custom_rule(
            "uppercase_concat".to_string(),
            |sources: &[SourceValue]| {
                let values: Vec<String> = sources
                    .iter()
                    .filter_map(|s| s.value.as_ref())
                    .map(|v| v.to_uppercase())
                    .collect();

                if values.is_empty() {
                    None
                } else {
                    Some(values.join("_"))
                }
            },
        );

        let sources = vec![
            create_source("src1", Some("john"), None),
            create_source("src2", Some("doe"), None),
        ];

        let strategy = ConflictResolution::CustomRule {
            rule: "uppercase_concat".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources)?;

        assert_eq!(result.value, Some("JOHN_DOE".to_string()));
        assert_eq!(result.contributing_sources, vec!["src1", "src2"]);

        Ok(())
    }

    #[test]
    fn test_resolve_custom_rule_not_found() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![create_source("src1", Some("value"), None)];

        let strategy = ConflictResolution::CustomRule {
            rule: "nonexistent_rule".to_string(),
        };

        let result = resolver.resolve(&strategy, &sources);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_suggest_resolution_single_source() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![create_source("src1", Some("value"), None)];

        let suggestion = resolver.suggest_resolution(&sources);

        assert!(matches!(suggestion, ConflictResolution::NoConflict));

        Ok(())
    }

    #[test]
    fn test_suggest_resolution_with_confidence() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("value1"), Some(0.80)),
            create_source("src2", Some("value2"), Some(0.95)),
            create_source("src3", Some("value3"), Some(0.70)),
        ];

        let suggestion = resolver.suggest_resolution(&sources);

        match suggestion {
            ConflictResolution::UsePrimary { primary_source } => {
                assert_eq!(primary_source, "src2"); // Highest confidence
            }
            _ => panic!("Expected UsePrimary strategy"),
        }

        Ok(())
    }

    #[test]
    fn test_suggest_resolution_same_values() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("same_value"), None),
            create_source("src2", Some("same_value"), None),
        ];

        let suggestion = resolver.suggest_resolution(&sources);

        // Should suggest Coalesce since all values are the same
        assert!(matches!(suggestion, ConflictResolution::Coalesce));

        Ok(())
    }

    #[test]
    fn test_suggest_resolution_different_values_no_confidence() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![
            create_source("src1", Some("value1"), None),
            create_source("src2", Some("value2"), None),
        ];

        let suggestion = resolver.suggest_resolution(&sources);

        // Should default to UsePrimary with first source
        match suggestion {
            ConflictResolution::UsePrimary { primary_source } => {
                assert_eq!(primary_source, "src1");
            }
            _ => panic!("Expected UsePrimary strategy"),
        }

        Ok(())
    }

    #[test]
    fn test_suggest_resolution_empty_sources() -> Result<()> {
        let resolver = ConflictResolver::new();
        let sources = vec![];

        let suggestion = resolver.suggest_resolution(&sources);

        assert!(matches!(suggestion, ConflictResolution::NoConflict));

        Ok(())
    }
}
