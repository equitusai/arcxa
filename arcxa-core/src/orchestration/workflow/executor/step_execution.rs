use anyhow::Result;

use super::{
    utilities::extract_materializable_rows, BatchStepExecutionResult, ExecutionContext, StepConfig,
    StepResult, StepType, WorkflowExecutor, WorkflowStep,
};
use crate::orchestration::workflow::runtime::frame::BatchFrame;

impl WorkflowExecutor {
    fn batch_result_with_materialized_frame(
        &self,
        success: bool,
        output: serde_json::Value,
        confidence: f64,
    ) -> BatchStepExecutionResult {
        let batch_frame = extract_materializable_rows(&output)
            .and_then(|rows| BatchFrame::from_json_values(&rows).ok());

        match batch_frame {
            Some(frame) => BatchStepExecutionResult::with_frame(success, output, confidence, frame),
            None => BatchStepExecutionResult::without_frame(success, output, confidence),
        }
    }

    /// Execute single workflow step
    pub(super) async fn execute_step(
        &self,
        step: &WorkflowStep,
        context: &ExecutionContext,
    ) -> Result<StepResult> {
        let started_at = chrono::Utc::now();

        self.ensure_step_can_start(step, context)?;
        self.mark_step_execution_started(step, context);

        tracing::info!(
            "EXECUTE_STEP: Starting step '{}' (type: {:?})",
            step.id,
            step.step_type
        );

        let batch_result = self.dispatch_step_execution(step, context).await?;

        tracing::info!(
            "EXECUTE_STEP: Match completed for step '{}', success={}",
            step.id,
            batch_result.success
        );

        let completed_at = chrono::Utc::now();
        self.mark_step_execution_completed(context);

        Ok(self.finalize_step_result(step, started_at, completed_at, batch_result))
    }

    pub(super) async fn dispatch_step_execution(
        &self,
        step: &WorkflowStep,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        match (&step.step_type, &step.config) {
            (StepType::MlPrediction, StepConfig::MLPrediction(config)) => {
                let (success, output, confidence) =
                    self.execute_ml_prediction(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::HeuristicRule, StepConfig::Heuristic(config)) => {
                let (success, output, confidence) = self.execute_heuristic(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::WasmRule, StepConfig::WasmRule(config)) => {
                let (success, output, confidence) = self.execute_wasm_rule(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::ConfidenceGate, StepConfig::ConfidenceGate(config)) => {
                let (success, output, confidence) =
                    self.execute_confidence_gate(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::WeightedVote, StepConfig::WeightedVote(config)) => {
                let (success, output, confidence) =
                    self.execute_weighted_vote(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::ConfidenceAggregate, StepConfig::ConfidenceAggregate(config)) => {
                let (success, output, confidence) =
                    self.execute_confidence_aggregate(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::FieldTransformer, StepConfig::FieldTransformer(config)) => {
                self.execute_field_transformer(config, context).await
            }
            (StepType::CsvSource, StepConfig::CsvSource(config)) => {
                let (success, output, confidence) =
                    self.execute_csv_source(config, context).await?;
                Ok(self.batch_result_with_materialized_frame(success, output, confidence))
            }
            (StepType::Deduplicator, StepConfig::Deduplicator(config)) => {
                self.execute_deduplicator(config, context).await
            }
            (StepType::CsvExporter, StepConfig::CsvExporter(config)) => {
                let (success, output, confidence) =
                    self.execute_csv_exporter(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::DbLoader, StepConfig::DbLoader(config)) => {
                let (success, output, confidence) = self.execute_db_loader(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::DbExtract, StepConfig::DbExtract(config)) => {
                self.execute_db_extract(config, context).await
            }
            (StepType::DataValidator, StepConfig::DataValidator(config)) => {
                self.execute_data_validator(config, context).await
            }
            (StepType::Aggregator, StepConfig::Aggregator(config)) => {
                self.execute_aggregator(config, context).await
            }
            (StepType::SosValidation, StepConfig::SosValidation(config)) => {
                self.execute_sos_validation(config, context).await
            }
            (StepType::DataJoiner, StepConfig::DataJoiner(config)) => {
                let (success, output, confidence) =
                    self.execute_data_joiner(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            (StepType::SemanticMapper, StepConfig::SemanticMapper(config)) => {
                tracing::info!(
                    "EXECUTE_STEP: Calling execute_semantic_mapper for step '{}'",
                    step.id
                );
                let result = self.execute_semantic_mapper(config, context).await;
                if let Err(ref error) = result {
                    tracing::error!(
                        "EXECUTE_STEP: Semantic mapper failed for step '{}': {:?}",
                        step.id,
                        error
                    );
                }
                let (success, output, confidence) = result?;
                Ok(self.batch_result_with_materialized_frame(success, output, confidence))
            }
            (StepType::RdfLoader, StepConfig::RdfLoader(config)) => {
                let (success, output, confidence) =
                    self.execute_rdf_loader(config, context).await?;
                Ok(BatchStepExecutionResult::without_frame(
                    success, output, confidence,
                ))
            }
            _ => anyhow::bail!(
                "Step type mismatch or not yet implemented: {:?}",
                step.step_type
            ),
        }
    }
}
