use super::json::{json_values_to_object_rows, object_rows_to_json_values};
use super::schema::{infer_arrow_schema, FrameDataType, FrameSchemaProfile};
use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::RowAccessor;
use arrow2::array::{
    Array, BooleanArray, MutableBooleanArray, MutablePrimitiveArray, MutableUtf8Array,
    PrimitiveArray, Utf8Array,
};
use arrow2::chunk::Chunk;
use arrow2::datatypes::Schema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchFrameMetadata {
    pub source_step_id: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
}

#[derive(Clone)]
pub struct BatchFrame {
    pub(super) schema_profile: FrameSchemaProfile,
    pub(super) schema: Arc<Schema>,
    pub(super) columns: Chunk<Box<dyn Array>>,
    pub(super) row_count: usize,
    pub(super) metadata: BatchFrameMetadata,
}

impl std::fmt::Debug for BatchFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchFrame")
            .field("schema_profile", &self.schema_profile)
            .field("row_count", &self.row_count)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl BatchFrame {
    pub fn from_json_values(rows: &[Value]) -> Result<Self> {
        let object_rows = json_values_to_object_rows(rows)?;
        Self::from_object_rows(&object_rows)
    }

    pub fn from_row_accessor(accessor: &RowAccessor) -> Result<Self> {
        let rows = accessor.to_vec()?;
        Self::from_json_values(&rows)
    }

    pub fn from_object_rows(rows: &[Map<String, Value>]) -> Result<Self> {
        let schema_profile = infer_arrow_schema(rows);
        let schema = Arc::new(schema_profile.to_arrow_schema());
        let columns = build_arrow_columns(&schema_profile, rows)?;

        Ok(Self {
            schema_profile,
            schema,
            columns,
            row_count: rows.len(),
            metadata: BatchFrameMetadata::default(),
        })
    }

    pub fn schema(&self) -> &Schema {
        self.schema.as_ref()
    }

    pub fn schema_profile(&self) -> &FrameSchemaProfile {
        &self.schema_profile
    }

    pub fn columns(&self) -> &Chunk<Box<dyn Array>> {
        &self.columns
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }

    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    pub fn metadata(&self) -> &BatchFrameMetadata {
        &self.metadata
    }

    pub fn with_metadata(mut self, metadata: BatchFrameMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn to_json_values(&self) -> Result<Vec<Value>> {
        let mut rows = vec![Map::new(); self.row_count];

        for (field, column) in self.schema.fields.iter().zip(self.columns.arrays().iter()) {
            match field.data_type() {
                arrow2::datatypes::DataType::Boolean => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                WorkflowError::InvalidData(format!(
                                    "Expected BooleanArray for column '{}'",
                                    field.name
                                ))
                            })?;

                    for row_index in 0..self.row_count {
                        let value = if array.is_null(row_index) {
                            Value::Null
                        } else {
                            Value::Bool(array.value(row_index))
                        };
                        rows[row_index].insert(field.name.clone(), value);
                    }
                }
                arrow2::datatypes::DataType::Int64 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<PrimitiveArray<i64>>()
                        .ok_or_else(|| {
                            WorkflowError::InvalidData(format!(
                                "Expected Int64 array for column '{}'",
                                field.name
                            ))
                        })?;

                    for row_index in 0..self.row_count {
                        let value = if array.is_null(row_index) {
                            Value::Null
                        } else {
                            Value::Number(array.value(row_index).into())
                        };
                        rows[row_index].insert(field.name.clone(), value);
                    }
                }
                arrow2::datatypes::DataType::Float64 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<PrimitiveArray<f64>>()
                        .ok_or_else(|| {
                            WorkflowError::InvalidData(format!(
                                "Expected Float64 array for column '{}'",
                                field.name
                            ))
                        })?;

                    for row_index in 0..self.row_count {
                        let value = if array.is_null(row_index) {
                            Value::Null
                        } else {
                            serde_json::Number::from_f64(array.value(row_index))
                                .map(Value::Number)
                                .unwrap_or(Value::Null)
                        };
                        rows[row_index].insert(field.name.clone(), value);
                    }
                }
                arrow2::datatypes::DataType::Utf8 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Utf8Array<i32>>()
                        .ok_or_else(|| {
                            WorkflowError::InvalidData(format!(
                                "Expected Utf8 array for column '{}'",
                                field.name
                            ))
                        })?;

                    for row_index in 0..self.row_count {
                        let value = if array.is_null(row_index) {
                            Value::Null
                        } else {
                            Value::String(array.value(row_index).to_string())
                        };
                        rows[row_index].insert(field.name.clone(), value);
                    }
                }
                other => {
                    return Err(WorkflowError::NotImplemented(format!(
                        "BatchFrame JSON conversion does not support Arrow type {:?}",
                        other
                    )));
                }
            }
        }

        Ok(object_rows_to_json_values(rows))
    }
}

