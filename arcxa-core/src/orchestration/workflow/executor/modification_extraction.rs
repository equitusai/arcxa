use super::{FieldModificationRecord, WorkflowExecutor};

impl WorkflowExecutor {
    /// Extract field modifications from step output for lineage tracking.
    pub(super) fn extract_modifications(
        &self,
        output: &serde_json::Value,
    ) -> Vec<FieldModificationRecord> {
        let mut modifications = Vec::new();

        if let Some(modification_items) = output
            .get("_modifications")
            .and_then(|value| value.as_array())
        {
            for modification in modification_items {
                if let Some(field_name) = modification
                    .get("field_name")
                    .and_then(|value| value.as_str())
                {
                    modifications.push(FieldModificationRecord {
                        field_name: field_name.to_string(),
                        old_value: modification
                            .get("old_value")
                            .cloned()
                            .unwrap_or(serde_json::json!(null)),
                        new_value: modification
                            .get("new_value")
                            .cloned()
                            .unwrap_or(serde_json::json!(null)),
                        is_reversible: modification
                            .get("is_reversible")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false),
                        operation_count: modification
                            .get("operations")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(1) as usize,
                    });
                }
            }
        }

        modifications
    }
}
