//! Voting Strategies for Field Resolution
//!
//! Implements multiple strategies for resolving conflicting field values:
//! - Frequency: Most common value wins
//! - Time-Decay: Recent values weighted higher
//! - Authority: Trusted sources weighted higher
//! - Ensemble: Combine multiple strategies

use super::types::*;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Voting engine for field resolution
pub struct VotingEngine {
    /// Default strategy to use
    default_strategy: StrategyType,
}

impl VotingEngine {
    /// Create a new voting engine
    pub fn new(default_strategy: StrategyType) -> Self {
        Self { default_strategy }
    }

    /// Resolve a field value from multiple source values
    pub fn resolve_field(
        &self,
        entity_id: &str,
        field_name: &str,
        source_values: Vec<SourceValue>,
        strategy: Option<VotingStrategy>,
    ) -> Result<FieldResolution> {
        if source_values.is_empty() {
            anyhow::bail!("Cannot resolve field with no source values");
        }

        // Use provided strategy or default
        let strategy = strategy.unwrap_or_else(|| VotingStrategy {
            strategy_type: self.default_strategy,
            parameters: serde_json::json!({}),
            description: format!("{:?} voting strategy", self.default_strategy),
        });

        // Apply voting strategy
        let (selected, rejected, explanation, conflict) = match strategy.strategy_type {
            StrategyType::Frequency => self.frequency_voting(&source_values)?,
            StrategyType::TimeDecay => {
                let decay_rate = strategy
                    .parameters
                    .get("decay_rate")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.1);
                let reference_time = strategy
                    .parameters
                    .get("reference_time")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(Utc::now);

                self.time_decay_voting(&source_values, decay_rate, reference_time)?
            }
            StrategyType::Authority => self.authority_voting(&source_values)?,
            StrategyType::Ensemble => {
                let strategies = strategy
                    .parameters
                    .get("strategies")
                    .and_then(|v| v.as_array())
                    .context("Ensemble strategy requires 'strategies' parameter")?;

                self.ensemble_voting(&source_values, strategies)?
            }
            StrategyType::MlPrediction => {
                anyhow::bail!("ML prediction voting not yet implemented");
            }
            StrategyType::Custom => {
                anyhow::bail!("Custom voting strategies require user-defined implementation");
            }
        };

        // Create resolution
        let resolution_id = format!(
            "res_{}_{}_{}",
            entity_id,
            field_name,
            Utc::now().timestamp_millis()
        );