fn build_arrow_columns(
    schema_profile: &FrameSchemaProfile,
    rows: &[Map<String, Value>],
) -> Result<Chunk<Box<dyn Array>>> {
    let arrays = schema_profile
        .fields
        .iter()
        .map(|field| match field.data_type {
            FrameDataType::Boolean => build_boolean_array(field.name.as_str(), rows),
            FrameDataType::Int64 => build_int64_array(field.name.as_str(), rows),
            FrameDataType::Float64 => build_float64_array(field.name.as_str(), rows),
            FrameDataType::Utf8 | FrameDataType::Null => {
                build_utf8_array(field.name.as_str(), rows)
            }
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Chunk::new(arrays))
}

fn build_boolean_array(column_name: &str, rows: &[Map<String, Value>]) -> Result<Box<dyn Array>> {
    let mut array = MutableBooleanArray::new();
    for row in rows {
        match row.get(column_name) {
            None | Some(Value::Null) => array.push(None),
            Some(Value::Bool(boolean)) => array.push(Some(*boolean)),
            Some(other) => {
                return Err(WorkflowError::InvalidData(format!(
                    "Column '{}' expected boolean values but found {}",
                    column_name, other
                )));
            }
        }
    }
    Ok(Box::new(BooleanArray::from(array)))
}

fn build_int64_array(column_name: &str, rows: &[Map<String, Value>]) -> Result<Box<dyn Array>> {
    let mut array = MutablePrimitiveArray::<i64>::new();
    for row in rows {
        match row.get(column_name) {
            None | Some(Value::Null) => array.push(None),
            Some(Value::Number(number)) => {
                let converted = if let Some(value) = number.as_i64() {
                    Some(value)
                } else {
                    number.as_u64().map(|value| value as i64)
                };
                array.push(converted);
            }
            Some(other) => {
                return Err(WorkflowError::InvalidData(format!(
                    "Column '{}' expected integer values but found {}",
                    column_name, other
                )));
            }
        }
    }
    Ok(Box::new(PrimitiveArray::<i64>::from(array)))
}

fn build_float64_array(column_name: &str, rows: &[Map<String, Value>]) -> Result<Box<dyn Array>> {
    let mut array = MutablePrimitiveArray::<f64>::new();
    for row in rows {
        match row.get(column_name) {
            None | Some(Value::Null) => array.push(None),
            Some(Value::Number(number)) => array.push(number.as_f64()),
            Some(other) => {
                return Err(WorkflowError::InvalidData(format!(
                    "Column '{}' expected numeric values but found {}",
                    column_name, other
                )));
            }
        }
    }
    Ok(Box::new(PrimitiveArray::<f64>::from(array)))
}

fn build_utf8_array(column_name: &str, rows: &[Map<String, Value>]) -> Result<Box<dyn Array>> {
    let mut array = MutableUtf8Array::<i32>::new();
    for row in rows {
        match row.get(column_name) {
            None | Some(Value::Null) => array.push::<&str>(None),
            Some(Value::String(string)) => array.push(Some(string.as_str())),
            Some(other) => {
                let stringified = match other {
                    Value::Array(_) | Value::Object(_) => serde_json::to_string(other)?,
                    _ => other.to_string(),
                };
                array.push(Some(stringified.as_str()));
            }
        }
    }
    let array: Utf8Array<i32> = array.into();
    Ok(Box::new(array))
}

#[cfg(test)]
mod tests {
    use super::BatchFrame;
    use serde_json::json;

    #[test]
    fn round_trips_json_rows_through_batch_frame() {
        let rows = vec![
            json!({"id": 1, "name": "alice", "active": true, "score": 10.5}),
            json!({"id": 2, "name": "bob", "active": false, "score": null}),
        ];

        let frame = BatchFrame::from_json_values(&rows).expect("frame to build");
        assert_eq!(frame.row_count(), 2);
        assert_eq!(frame.schema().fields.len(), 4);

        let round_tripped = frame.to_json_values().expect("frame to round-trip");
        assert_eq!(round_tripped, rows);
    }

    #[test]
    fn stringifies_nested_json_values_for_batch_compatibility() {
        let rows = vec![json!({"payload": {"nested": true}, "tags": ["a", "b"]})];
        let frame = BatchFrame::from_json_values(&rows).expect("frame to build");
        let round_tripped = frame.to_json_values().expect("frame to round-trip");

        assert_eq!(round_tripped[0]["payload"], json!("{\"nested\":true}"));
        assert_eq!(round_tripped[0]["tags"], json!("[\"a\",\"b\"]"));
    }
}
