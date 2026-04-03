use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use super::{
    build_materialized_rows_step_result, BatchStepExecutionResult, ExecutionContext,
    WorkflowExecutor,
};

impl WorkflowExecutor {
    /// Execute aggregator step - aggregate data by groups
    pub(super) async fn execute_aggregator(
        &self,
        config: &crate::orchestration::workflow::definition::AggregatorConfig,
        context: &ExecutionContext,
    ) -> Result<BatchStepExecutionResult> {
        use crate::orchestration::workflow::definition::AggFunction;

        tracing::info!(
            "Executing aggregator: group_by={:?}, aggregations={}",
            config.group_by,
            config.aggregations.len()
        );

        if let Some(batch_result) = self.try_execute_aggregator_batch(context, config)? {
            return Ok(batch_result);
        }

        let rows = self.get_rows_from_context(context)?;

        let mut groups: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
        for row in &rows {
            let key = config
                .group_by
                .iter()
                .map(|field| {
                    row.get(field)
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|");
            groups.entry(key).or_default().push(row);
        }

        let mut result_rows: Vec<serde_json::Value> = Vec::new();
        for (key_str, group_rows) in groups {
            let mut result_row = serde_json::Map::new();

            let keys: Vec<&str> = key_str.split('|').collect();
            for (index, field) in config.group_by.iter().enumerate() {
                if index < keys.len() {
                    result_row.insert(field.clone(), parse_group_key_token(keys[index]));
                }
            }

            for aggregation in &config.aggregations {
                let values: Vec<f64> = group_rows
                    .iter()
                    .filter_map(|row| row.get(&aggregation.field).and_then(json_numeric_value))
                    .collect();

                let aggregated_value = match aggregation.function {
                    AggFunction::Sum => values.iter().sum(),
                    AggFunction::Avg => {
                        if values.is_empty() {
                            0.0
                        } else {
                            values.iter().sum::<f64>() / values.len() as f64
                        }
                    }
                    AggFunction::Count => values.len() as f64,
                    AggFunction::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
                    AggFunction::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    _ => 0.0,
                };

                let field_name = aggregation.alias.clone().unwrap_or_else(|| {
                    format!("{}_{:?}", aggregation.field, aggregation.function).to_lowercase()
                });
                result_row.insert(field_name, serde_json::json!(aggregated_value));
            }

            result_rows.push(serde_json::Value::Object(result_row));
        }

        tracing::info!(
            "Aggregation complete: {} groups from {} rows",
            result_rows.len(),
            rows.len()
        );

        Ok(build_materialized_rows_step_result(
            result_rows,
            vec![("_original_count".to_string(), serde_json::json!(rows.len()))],
            true,
            1.0,
        ))
    }
}

fn json_numeric_value(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<f64>().ok())
    })
}

fn parse_group_key_token(token: &str) -> Value {
    serde_json::from_str(token).unwrap_or_else(|_| Value::String(token.to_string()))
}
