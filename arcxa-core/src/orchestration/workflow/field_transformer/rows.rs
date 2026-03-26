use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::orchestration::workflow::definition::FieldTransformerConfig;
use crate::orchestration::workflow::field_transformer::operations::is_reversible;

use super::operations::apply_transform_operation;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RowTransformationStats {
    pub rows_transformed: usize,
    pub fields_modified: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldModificationSummary {
    pub field_name: String,
    pub old_value: Value,
    pub new_value: Value,
    pub operation_count: usize,
    pub rows_modified: usize,
    pub is_reversible: bool,
}

impl FieldModificationSummary {
    pub(crate) fn to_json(&self) -> Value {
        serde_json::json!({
            "field_name": self.field_name,
            "old_value": self.old_value,
            "new_value": self.new_value,
            "operations": self.operation_count,
            "is_reversible": self.is_reversible,
            "metadata": {
                "rows_modified": self.rows_modified,
            }
        })
    }
}

pub(crate) fn transform_object_rows(
    rows: &[Map<String, Value>],
    config: &FieldTransformerConfig,
) -> Result<(Vec<Map<String, Value>>, RowTransformationStats)> {
    let (transformed_rows, stats, _) = transform_object_rows_with_metadata(rows, config)?;
    Ok((transformed_rows, stats))
}

pub(crate) fn transform_object_rows_with_metadata(
    rows: &[Map<String, Value>],
    config: &FieldTransformerConfig,
) -> Result<(
    Vec<Map<String, Value>>,
    RowTransformationStats,
    Vec<FieldModificationSummary>,
)> {
    let mut transformed_rows = Vec::with_capacity(rows.len());
    let mut stats = RowTransformationStats::default();
    let mut modification_summaries = BTreeMap::<String, FieldModificationSummary>::new();

    for row in rows {
        let mut transformed_row = row.clone();
        let mut row_modified = false;

        for transformation in &config.transformations {
            let Some(old_value) = transformed_row.get(&transformation.field).cloned() else {
                continue;
            };

            let mut current_value = old_value.clone();
            for operation in &transformation.operations {
                current_value = apply_transform_operation(&current_value, operation)?;
            }

            if old_value != current_value {
                transformed_row.insert(transformation.field.clone(), current_value);
                stats.fields_modified += 1;
                row_modified = true;
                let current_value = transformed_row
                    .get(&transformation.field)
                    .cloned()
                    .unwrap_or(Value::Null);
                modification_summaries
                    .entry(transformation.field.clone())
                    .and_modify(|summary| {
                        summary.rows_modified += 1;
                    })
                    .or_insert_with(|| FieldModificationSummary {
                        field_name: transformation.field.clone(),
                        old_value: old_value.clone(),
                        new_value: current_value,
                        operation_count: transformation.operations.len(),
                        rows_modified: 1,
                        is_reversible: is_reversible(&transformation.operations),
                    });
            }
        }

        if row_modified {
            stats.rows_transformed += 1;
        }

        transformed_rows.push(transformed_row);
    }

    Ok((
        transformed_rows,
        stats,
        modification_summaries.into_values().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        transform_object_rows, transform_object_rows_with_metadata, RowTransformationStats,
    };
    use crate::orchestration::workflow::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };
    use serde_json::{json, Map, Value};

    #[test]
    fn transforms_rows_and_counts_modified_fields() {
        let rows = vec![
            as_object(json!({"email": "  TEST@EXAMPLE.COM  ", "status": "ACTIVE"})),
            as_object(json!({"email": "second@example.com", "status": "PENDING"})),
            as_object(json!({"status": "MISSING_EMAIL"})),
        ];
        let config = FieldTransformerConfig {
            transformations: vec![
                FieldTransformation {
                    field: "email".to_string(),
                    operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                },
                FieldTransformation {
                    field: "status".to_string(),
                    operations: vec![TransformOperation::Lower],
                },
            ],
        };

        let (transformed, stats) = transform_object_rows(&rows, &config).unwrap();

        assert_eq!(
            transformed[0].get("email"),
            Some(&json!("test@example.com"))
        );
        assert_eq!(transformed[0].get("status"), Some(&json!("active")));
        assert_eq!(
            transformed[1].get("email"),
            Some(&json!("second@example.com"))
        );
        assert_eq!(transformed[1].get("status"), Some(&json!("pending")));
        assert_eq!(transformed[2].get("status"), Some(&json!("missing_email")));
        assert_eq!(
            stats,
            RowTransformationStats {
                rows_transformed: 3,
                fields_modified: 4,
            }
        );
    }

    #[test]
    fn leaves_rows_unchanged_when_no_fields_match() {
        let rows = vec![as_object(json!({"present": "value"}))];
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "missing".to_string(),
                operations: vec![TransformOperation::Trim],
            }],
        };

        let (transformed, stats) = transform_object_rows(&rows, &config).unwrap();

        assert_eq!(transformed, rows);
        assert_eq!(stats, RowTransformationStats::default());
    }

    #[test]
    fn collects_field_level_modification_summaries() {
        let rows = vec![
            as_object(json!({"email": "  TEST@EXAMPLE.COM  ", "status": "ACTIVE"})),
            as_object(json!({"email": "SECOND@EXAMPLE.COM  ", "status": "ACTIVE"})),
        ];
        let config = FieldTransformerConfig {
            transformations: vec![
                FieldTransformation {
                    field: "email".to_string(),
                    operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                },
                FieldTransformation {
                    field: "status".to_string(),
                    operations: vec![TransformOperation::Lower],
                },
            ],
        };

        let (_, stats, modifications) =
            transform_object_rows_with_metadata(&rows, &config).unwrap();

        assert_eq!(stats.rows_transformed, 2);
        assert_eq!(stats.fields_modified, 4);
        assert_eq!(modifications.len(), 2);
        assert_eq!(modifications[0].field_name, "email");
        assert_eq!(modifications[0].rows_modified, 2);
        assert_eq!(modifications[0].operation_count, 2);
        assert!(!modifications[0].is_reversible);
        assert_eq!(modifications[1].field_name, "status");
        assert_eq!(modifications[1].rows_modified, 2);
    }

    fn as_object(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("object row")
    }
}
