use anyhow::Result;

use crate::orchestration::workflow::definition::FieldTransformerConfig;

use super::operations::{apply_transform_operation, is_reversible};

pub(crate) fn execute_legacy_object_transform(
    config: &FieldTransformerConfig,
    working_data: &serde_json::Value,
) -> Result<(bool, serde_json::Value, f64)> {
    let mut transformed_data = if let serde_json::Value::Object(map) = working_data {
        map.clone()
    } else {
        return Ok((
            false,
            serde_json::json!({"error": "working_data is not an object"}),
            0.0,
        ));
    };

    let mut modifications = Vec::new();
    let mut field_count = 0;

    for transformation in &config.transformations {
        let field_name = &transformation.field;

        let Some(old_value) = transformed_data.get(field_name).cloned() else {
            continue;
        };

        let mut current_value = old_value.clone();
        for operation in &transformation.operations {
            current_value = apply_transform_operation(&current_value, operation)?;
        }

        if old_value != current_value {
            modifications.push(serde_json::json!({
                "field_name": field_name,
                "old_value": old_value,
                "new_value": current_value.clone(),
                "operations": transformation.operations.len(),
                "is_reversible": is_reversible(&transformation.operations),
            }));
            field_count += 1;
        }

        transformed_data.insert(field_name.clone(), current_value);
    }

    let mut output = serde_json::Map::new();
    for (key, value) in transformed_data {
        output.insert(key, value);
    }
    output.insert(
        "_modifications".to_string(),
        serde_json::json!(modifications),
    );
    output.insert(
        "_fields_modified".to_string(),
        serde_json::json!(field_count),
    );

    Ok((true, serde_json::Value::Object(output), 1.0))
}

#[cfg(test)]
mod tests {
    use super::execute_legacy_object_transform;
    use crate::orchestration::workflow::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };
    use serde_json::json;

    #[test]
    fn transforms_object_fields_and_tracks_modifications() {
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "email".to_string(),
                operations: vec![TransformOperation::Trim, TransformOperation::Lower],
            }],
        };

        let (success, output, confidence) =
            execute_legacy_object_transform(&config, &json!({"email": "  TEST@EXAMPLE.COM  "}))
                .unwrap();

        assert!(success);
        assert_eq!(confidence, 1.0);
        assert_eq!(output["email"], json!("test@example.com"));
        assert_eq!(output["_fields_modified"], json!(1));
        assert_eq!(output["_modifications"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn skips_missing_fields_without_error() {
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "missing".to_string(),
                operations: vec![TransformOperation::Trim],
            }],
        };

        let (success, output, _) =
            execute_legacy_object_transform(&config, &json!({"present": "value"})).unwrap();

        assert!(success);
        assert_eq!(output["present"], json!("value"));
        assert_eq!(output["_fields_modified"], json!(0));
        assert_eq!(output["_modifications"], json!([]));
    }

    #[test]
    fn rejects_non_object_working_data() {
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "email".to_string(),
                operations: vec![TransformOperation::Trim],
            }],
        };

        let (success, output, confidence) =
            execute_legacy_object_transform(&config, &json!(["not", "an", "object"])).unwrap();

        assert!(!success);
        assert_eq!(confidence, 0.0);
        assert_eq!(output["error"], json!("working_data is not an object"));
    }
}
