use arrow2::datatypes::{DataType, Field, Schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::orchestration::workflow::error::{Result, WorkflowError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameDataType {
    Null,
    Boolean,
    Int64,
    Float64,
    Utf8,
}

impl FrameDataType {
    fn merge(self, other: Self) -> Self {
        use FrameDataType::*;
        match (self, other) {
            (Utf8, _) | (_, Utf8) => Utf8,
            (Float64, _) | (_, Float64) => Float64,
            (Int64, Int64) => Int64,
            (Boolean, Boolean) => Boolean,
            (Null, rhs) => rhs,
            (lhs, Null) => lhs,
            _ => Utf8,
        }
    }

    fn observe_json(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(_) => Self::Boolean,
            Value::Number(n) if n.is_i64() || n.is_u64() => Self::Int64,
            Value::Number(_) => Self::Float64,
            Value::String(_) => Self::Utf8,
            Value::Array(_) | Value::Object(_) => Self::Utf8,
        }
    }

    pub fn to_arrow(self) -> DataType {
        match self {
            Self::Null => DataType::Utf8,
            Self::Boolean => DataType::Boolean,
            Self::Int64 => DataType::Int64,
            Self::Float64 => DataType::Float64,
            Self::Utf8 => DataType::Utf8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameFieldProfile {
    pub name: String,
    pub data_type: FrameDataType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameSchemaProfile {
    pub fields: Vec<FrameFieldProfile>,
}

pub fn infer_arrow_schema(rows: &[Map<String, Value>]) -> FrameSchemaProfile {
    let mut profiles: BTreeMap<String, FrameFieldProfile> = BTreeMap::new();

    for row in rows {
        for (key, value) in row {
            let observed = FrameDataType::observe_json(value);
            profiles
                .entry(key.clone())
                .and_modify(|field| {
                    field.data_type = field.data_type.merge(observed);
                    if value.is_null() {
                        field.nullable = true;
                    }
                })
                .or_insert(FrameFieldProfile {
                    name: key.clone(),
                    data_type: observed,
                    nullable: value.is_null(),
                });
        }
    }

    FrameSchemaProfile {
        fields: profiles.into_values().collect(),
    }
}

pub fn schema_profile_from_arrow(schema: &Schema) -> Result<FrameSchemaProfile> {
    let fields = schema
        .fields
        .iter()
        .map(|field| {
            let data_type = match field.data_type() {
                DataType::Boolean => FrameDataType::Boolean,
                DataType::Int64 => FrameDataType::Int64,
                DataType::Float64 => FrameDataType::Float64,
                DataType::Utf8 => FrameDataType::Utf8,
                other => {
                    return Err(WorkflowError::NotImplemented(format!(
                        "BatchFrame Arrow conversion does not support Arrow type {:?}",
                        other
                    )));
                }
            };

            Ok(FrameFieldProfile {
                name: field.name.clone(),
                data_type,
                nullable: field.is_nullable,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(FrameSchemaProfile { fields })
}

impl FrameSchemaProfile {
    pub fn to_arrow_schema(&self) -> Schema {
        Schema::from(
            self.fields
                .iter()
                .map(|field| {
                    Field::new(
                        field.name.clone(),
                        field.data_type.to_arrow(),
                        field.nullable,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}
