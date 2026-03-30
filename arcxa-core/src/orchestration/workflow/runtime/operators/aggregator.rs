use crate::orchestration::workflow::definition::{AggFunction, AggregatorConfig};
use crate::orchestration::workflow::error::Result;
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use arrow2::array::{BooleanArray, PrimitiveArray, Utf8Array};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::RuntimeOperator;

/// Batch-native aggregator for the optimized small-dataset path.
///
/// This mirrors the legacy executor contract while preserving usable
/// grouped output values and accepting numeric strings from connector rows.
#[derive(Debug, Default)]
pub struct AggregatorBatchOperator;

impl RuntimeOperator for AggregatorBatchOperator {
    fn name(&self) -> &'static str {
        "aggregator"
    }
}

impl AggregatorBatchOperator {
    pub fn execute(&self, frame: BatchFrame, config: &AggregatorConfig) -> Result<BatchFrame> {
        let group_field_positions = config
            .group_by
            .iter()
            .map(|field| {
                frame
                    .schema()
                    .fields
                    .iter()
                    .position(|schema_field| schema_field.name == *field)
            })
            .collect::<Vec<_>>();

        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for row_index in 0..frame.row_count() {
            let key = group_field_positions
                .iter()
                .map(|field_position| match field_position {
                    Some(field_index) => json_stringified_cell(&frame, *field_index, row_index),
                    None => Ok(String::new()),
                })
                .collect::<Result<Vec<_>>>()?
                .join("|");
            groups.entry(key).or_default().push(row_index);
        }

        let mut result_rows = Vec::with_capacity(groups.len());
        for (key_str, group_rows) in groups {
            let mut result_row = serde_json::Map::new();

            let keys: Vec<&str> = key_str.split('|').collect();
            for (index, field) in config.group_by.iter().enumerate() {
                if index < keys.len() {
                    result_row.insert(field.clone(), parse_group_key_token(keys[index]));
                }
            }

            for aggregation in &config.aggregations {
                let field_position = frame
                    .schema()
                    .fields
                    .iter()
                    .position(|schema_field| schema_field.name == aggregation.field);
                let mut values = Vec::new();
                if let Some(field_index) = field_position {
                    for row_index in &group_rows {
                        if let Some(value) = numeric_cell_value(&frame, field_index, *row_index)? {
                            values.push(value);
                        }
                    }
                }

                let aggregate_value = match aggregation.function {
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
                result_row.insert(field_name, json!(aggregate_value));
            }

            result_rows.push(Value::Object(result_row));
        }

        Ok(BatchFrame::from_json_values(&result_rows)?.with_metadata(frame.metadata().clone()))
    }
}

fn json_stringified_cell(
    frame: &BatchFrame,
    field_index: usize,
    row_index: usize,
) -> Result<String> {
    let field = &frame.schema().fields[field_index];
    let column = frame.columns().arrays()[field_index].as_ref();

    if column.is_null(row_index) {
        return Ok("null".to_string());
    }

    match field.data_type() {
        arrow2::datatypes::DataType::Boolean => {
            let array = column
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected BooleanArray in batch aggregator".into(),
                    )
                })?;
            Ok(Value::Bool(array.value(row_index)).to_string())
        }
        arrow2::datatypes::DataType::Int64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Int64Array in batch aggregator".into(),
                    )
                })?;
            Ok(Value::Number(array.value(row_index).into()).to_string())
        }
        arrow2::datatypes::DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Float64Array in batch aggregator".into(),
                    )
                })?;
            Ok(serde_json::Number::from_f64(array.value(row_index))
                .map(Value::Number)
                .unwrap_or(Value::Null)
                .to_string())
        }
        arrow2::datatypes::DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Utf8Array in batch aggregator".into(),
                    )
                })?;
            Ok(Value::String(array.value(row_index).to_string()).to_string())
        }
        other => Err(
            crate::orchestration::workflow::error::WorkflowError::NotImplemented(format!(
                "Batch aggregator does not support Arrow type {:?}",
                other
            )),
        ),
    }
}

