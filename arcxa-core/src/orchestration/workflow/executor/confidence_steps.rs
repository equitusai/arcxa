use anyhow::Result;

use super::{ExecutionContext, WorkflowExecutor};
use crate::orchestration::confidence::{AggregationMethod, ConfidenceAggregator, ConfidenceScore};

impl WorkflowExecutor {
    pub(super) async fn execute_confidence_gate(
        &self,
        config: &crate::orchestration::workflow::definition::ConfidenceGateConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        let confidence = if let Some(ref input_step) = config.input_step {
            context
                .step_outputs
                .get(input_step)
                .and_then(|value| value.get("confidence"))
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        } else if context.step_outputs.is_empty() {
            context
                .input_data
                .get("confidence")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        } else {
            let total: f64 = context
                .step_outputs
                .values()
                .filter_map(|value| value.get("confidence")?.as_f64())
                .sum();
            let count = context
                .step_outputs
                .values()
                .filter(|value| value.get("confidence").and_then(|c| c.as_f64()).is_some())
                .count();

            if count > 0 {
                total / count as f64
            } else {
                0.0
            }
        };

        let passed = confidence >= config.threshold;

        Ok((
            passed,
            serde_json::json!({
                "threshold": config.threshold,
                "actual_confidence": confidence,
                "confidence": confidence,
                "passed": passed,
            }),
            confidence,
        ))
    }

    pub(super) async fn execute_weighted_vote(
        &self,
        config: &crate::orchestration::workflow::definition::WeightedVoteConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        let mut weighted_sum = 0.0;

        for (step_id, weight) in &config.weights {
            let confidence = context
                .step_outputs
                .get(step_id)
                .and_then(|value| value.get("confidence"))
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);

            weighted_sum += confidence * weight;
        }

        Ok((
            true,
            serde_json::json!({
                "weighted_confidence": weighted_sum,
            }),
            weighted_sum,
        ))
    }

    pub(super) async fn execute_confidence_aggregate(
        &self,
        config: &crate::orchestration::workflow::definition::ConfidenceAggregateConfig,
        context: &ExecutionContext,
    ) -> Result<(bool, serde_json::Value, f64)> {
        let scores: Vec<ConfidenceScore> = if config.inputs.is_empty() {
            context
                .step_outputs
                .iter()
                .filter_map(|(step_id, output)| {
                    let confidence = output.get("confidence")?.as_f64()?;
                    Some(ConfidenceScore {
                        source: step_id.clone(),
                        confidence,
                        weight: 1.0,
                    })
                })
                .collect()
        } else {
            config
                .inputs
                .iter()
                .filter_map(|step_id| {
                    let output = context.step_outputs.get(step_id)?;
                    let confidence = output.get("confidence")?.as_f64()?;
                    Some(ConfidenceScore {
                        source: step_id.clone(),
                        confidence,
                        weight: 1.0,
                    })
                })
                .collect()
        };

        let method = match config.method.as_str() {
            "weighted_average" => AggregationMethod::WeightedAverage,
            "bayesian" => AggregationMethod::Bayesian,
            "voting" => AggregationMethod::Voting,
            _ => AggregationMethod::WeightedAverage,
        };

        let aggregator = ConfidenceAggregator::new(method);
        let aggregated_confidence = aggregator.aggregate(&scores);

        Ok((
            true,
            serde_json::json!({
                "method": config.method,
                "aggregated_confidence": aggregated_confidence,
                "input_count": scores.len(),
            }),
            aggregated_confidence,
        ))
    }
}