        Ok(FieldResolution {
            id: resolution_id,
            entity_id: entity_id.to_string(),
            field_name: field_name.to_string(),
            source_values,
            selected_value: selected,
            rejected_values: rejected,
            strategy,
            resolved_at: Utc::now(),
            conflict,
            resolved_by: "VotingEngine".to_string(),
            explanation,
            review: None,
        })
    }

    /// Frequency voting: Most common value wins
    fn frequency_voting(
        &self,
        source_values: &[SourceValue],
    ) -> Result<(SourceValue, Vec<SourceValue>, String, Option<FieldConflict>)> {
        // Count occurrences of each value
        let mut value_counts: HashMap<String, Vec<&SourceValue>> = HashMap::new();

        for source in source_values {
            let key = serde_json::to_string(&source.value)?;
            value_counts.entry(key).or_default().push(source);
        }

        // Find most common value
        let (most_common_key, most_common_sources) = value_counts
            .iter()
            .max_by_key(|(_, sources)| sources.len())
            .context("No values to vote on")?;

        let vote_count = most_common_sources.len();
        let total_count = source_values.len();
        let confidence = vote_count as f64 / total_count as f64;

        // Create selected value with vote count
        let mut selected = (*most_common_sources[0]).clone();
        selected.vote_count = vote_count as u32;
        selected.vote_weight = vote_count as f64;

        // Create rejected values
        let rejected: Vec<SourceValue> = source_values
            .iter()
            .filter(|s| serde_json::to_string(&s.value).unwrap() != *most_common_key)
            .cloned()
            .collect();

        // Check for conflicts
        let conflict = if value_counts.len() > 1 {
            let severity = if confidence <= 0.5 {
                ConflictSeverity::High
            } else if confidence < 0.7 {
                ConflictSeverity::Medium
            } else {
                ConflictSeverity::Low
            };

            Some(FieldConflict {
                id: format!("conflict_{}", Utc::now().timestamp_millis()),
                conflicting_values: source_values.to_vec(),
                severity,
                reason: format!(
                    "{} different values found. Most common has {} votes ({:.1}%)",
                    value_counts.len(),
                    vote_count,
                    confidence * 100.0
                ),
                requires_review: severity.requires_human_review(),
                suggested_resolution: None,
            })
        } else {
            None
        };

        let explanation = format!(
            "Frequency voting: {} out of {} sources agree ({:.1}% confidence)",
            vote_count,
            total_count,
            confidence * 100.0
        );

        Ok((selected, rejected, explanation, conflict))
    }

    /// Time-decay voting: Recent values weighted higher
    fn time_decay_voting(
        &self,
        source_values: &[SourceValue],
        decay_rate: f64,
        reference_time: DateTime<Utc>,
    ) -> Result<(SourceValue, Vec<SourceValue>, String, Option<FieldConflict>)> {
        // Calculate time-weighted scores for each value
        let mut value_scores: HashMap<String, (f64, Vec<&SourceValue>)> = HashMap::new();

        for source in source_values {
            let key = serde_json::to_string(&source.value)?;

            // Calculate time difference in days
            let age_days = (reference_time - source.source_timestamp).num_days() as f64;

            // Exponential decay: weight = exp(-decay_rate * age)
            let weight = (-decay_rate * age_days).exp();

            let entry = value_scores.entry(key).or_insert((0.0, Vec::new()));
            entry.0 += weight;
            entry.1.push(source);
        }

        // Find value with highest weighted score
        let (best_key, (best_score, best_sources)) = value_scores
            .iter()
            .max_by(|(_, (score1, _)), (_, (score2, _))| {
                score1
                    .partial_cmp(score2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .context("No values to vote on")?;

        let total_weight: f64 = value_scores.values().map(|(score, _)| score).sum();
        let confidence = best_score / total_weight;

        // Create selected value with weighted score
        let mut selected = (*best_sources[0]).clone();
        selected.vote_weight = *best_score;

        // Create rejected values
        let rejected: Vec<SourceValue> = source_values
            .iter()
            .filter(|s| serde_json::to_string(&s.value).unwrap() != *best_key)
            .cloned()
            .collect();

        // Check for conflicts
        let conflict = if value_scores.len() > 1 {
            let severity = if confidence < 0.5 {
                ConflictSeverity::High
            } else if confidence < 0.7 {
                ConflictSeverity::Medium
            } else {
                ConflictSeverity::Low
            };

            Some(FieldConflict {
                id: format!("conflict_{}", Utc::now().timestamp_millis()),
                conflicting_values: source_values.to_vec(),
                severity,
                reason: format!(
                    "{} different values found. Best has weighted score {:.2} ({:.1}% confidence)",
                    value_scores.len(),
                    best_score,
                    confidence * 100.0
                ),
                requires_review: severity.requires_human_review(),
                suggested_resolution: None,
            })
        } else {
            None
        };

        let explanation = format!(
            "Time-decay voting (decay={:.2}): Selected value has weighted score {:.2} ({:.1}% confidence)",
            decay_rate, best_score, confidence * 100.0
        );

        Ok((selected, rejected, explanation, conflict))
    }

    /// Authority voting: Trusted sources weighted higher
    fn authority_voting(
        &self,
        source_values: &[SourceValue],
    ) -> Result<(SourceValue, Vec<SourceValue>, String, Option<FieldConflict>)> {
        // Calculate authority-weighted scores for each value
        let mut value_scores: HashMap<String, (f64, Vec<&SourceValue>)> = HashMap::new();

        for source in source_values {
            let key = serde_json::to_string(&source.value)?;

            // Weight by source authority
            let weight = source.source_authority;

            let entry = value_scores.entry(key).or_insert((0.0, Vec::new()));
            entry.0 += weight;
            entry.1.push(source);
        }

        // Find value with highest authority score
        let (best_key, (best_score, best_sources)) = value_scores
            .iter()
            .max_by(|(_, (score1, _)), (_, (score2, _))| {
                score1
                    .partial_cmp(score2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .context("No values to vote on")?;

        let total_authority: f64 = source_values.iter().map(|s| s.source_authority).sum();
        let confidence = best_score / total_authority;

        // Create selected value with authority score
        let mut selected = (*best_sources[0]).clone();
        selected.vote_weight = *best_score;

        // Create rejected values
        let rejected: Vec<SourceValue> = source_values
            .iter()
            .filter(|s| serde_json::to_string(&s.value).unwrap() != *best_key)
            .cloned()
            .collect();

        // Check for conflicts
        let conflict = if value_scores.len() > 1 {
            let severity = if confidence < 0.6 {
                ConflictSeverity::High
            } else if confidence < 0.8 {
                ConflictSeverity::Medium
            } else {
                ConflictSeverity::Low
            };

            Some(FieldConflict {
                id: format!("conflict_{}", Utc::now().timestamp_millis()),
                conflicting_values: source_values.to_vec(),
                severity,
                reason: format!(
                    "{} different values found. Best has authority score {:.2} ({:.1}% confidence)",
                    value_scores.len(),
                    best_score,
                    confidence * 100.0
                ),
                requires_review: severity.requires_human_review(),
                suggested_resolution: None,
            })
        } else {
            None
        };

        let explanation = format!(
            "Authority voting: Selected value from sources with combined authority {:.2} ({:.1}% confidence)",
            best_score, confidence * 100.0
        );

        Ok((selected, rejected, explanation, conflict))
    }

    /// Ensemble voting: Combine multiple strategies
    fn ensemble_voting(
        &self,
        source_values: &[SourceValue],
        strategies: &[serde_json::Value],
    ) -> Result<(SourceValue, Vec<SourceValue>, String, Option<FieldConflict>)> {
        if strategies.is_empty() {
            anyhow::bail!("Ensemble requires at least one strategy");
        }

        // Run each strategy and collect results
        let mut strategy_results = Vec::new();

        for strategy_config in strategies {
            let strategy_type = strategy_config
                .get("type")
                .and_then(|v| v.as_str())
                .context("Strategy must have 'type' field")?;

            let weight = strategy_config
                .get("weight")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);

            let result = match strategy_type {
                "frequency" => self.frequency_voting(source_values)?,
                "time-decay" => {
                    let decay_rate = strategy_config
                        .get("decay_rate")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.1);
                    self.time_decay_voting(source_values, decay_rate, Utc::now())?
                }
                "authority" => self.authority_voting(source_values)?,
                _ => anyhow::bail!("Unknown strategy type: {}", strategy_type),
            };

            strategy_results.push((result.0, weight));
        }

        // Aggregate scores across strategies
        let mut value_scores: HashMap<String, f64> = HashMap::new();

        for (selected, weight) in &strategy_results {
            let key = serde_json::to_string(&selected.value)?;
            *value_scores.entry(key).or_insert(0.0) += weight;
        }

        // Find value with highest ensemble score
        let (best_key, best_score) = value_scores
            .iter()
            .max_by(|(_, score1), (_, score2)| {
                score1
                    .partial_cmp(score2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .context("No values to vote on")?;

        let total_weight: f64 = strategy_results.iter().map(|(_, w)| w).sum();
        let confidence = best_score / total_weight;

        // Find the source value that matches
        let selected = source_values
            .iter()
            .find(|s| serde_json::to_string(&s.value).unwrap() == *best_key)
            .context("Selected value not found in source values")?
            .clone();

        // Create rejected values
        let rejected: Vec<SourceValue> = source_values
            .iter()
            .filter(|s| serde_json::to_string(&s.value).unwrap() != *best_key)
            .cloned()
            .collect();

        // Check for conflicts (if any strategy reported a conflict)
        let has_conflicts = value_scores.len() > 1;
        let conflict = if has_conflicts {
            let severity = if confidence < 0.6 {
                ConflictSeverity::High
            } else if confidence < 0.75 {
                ConflictSeverity::Medium
            } else {
                ConflictSeverity::Low
            };

            Some(FieldConflict {
                id: format!("conflict_{}", Utc::now().timestamp_millis()),
                conflicting_values: source_values.to_vec(),
                severity,
                reason: format!(
                    "Ensemble of {} strategies found {} different values. Best has score {:.2} ({:.1}% confidence)",
                    strategies.len(),
                    value_scores.len(),
                    best_score,
                    confidence * 100.0
                ),
                requires_review: severity.requires_human_review(),
                suggested_resolution: None,
            })
        } else {
            None
        };

        let explanation = format!(
            "Ensemble voting ({} strategies): Selected value has combined score {:.2} ({:.1}% confidence)",
            strategies.len(), best_score, confidence * 100.0
        );

        Ok((selected, rejected, explanation, conflict))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_source(value: &str, system: &str, authority: f64, days_old: i64) -> SourceValue {
        SourceValue {
            id: format!("src_{}_{}", system, value),
            value: serde_json::json!(value),
            source_system: system.to_string(),
            source_timestamp: Utc::now() - chrono::Duration::days(days_old),
            source_authority: authority,
            confidence: None,
            vote_count: 0,
            vote_weight: 0.0,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_frequency_voting_clear_winner() {
        let engine = VotingEngine::new(StrategyType::Frequency);

        let sources = vec![
            create_test_source("123 Main St", "CRM", 0.8, 0),
            create_test_source("123 Main St", "ERP", 0.9, 1),
            create_test_source("123 Main St", "Website", 0.7, 2),
            create_test_source("456 Oak Ave", "Legacy", 0.5, 10),
        ];

        let result = engine
            .resolve_field("cust_001", "address", sources, None)
            .unwrap();

        assert_eq!(
            result.selected_value.value,
            serde_json::json!("123 Main St")
        );
        assert_eq!(result.selected_value.vote_count, 3);
        assert_eq!(result.rejected_values.len(), 1);
        assert!(result.conflict.is_some());

        let conflict = result.conflict.unwrap();
        assert_eq!(conflict.severity, ConflictSeverity::Low); // 75% confidence
    }

    #[test]
    fn test_time_decay_voting_recent_wins() {
        let engine = VotingEngine::new(StrategyType::TimeDecay);

        let sources = vec![
            create_test_source("new@example.com", "Website", 0.7, 0), // Today
            create_test_source("old@example.com", "CRM", 0.9, 365),   // 1 year ago
        ];

        let strategy = VotingStrategy {
            strategy_type: StrategyType::TimeDecay,
            parameters: serde_json::json!({
                "decay_rate": 0.01  // 1% decay per day
            }),
            description: "Time-decay with 1% daily decay".to_string(),
        };

        let result = engine
            .resolve_field("cust_001", "email", sources, Some(strategy))
            .unwrap();

        // Recent value should win despite old value having higher authority
        assert_eq!(
            result.selected_value.value,
            serde_json::json!("new@example.com")
        );
        assert!(result.selected_value.vote_weight > 0.0);
    }

    #[test]
    fn test_authority_voting_trusted_source_wins() {
        let engine = VotingEngine::new(StrategyType::Authority);

        let sources = vec![
            create_test_source("555-1234", "Website", 0.3, 0), // Low authority
            create_test_source("555-5678", "CRM", 0.95, 1),    // High authority
            create_test_source("555-9999", "Email", 0.4, 2),   // Low authority
        ];

        let result = engine
            .resolve_field("cust_001", "phone", sources, None)
            .unwrap();

        // High authority CRM should win with highest single authority
        assert_eq!(result.selected_value.value, serde_json::json!("555-5678"));
        assert!(result.selected_value.vote_weight >= 0.95);
    }

    #[test]
    fn test_ensemble_voting() {
        let engine = VotingEngine::new(StrategyType::Ensemble);

        let sources = vec![
            create_test_source("value1", "CRM", 0.9, 0),
            create_test_source("value1", "ERP", 0.8, 1),
            create_test_source("value2", "Website", 0.7, 2),
        ];

        let strategy = VotingStrategy {
            strategy_type: StrategyType::Ensemble,
            parameters: serde_json::json!({
                "strategies": [
                    {"type": "frequency", "weight": 1.0},
                    {"type": "authority", "weight": 1.0}
                ]
            }),
            description: "Ensemble of frequency and authority".to_string(),
        };

        let result = engine
            .resolve_field("cust_001", "field", sources, Some(strategy))
            .unwrap();

        // value1 should win (majority + higher authority)
        assert_eq!(result.selected_value.value, serde_json::json!("value1"));
        assert_eq!(result.strategy.strategy_type, StrategyType::Ensemble);
    }

    #[test]
    fn test_conflict_severity() {
        let engine = VotingEngine::new(StrategyType::Frequency);

        // Evenly split values = high conflict
        let sources = vec![
            create_test_source("value1", "CRM", 0.8, 0),
            create_test_source("value2", "ERP", 0.8, 0),
        ];

        let result = engine
            .resolve_field("cust_001", "field", sources, None)
            .unwrap();

        assert!(result.conflict.is_some());
        let conflict = result.conflict.unwrap();
        assert_eq!(conflict.severity, ConflictSeverity::High); // 50% confidence
        assert!(conflict.requires_review);
    }
}