fn numeric_cell_value(
    frame: &BatchFrame,
    field_index: usize,
    row_index: usize,
) -> Result<Option<f64>> {
    let field = &frame.schema().fields[field_index];
    let column = frame.columns().arrays()[field_index].as_ref();

    if column.is_null(row_index) {
        return Ok(None);
    }

    match field.data_type() {
        arrow2::datatypes::DataType::Int64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Int64Array in batch aggregator".into(),
                    )
                })?;
            Ok(Some(array.value(row_index) as f64))
        }
        arrow2::datatypes::DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Float64Array in batch aggregator".into(),
                    )
                })?;
            Ok(Some(array.value(row_index)))
        }
        arrow2::datatypes::DataType::Boolean => Ok(None),
        arrow2::datatypes::DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Expected Utf8Array in batch aggregator".into(),
                    )
                })?;
            Ok(array.value(row_index).trim().parse::<f64>().ok())
        }
        other => Err(
            crate::orchestration::workflow::error::WorkflowError::NotImplemented(format!(
                "Batch aggregator does not support Arrow type {:?}",
                other
            )),
        ),
    }
}

fn parse_group_key_token(token: &str) -> Value {
    serde_json::from_str(token).unwrap_or_else(|_| Value::String(token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::AggregatorBatchOperator;
    use crate::orchestration::workflow::definition::{AggFunction, Aggregation, AggregatorConfig};
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};
    use serde_json::json;

    #[test]
    fn aggregates_rows_and_preserves_metadata() {
        let frame = BatchFrame::from_json_values(&[
            json!({"region": "east", "amount": 10.0, "orders": 1}),
            json!({"region": "east", "amount": 15.0, "orders": 2}),
            json!({"region": "west", "amount": 7.0, "orders": 3}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_aggregate".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });

        let operator = AggregatorBatchOperator;
        let result = operator
            .execute(
                frame,
                &AggregatorConfig {
                    group_by: vec!["region".to_string()],
                    aggregations: vec![
                        Aggregation {
                            field: "amount".to_string(),
                            function: AggFunction::Sum,
                            alias: Some("total_amount".to_string()),
                        },
                        Aggregation {
                            field: "orders".to_string(),
                            function: AggFunction::Count,
                            alias: Some("order_count".to_string()),
                        },
                    ],
                },
            )
            .unwrap();

        let rows = result.to_json_values().unwrap();

        assert_eq!(result.row_count(), 2);
        assert_eq!(
            result.metadata().source_step_id.as_deref(),
            Some("extract_aggregate")
        );
        assert!(rows.iter().any(|row| {
            row["region"] == "east" && row["total_amount"] == 25.0 && row["order_count"] == 2.0
        }));
        assert!(rows.iter().any(|row| {
            row["region"] == "west" && row["total_amount"] == 7.0 && row["order_count"] == 1.0
        }));
    }

    #[test]
    fn aggregates_numeric_strings() {
        let frame = BatchFrame::from_json_values(&[
            json!({"segment": "gold", "amount": "1250.555"}),
            json!({"segment": "gold", "amount": "2200.499"}),
            json!({"segment": "silver", "amount": "850"}),
        ])
        .unwrap();

        let operator = AggregatorBatchOperator;
        let result = operator
            .execute(
                frame,
                &AggregatorConfig {
                    group_by: vec!["segment".to_string()],
                    aggregations: vec![Aggregation {
                        field: "amount".to_string(),
                        function: AggFunction::Sum,
                        alias: Some("total_amount".to_string()),
                    }],
                },
            )
            .unwrap();

        let rows = result.to_json_values().unwrap();

        assert!(rows
            .iter()
            .any(|row| row["segment"] == "gold" && row["total_amount"] == 3451.054));
        assert!(rows
            .iter()
            .any(|row| row["segment"] == "silver" && row["total_amount"] == 850.0));
    }
}
