use anyhow::Result;
use serde_json::{json, Value};

use super::{
    BatchStepExecutionResult, ExecutionContext, SosValidationConfig, SosValidationStepResult,
    WorkflowExecutor,
};

impl WorkflowExecutor {
    pub(super) async fn execute_sos_validation(
        &self,
        config: &SosValidationConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        let callback = self
            .sos_validation_callback
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No SoS validation callback configured"))?;

        let callback_result = callback(config, context).await?;
        let success = self.evaluate_sos_validation_success(config, &callback_result);
        let output = self.build_sos_validation_output(config, callback_result, success)?;
        let confidence = output
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);

        Ok(BatchStepExecutionResult::without_frame(
            success, output, confidence,
        ))
    }

    fn evaluate_sos_validation_success(
        &self,
        config: &SosValidationConfig,
        result: &SosValidationStepResult,
    ) -> bool {
        let blocking_severities: std::collections::HashSet<String> = config
            .blocking_severities
            .iter()
            .map(|severity| severity.to_ascii_lowercase())
            .collect();

        !result.checks.iter().any(|check| {
            !check.passed && blocking_severities.contains(&check.severity.to_ascii_lowercase())
        })
    }

    fn build_sos_validation_output(
        &self,
        config: &SosValidationConfig,
        result: SosValidationStepResult,
        step_passed: bool,
    ) -> Result<Value> {
        let mut output = serde_json::to_value(result)?;
        if let Some(object) = output.as_object_mut() {
            object.insert("step_passed".to_string(), json!(step_passed));
            object.insert(
                "blocking_severities".to_string(),
                serde_json::to_value(&config.blocking_severities)?,
            );
            object.insert("persist_report".to_string(), json!(config.persist_report));
            object.insert(
                "emit_graph_lineage".to_string(),
                json!(config.emit_graph_lineage),
            );
        }

        Ok(output)
    }
}
