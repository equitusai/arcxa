use std::collections::HashMap;

use anyhow::Result;

use super::{FinalDecision, StepResult, WorkflowExecutor};
use crate::orchestration::workflow::definition::FallbackStrategy;

impl WorkflowExecutor {
    pub(super) fn compute_final_decision(
        &self,
        step_results: &HashMap<String, StepResult>,
    ) -> Result<FinalDecision> {
        let execution_order = self.dag.execution_order()?;
        let last_step_id = &execution_order
            .last()
            .ok_or_else(|| anyhow::anyhow!("No steps in workflow"))?
            .id;

        let last_result = step_results
            .get(last_step_id)
            .ok_or_else(|| anyhow::anyhow!("Last step result not found"))?;

        if last_result.confidence >= self.definition.fusion_threshold {
            Ok(FinalDecision::Accept)
        } else {
            Ok(match self.definition.fallback {
                FallbackStrategy::ManualReview => FinalDecision::ManualReview,
                FallbackStrategy::RejectFusion => FinalDecision::Reject,
                FallbackStrategy::AcceptFusion => FinalDecision::Accept,
            })
        }
    }

    pub(super) fn compute_final_confidence(
        &self,
        step_results: &HashMap<String, StepResult>,
    ) -> f64 {
        if step_results.is_empty() {
            return 0.0;
        }

        step_results
            .values()
            .map(|result| result.confidence)
            .sum::<f64>()
            / step_results.len() as f64
    }
}
