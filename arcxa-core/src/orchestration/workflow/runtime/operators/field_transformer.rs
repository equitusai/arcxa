use crate::orchestration::workflow::definition::FieldTransformerConfig;
use crate::orchestration::workflow::error::Result;
use crate::orchestration::workflow::field_transformer::FieldModificationSummary;
use crate::orchestration::workflow::runtime::frame::BatchFrame;
use crate::orchestration::workflow::RowTransformationStats;

use super::RuntimeOperator;

#[derive(Debug)]
pub struct FieldTransformerBatchResult {
    pub frame: BatchFrame,
    pub(crate) stats: RowTransformationStats,
    pub(crate) modifications: Vec<FieldModificationSummary>,
}

/// Batch-surface field transformer for the optimized small-dataset row path.
///
/// This intentionally preserves the existing scalar field-transform semantics by
/// reusing the extracted row helper. The legacy object-style field transformer
/// remains the primary path for classic workflows.
#[derive(Debug, Default)]
pub struct FieldTransformerBatchOperator;

impl RuntimeOperator for FieldTransformerBatchOperator {
    fn name(&self) -> &'static str {
        "field_transformer"
    }
}

impl FieldTransformerBatchOperator {
    pub fn execute(
        &self,
        frame: BatchFrame,
        config: &FieldTransformerConfig,
    ) -> Result<FieldTransformerBatchResult> {
        let metadata = frame.metadata().clone();
        let rows = frame.to_json_values()?;
        let object_rows = rows
            .into_iter()
            .map(|row| {
                row.as_object().cloned().ok_or_else(|| {
                    crate::orchestration::workflow::error::WorkflowError::InvalidData(
                        "Batch field transformer requires object rows".into(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (transformed_rows, stats, modifications) =
            crate::orchestration::workflow::field_transformer::transform_object_rows_with_metadata(
                &object_rows,
                config,
            )?;

        Ok(FieldTransformerBatchResult {
            frame: BatchFrame::from_object_rows(&transformed_rows)?.with_metadata(metadata),
            stats,
            modifications,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FieldTransformerBatchOperator;
    use crate::orchestration::workflow::definition::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};
    use serde_json::json;

    #[test]
    fn transforms_rows_and_preserves_metadata() {
        let frame = BatchFrame::from_json_values(&[
            json!({"email": "  TEST@EXAMPLE.COM  ", "status": "ACTIVE"}),
            json!({"email": "second@example.com", "status": "PENDING"}),
        ])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("extract_transform".to_string()),
            source_kind: Some("db_extract".to_string()),
            source_id: None,
        });

        let operator = FieldTransformerBatchOperator;
        let result = operator
            .execute(
                frame,
                &FieldTransformerConfig {
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
                },
            )
            .unwrap();

        let transformed_rows = result.frame.to_json_values().unwrap();
        assert_eq!(transformed_rows[0]["email"], json!("test@example.com"));
        assert_eq!(transformed_rows[0]["status"], json!("active"));
        assert_eq!(transformed_rows[1]["status"], json!("pending"));
        assert_eq!(result.stats.rows_transformed, 2);
        assert_eq!(result.stats.fields_modified, 3);
        assert_eq!(result.modifications.len(), 2);
        assert_eq!(
            result.frame.metadata().source_step_id.as_deref(),
            Some("extract_transform")
        );
    }
}
