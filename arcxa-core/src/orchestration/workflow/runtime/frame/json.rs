use crate::orchestration::workflow::error::{Result, WorkflowError};
use serde_json::{Map, Value};

pub fn json_values_to_object_rows(rows: &[Value]) -> Result<Vec<Map<String, Value>>> {
    rows.iter()
        .map(|row| match row {
            Value::Object(map) => Ok(map.clone()),
            _ => Err(WorkflowError::InvalidData(
                "BatchFrame requires JSON object rows".to_string(),
            )),
        })
        .collect()
}

pub fn object_rows_to_json_values(rows: Vec<Map<String, Value>>) -> Vec<Value> {
    rows.into_iter().map(Value::Object).collect()
}
