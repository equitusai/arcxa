use crate::orchestration::workflow::definition::{DataValidatorConfig, RuleType, Severity};
use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use arrow2::array::{BooleanArray, PrimitiveArray, Utf8Array};
use serde_json::{json, Value};

use super::RuntimeOperator;

#[derive(Debug)]
pub struct DataValidatorBatchResult {
    pub frame: BatchFrame,
    pub errors: Vec<Value>,
    pub warnings: Vec<Value>,
    pub success: bool,
}

/// Batch-native data validator for the optimized small-dataset path.
///
/// This intentionally mirrors the current legacy validator semantics:
/// only the currently implemented rule types are enforced, and unsupported
/// rule variants continue to pass through unchanged.
#[derive(Debug, Default)]
pub struct DataValidatorBatchOperator;

impl RuntimeOperator for DataValidatorBatchOperator {
    fn name(&self) -> &'static str {
        "data_validator"
    }
}

impl DataValidatorBatchOperator {
    pub fn execute(
        &self,
        frame: BatchFrame,
        config: &DataValidatorConfig,
    ) -> Result<DataValidatorBatchResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for row_index in 0..frame.row_count() {
            for rule in &config.rules {
                let field_position = frame
                    .schema()
                    .fields
                    .iter()
                    .position(|field| field.name == rule.field);
                let field_value = field_position
                    .map(|field_index| json_value_at(&frame, field_index, row_index))
                    .transpose()?;

                let is_valid = match &rule.rule_type {
                    RuleType::NotNull => matches!(field_value, Some(ref value) if !value.is_null()),
                    RuleType::Regex { pattern } => match field_value.as_ref() {
                        Some(Value::String(string)) => regex::Regex::new(pattern)
                            .map(|compiled| compiled.is_match(string))
                            .unwrap_or(false),
                        _ => false,
                    },
                    RuleType::Range { min, max } => {
                        match field_value.as_ref().and_then(Value::as_f64) {
                            Some(number) => number >= *min && number <= *max,
                            None => false,
                        }
                    }
                    RuleType::InSet { values } => match field_value.as_ref() {
                        Some(Value::String(string)) => values.contains(string),
                        _ => false,
                    },
                    RuleType::Length { min, max } => match field_value.as_ref() {
                        Some(Value::String(string)) => string.len() >= *min && string.len() <= *max,
                        _ => false,
                    },
                    _ => true,
                };

                if !is_valid {
                    let violation = json!({
                        "row": row_index,
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

        Ok(DataValidatorBatchResult {
            frame,
            errors,
            warnings,
            success,
        })
    }
}

fn json_value_at(frame: &BatchFrame, field_index: usize, row_index: usize) -> Result<Value> {
    let field = &frame.schema().fields[field_index];
    let column = frame.columns().arrays()[field_index].as_ref();

    if column.is_null(row_index) {
        return Ok(Value::Null);
    }

    match field.data_type() {
        arrow2::datatypes::DataType::Boolean => {
            let array = column
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData(
                        "Expected BooleanArray in batch data validator".into(),
                    )
                })?;
            Ok(Value::Bool(array.value(row_index)))
        }
        arrow2::datatypes::DataType::Int64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Int64Array in batch data validator".into())
                })?;
            Ok(Value::Number(array.value(row_index).into()))
        }
        arrow2::datatypes::DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData(
                        "Expected Float64Array in batch data validator".into(),
                    )
                })?;
            Ok(serde_json::Number::from_f64(array.value(row_index))
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }
        arrow2::datatypes::DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    WorkflowError::InvalidData("Expected Utf8Array in batch data validator".into())
                })?;
            Ok(Value::String(array.value(row_index).to_string()))
        }
        other => Err(WorkflowError::NotImplemented(format!(
            "Batch data validator does not support Arrow type {:?}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::DataValidatorBatchOperator;
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, ValidationRule,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};
    use serde_json::json;

    #[test]
    fn validates_rows_and_preserves_metadata() {
        let frame = BatchFrame::from_json_values(&[
            json!({"name": "Alice", "age": 30, "status": "active"}),
            json!({"name": null, "age": 150, "status": "inactive"}),
            json!({"name": "Bob", "age": 21, "status": "paused"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_validate".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });

        let operator = DataValidatorBatchOperator;
        let result = operator
            .execute(
                frame,
                &DataValidatorConfig {
                    rules: vec![
                        ValidationRule {
                            field: "name".to_string(),
                            rule_type: RuleType::NotNull,
                            params: None,
                            severity: Severity::Error,
                        },
                        ValidationRule {
                            field: "age".to_string(),
                            rule_type: RuleType::Range {
                                min: 0.0,
                                max: 120.0,
                            },
                            params: None,
                            severity: Severity::Error,
                        },
                        ValidationRule {
                            field: "status".to_string(),
                            rule_type: RuleType::InSet {
                                values: vec!["active".to_string(), "inactive".to_string()],
                            },
                            params: None,
                            severity: Severity::Warning,
                        },
                    ],
                    fail_on_error: true,
                },
            )
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.frame.metadata().source_step_id.as_deref(),
            Some("extract_validate")
        );
    }
}
