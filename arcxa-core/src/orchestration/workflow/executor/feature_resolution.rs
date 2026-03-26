use anyhow::Result;
use std::collections::HashMap;

use super::{ExecutionContext, WorkflowExecutor};

impl WorkflowExecutor {
    /// Extract features using the new FeatureMapping structure.
    pub(super) fn extract_features_from_mappings(
        &self,
        mappings: &[crate::orchestration::workflow::definition::FeatureMapping],
        context: &ExecutionContext,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut features = HashMap::new();

        for mapping in mappings {
            let field_value = self.resolve_feature(&mapping.field_name, context)?;

            let feature_value = if let Some(transform) = &mapping.transform {
                self.apply_feature_transform(&field_value, transform)?
            } else {
                field_value
            };

            features.insert(mapping.feature_name.clone(), feature_value);
        }

        Ok(features)
    }

    /// Apply feature transformation (for feature engineering).
    pub(super) fn apply_feature_transform(
        &self,
        value: &serde_json::Value,
        transform: &str,
    ) -> Result<serde_json::Value> {
        match transform {
            "lower" => {
                if let Some(s) = value.as_str() {
                    Ok(serde_json::json!(s.to_lowercase()))
                } else {
                    Ok(value.clone())
                }
            }
            "upper" => {
                if let Some(s) = value.as_str() {
                    Ok(serde_json::json!(s.to_uppercase()))
                } else {
                    Ok(value.clone())
                }
            }
            "trim" => {
                if let Some(s) = value.as_str() {
                    Ok(serde_json::json!(s.trim()))
                } else {
                    Ok(value.clone())
                }
            }
            "normalize" => {
                // Mock normalization - in real system would call normalization service.
                Ok(value.clone())
            }
            _ => {
                tracing::warn!("Unknown feature transform: {}", transform);
                Ok(value.clone())
            }
        }
    }

    /// Extract features from execution context.
    pub(super) fn extract_features(
        &self,
        feature_names: &[String],
        context: &ExecutionContext,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut features = HashMap::new();

        if feature_names.is_empty() {
            if let serde_json::Value::Object(map) = &context.working_data {
                for (key, value) in map {
                    features.insert(key.clone(), value.clone());
                }
            }
            return Ok(features);
        }

        for feature_name in feature_names {
            let value = self.resolve_feature(feature_name, context)?;
            features.insert(feature_name.clone(), value);
        }

        Ok(features)
    }

    /// Resolve a feature from context with support for nested references.
    /// Supports: "field_name", "step_id.field", "step_id.nested.field".
    pub(super) fn resolve_feature(
        &self,
        feature_name: &str,
        context: &ExecutionContext,
    ) -> Result<serde_json::Value> {
        if feature_name.contains('.') {
            let parts: Vec<&str> = feature_name.splitn(2, '.').collect();
            if parts.len() == 2 {
                let step_id = parts[0];
                let field_path = parts[1];

                if let Some(step_output) = context.step_outputs.get(step_id) {
                    return self
                        .get_nested_value(step_output, field_path)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Field '{}' not found in step '{}' output",
                                field_path,
                                step_id
                            )
                        });
                }
            }
        }

        if let Some(value) = context.step_outputs.get(feature_name) {
            return Ok(value.clone());
        }

        if let Some(value) = context.working_data.get(feature_name) {
            return Ok(value.clone());
        }

        if let Some(value) = context.input_data.get(feature_name) {
            return Ok(value.clone());
        }

        anyhow::bail!(
            "Required feature '{}' not found. Available in working_data: {:?}, step_outputs: {:?}",
            feature_name,
            context
                .working_data
                .as_object()
                .map(|o| o.keys().collect::<Vec<_>>()),
            context.step_outputs.keys().collect::<Vec<_>>()
        )
    }

    /// Get nested value from JSON using dot notation (e.g. "confidence" or "output.score").
    pub(super) fn get_nested_value(
        &self,
        value: &serde_json::Value,
        path: &str,
    ) -> Option<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current.clone())
    }
}
