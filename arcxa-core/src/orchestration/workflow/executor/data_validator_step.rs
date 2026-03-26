use anyhow::Result;
use regex::Regex;

use super::{build_rows_output, BatchStepExecutionResult, ExecutionContext, WorkflowExecutor};

impl WorkflowExecutor {
    /// Execute data validator step - validate data against rules
    pub(super) async fn execute_data_validator(
        &self,
        config: &crate::orchestration::workflow::definition::DataValidatorConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        use crate::orchestration::workflow::definition::{RuleType, Severity};

        tracing::info!("Executing data validator: {} rules", config.rules.len());

        let rows = self.get_rows_from_context(context)?;

        if let Some(batch_result) = self.try_execute_data_validator_batch(context, config, &rows)? {
            return Ok(batch_result);
        }

        let mut errors: Vec<serde_json::Value> = Vec::new();
        let mut warnings: Vec<serde_json::Value> = Vec::new();

        for (row_idx, row) in rows.iter().enumerate() {
            for rule in &config.rules {
                let field_value = row.get(&rule.field);
                let is_valid = match &rule.rule_type {
                    RuleType::NotNull => matches!(field_value, Some(v) if !v.is_null()),
                    RuleType::Regex { pattern } => {
                        if let Some(serde_json::Value::String(s)) = field_value {
                            Regex::new(pattern)
                                .map(|re| re.is_match(s))
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                    RuleType::Range { min, max } => {
                        if let Some(v) = field_value.and_then(|v| v.as_f64()) {
                            v >= *min && v <= *max
                        } else {
                            false
                        }
                    }
                    RuleType::InSet { values } => {
                        if let Some(serde_json::Value::String(s)) = field_value {
                            values.contains(s)
                        } else {
                            false
                        }
                    }
                    RuleType::Length { min, max } => {
                        if let Some(serde_json::Value::String(s)) = field_value {
                            s.len() >= *min && s.len() <= *max
                        } else {
                            false
                        }
                    }
                    _ => true,
                };

                if !is_valid {
                    let violation = serde_json::json!({
                        "row": row_idx,
                        "field": rule.field,
                        "rule_type": format!("{:?}", rule.rule_type),
                        "value": field_value,
                    });

                    match rule.severity {
                        Severity::Error => errors.push(violation),
                        Severity::Warning => warnings.push(violation),
                    }
                }
            }
        }

        let success = !config.fail_on_error || errors.is_empty();

        tracing::info!(
            "Validation complete: {} errors, {} warnings",
            errors.len(),
            warnings.len()
        );

        let row_count = rows.len();
        let error_count = errors.len();
        let warning_count = warnings.len();

        Ok(BatchStepExecutionResult::without_frame(
            success,
            build_rows_output(
                rows,
                row_count,
                vec![
                    (
                        "_errors".to_string(),
                        serde_json::Value::Array(errors.clone()),
                    ),
                    (
                        "_warnings".to_string(),
                        serde_json::Value::Array(warnings.clone()),
                    ),
                    ("_error_count".to_string(), serde_json::json!(error_count)),
                    (
                        "_warning_count".to_string(),
                        serde_json::json!(warning_count),
                    ),
                ],
            ),
            if errors.is_empty() { 1.0 } else { 0.0 },
        ))
    }
}
