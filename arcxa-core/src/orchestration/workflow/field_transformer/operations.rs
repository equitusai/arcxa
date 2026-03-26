use anyhow::{Context, Result};

use crate::orchestration::workflow::definition::TransformOperation;

pub(crate) fn apply_transform_operation(
    value: &serde_json::Value,
    operation: &TransformOperation,
) -> Result<serde_json::Value> {
    let str_value = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        _ => value.to_string(),
    };

    let result = match operation {
        TransformOperation::Trim => serde_json::Value::String(str_value.trim().to_string()),
        TransformOperation::Lower => serde_json::Value::String(str_value.to_lowercase()),
        TransformOperation::Upper => serde_json::Value::String(str_value.to_uppercase()),
        TransformOperation::Replace { from, to } => {
            serde_json::Value::String(str_value.replace(from, to))
        }
        TransformOperation::Regex {
            pattern,
            replacement,
        } => {
            let re = regex::Regex::new(pattern)
                .with_context(|| format!("Invalid regex pattern: {}", pattern))?;
            serde_json::Value::String(re.replace_all(&str_value, replacement.as_str()).to_string())
        }
        TransformOperation::Substring { start, length } => {
            let end = if let Some(len) = length {
                std::cmp::min(start + len, str_value.len())
            } else {
                str_value.len()
            };
            let substr = if *start < str_value.len() {
                &str_value[*start..end]
            } else {
                ""
            };
            serde_json::Value::String(substr.to_string())
        }
        TransformOperation::IfNull { default_value } => {
            if str_value.is_empty() || value.is_null() {
                serde_json::Value::String(default_value.clone())
            } else {
                value.clone()
            }
        }
        TransformOperation::Round { decimals } => {
            if let Some(num) = value.as_f64() {
                let factor = 10_f64.powi(*decimals as i32);
                let rounded = (num * factor).round() / factor;
                serde_json::json!(rounded)
            } else {
                value.clone()
            }
        }
        TransformOperation::FormatDate { format } => {
            serde_json::Value::String(format!("formatted({}): {}", format, str_value))
        }
        TransformOperation::Concat { .. } => value.clone(),
        TransformOperation::Split { delimiter, index } => {
            let parts: Vec<&str> = str_value.split(delimiter.as_str()).collect();
            if *index < parts.len() {
                serde_json::Value::String(parts[*index].to_string())
            } else {
                serde_json::Value::String(String::new())
            }
        }
        TransformOperation::Coalesce { .. } => value.clone(),
        TransformOperation::Custom { expression } => {
            serde_json::Value::String(format!("custom({}): {}", expression, str_value))
        }
    };

    Ok(result)
}

pub(crate) fn is_reversible(operations: &[TransformOperation]) -> bool {
    for operation in operations {
        match operation {
            TransformOperation::Trim
            | TransformOperation::Lower
            | TransformOperation::Upper
            | TransformOperation::Substring { .. }
            | TransformOperation::Round { .. }
            | TransformOperation::Split { .. } => return false,
            _ => {}
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{apply_transform_operation, is_reversible};
    use crate::orchestration::workflow::definition::TransformOperation;
    use serde_json::json;

    #[test]
    fn applies_scalar_string_operations() {
        let trimmed =
            apply_transform_operation(&json!("  TEST@EXAMPLE.COM  "), &TransformOperation::Trim)
                .unwrap();
        let lowered = apply_transform_operation(&trimmed, &TransformOperation::Lower).unwrap();

        assert_eq!(lowered, json!("test@example.com"));
    }

    #[test]
    fn preserves_current_concat_behavior() {
        let value = json!("alice");
        let result = apply_transform_operation(
            &value,
            &TransformOperation::Concat {
                separator: "-".to_string(),
                fields: vec!["first".to_string(), "last".to_string()],
            },
        )
        .unwrap();

        assert_eq!(result, value);
    }

    #[test]
    fn reports_irreversible_operations() {
        assert!(is_reversible(&[TransformOperation::Replace {
            from: "a".to_string(),
            to: "b".to_string(),
        }]));
        assert!(!is_reversible(&[
            TransformOperation::Trim,
            TransformOperation::Lower,
        ]));
    }
}
