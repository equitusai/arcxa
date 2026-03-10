//! Condition Evaluator - Evaluate boolean expressions on input data
//!
//! High-performance condition evaluation with support for:
//! - Comparison operators (==, !=, >, <, >=, <=)
//! - String operations (contains, regex matching)
//! - Logical operators (AND, OR, NOT)
//! - Nested field access (dot notation)

use crate::workflows::domain::Condition;
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;

/// Evaluates conditions on JSON input data
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    /// Evaluate a condition against input data
    ///
    /// ## Performance
    /// - Simple comparisons: O(1), ~100ns
    /// - String contains: O(n), ~1-10μs
    /// - Regex matching: O(n), ~5-20μs
    /// - Nested conditions: O(k) where k = number of conditions
    ///
    /// ## Errors
    /// - Field not found in data
    /// - Type mismatch (comparing string to number)
    /// - Invalid regex pattern
    pub fn evaluate(condition: &Condition, data: &JsonValue) -> Result<bool> {
        match condition {
            Condition::Equals { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Ok(field_value == value)
            }

            Condition::NotEquals { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Ok(field_value != value)
            }

            Condition::GreaterThan { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Self::compare_numeric(field_value, value, |a, b| a > b)
            }

            Condition::LessThan { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Self::compare_numeric(field_value, value, |a, b| a < b)
            }

            Condition::GreaterThanOrEqual { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Self::compare_numeric(field_value, value, |a, b| a >= b)
            }

            Condition::LessThanOrEqual { field, value } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Self::compare_numeric(field_value, value, |a, b| a <= b)
            }

            Condition::Contains { field, substring } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;

                if let Some(s) = field_value.as_str() {
                    Ok(s.contains(substring))
                } else {
                    anyhow::bail!("Field '{}' is not a string", field)
                }
            }

            Condition::Matches { field, pattern } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;

                if let Some(s) = field_value.as_str() {
                    let regex = regex::Regex::new(pattern)
                        .with_context(|| format!("Invalid regex pattern: {}", pattern))?;
                    Ok(regex.is_match(s))
                } else {
                    anyhow::bail!("Field '{}' is not a string", field)
                }
            }

            Condition::Exists { field } => Ok(Condition::extract_field(data, field).is_some()),

            Condition::IsNull { field } => match Condition::extract_field(data, field) {
                None => Ok(true),
                Some(value) => Ok(value.is_null()),
            },

            Condition::In { field, values } => {
                let field_value = Condition::extract_field(data, field)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", field))?;
                Ok(values.contains(field_value))
            }

            Condition::And(conditions) => {
                for cond in conditions.iter() {
                    if !Self::evaluate(cond, data)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Condition::Or(conditions) => {
                for cond in conditions.iter() {
                    if Self::evaluate(cond, data)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Condition::Not(condition) => Ok(!Self::evaluate(condition, data)?),

            Condition::Always => Ok(true),

            Condition::Never => Ok(false),
        }
    }

    /// Compare numeric values using a comparison function
    fn compare_numeric<F>(a: &JsonValue, b: &JsonValue, cmp: F) -> Result<bool>
    where
        F: Fn(f64, f64) -> bool,
    {
        match (a.as_f64(), b.as_f64()) {
            (Some(a_num), Some(b_num)) => Ok(cmp(a_num, b_num)),
            (Some(a_num), None) => {
                // Try to convert b to number if it's a string
                if let Some(b_str) = b.as_str() {
                    if let Ok(b_num) = b_str.parse::<f64>() {
                        return Ok(cmp(a_num, b_num));
                    }
                }
                anyhow::bail!("Cannot compare non-numeric values")
            }
            (None, Some(b_num)) => {
                // Try to convert a to number if it's a string
                if let Some(a_str) = a.as_str() {
                    if let Ok(a_num) = a_str.parse::<f64>() {
                        return Ok(cmp(a_num, b_num));
                    }
                }
                anyhow::bail!("Cannot compare non-numeric values")
            }
            (None, None) => anyhow::bail!("Cannot compare non-numeric values"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_equals_true() {
        let data = json!({"status": "active", "count": 10});

        assert!(
            ConditionEvaluator::evaluate(&Condition::equals("status", "active"), &data).unwrap()
        );

        assert!(ConditionEvaluator::evaluate(&Condition::equals("count", 10), &data).unwrap());
    }

    #[test]
    fn test_equals_false() {
        let data = json!({"status": "active"});

        assert!(
            !ConditionEvaluator::evaluate(&Condition::equals("status", "inactive"), &data).unwrap()
        );
    }

    #[test]
    fn test_not_equals() {
        let data = json!({"status": "active"});

        assert!(
            ConditionEvaluator::evaluate(&Condition::not_equals("status", "inactive"), &data)
                .unwrap()
        );

        assert!(
            !ConditionEvaluator::evaluate(&Condition::not_equals("status", "active"), &data)
                .unwrap()
        );
    }

    #[test]
    fn test_greater_than() {
        let data = json!({"age": 30, "score": 85.5});

        assert!(ConditionEvaluator::evaluate(&Condition::greater_than("age", 25), &data).unwrap());

        assert!(!ConditionEvaluator::evaluate(&Condition::greater_than("age", 30), &data).unwrap());

        assert!(
            ConditionEvaluator::evaluate(&Condition::greater_than("score", 80.0), &data).unwrap()
        );
    }

    #[test]
    fn test_less_than() {
        let data = json!({"age": 25});

        assert!(ConditionEvaluator::evaluate(&Condition::less_than("age", 30), &data).unwrap());

        assert!(!ConditionEvaluator::evaluate(&Condition::less_than("age", 20), &data).unwrap());
    }

    #[test]
    fn test_contains() {
        let data = json!({"name": "Alice Smith", "email": "alice@example.com"});

        assert!(
            ConditionEvaluator::evaluate(&Condition::contains("name", "Alice"), &data).unwrap()
        );

        assert!(
            ConditionEvaluator::evaluate(&Condition::contains("email", "@example.com"), &data)
                .unwrap()
        );

        assert!(!ConditionEvaluator::evaluate(&Condition::contains("name", "Bob"), &data).unwrap());
    }

    #[test]
    fn test_matches_regex() {
        let data = json!({"email": "alice@example.com", "phone": "555-1234"});

        assert!(ConditionEvaluator::evaluate(
            &Condition::Matches {
                field: "email".to_string(),
                pattern: r"^\w+@\w+\.\w+$".to_string(),
            },
            &data
        )
        .unwrap());

        assert!(ConditionEvaluator::evaluate(
            &Condition::Matches {
                field: "phone".to_string(),
                pattern: r"^\d{3}-\d{4}$".to_string(),
            },
            &data
        )
        .unwrap());
    }

    #[test]
    fn test_exists() {
        let data = json!({"name": "Alice", "age": null});

        assert!(ConditionEvaluator::evaluate(&Condition::exists("name"), &data).unwrap());

        assert!(ConditionEvaluator::evaluate(&Condition::exists("age"), &data).unwrap());

        assert!(!ConditionEvaluator::evaluate(&Condition::exists("missing"), &data).unwrap());
    }

    #[test]
    fn test_is_null() {
        let data = json!({"name": "Alice", "age": null});

        assert!(!ConditionEvaluator::evaluate(
            &Condition::IsNull {
                field: "name".to_string()
            },
            &data
        )
        .unwrap());

        assert!(ConditionEvaluator::evaluate(
            &Condition::IsNull {
                field: "age".to_string()
            },
            &data
        )
        .unwrap());

        assert!(ConditionEvaluator::evaluate(
            &Condition::IsNull {
                field: "missing".to_string()
            },
            &data
        )
        .unwrap());
    }

    #[test]
    fn test_in_values() {
        let data = json!({"status": "active", "priority": 2});

        assert!(ConditionEvaluator::evaluate(
            &Condition::In {
                field: "status".to_string(),
                values: vec![json!("active"), json!("pending")],
            },
            &data
        )
        .unwrap());

        assert!(!ConditionEvaluator::evaluate(
            &Condition::In {
                field: "status".to_string(),
                values: vec![json!("inactive"), json!("deleted")],
            },
            &data
        )
        .unwrap());

        assert!(ConditionEvaluator::evaluate(
            &Condition::In {
                field: "priority".to_string(),
                values: vec![json!(1), json!(2), json!(3)],
            },
            &data
        )
        .unwrap());
    }

    #[test]
    fn test_and_all_true() {
        let data = json!({"status": "active", "age": 30});

        let condition = Condition::and(vec![
            Condition::equals("status", "active"),
            Condition::greater_than("age", 25),
        ]);

        assert!(ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_and_one_false() {
        let data = json!({"status": "active", "age": 20});

        let condition = Condition::and(vec![
            Condition::equals("status", "active"),
            Condition::greater_than("age", 25),
        ]);

        assert!(!ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_or_one_true() {
        let data = json!({"status": "inactive", "age": 30});

        let condition = Condition::or(vec![
            Condition::equals("status", "active"),
            Condition::greater_than("age", 25),
        ]);

        assert!(ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_or_all_false() {
        let data = json!({"status": "inactive", "age": 20});

        let condition = Condition::or(vec![
            Condition::equals("status", "active"),
            Condition::greater_than("age", 25),
        ]);

        assert!(!ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_not() {
        let data = json!({"status": "active"});

        let condition = Condition::not(Condition::equals("status", "inactive"));

        assert!(ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_nested_conditions() {
        let data = json!({"type": "user", "status": "active", "age": 30});

        // (type == "user") AND ((status == "active") OR (age > 25))
        let condition = Condition::and(vec![
            Condition::equals("type", "user"),
            Condition::or(vec![
                Condition::equals("status", "active"),
                Condition::greater_than("age", 25),
            ]),
        ]);

        assert!(ConditionEvaluator::evaluate(&condition, &data).unwrap());
    }

    #[test]
    fn test_nested_field_access() {
        let data = json!({
            "user": {
                "profile": {
                    "age": 30
                }
            }
        });

        assert!(ConditionEvaluator::evaluate(
            &Condition::greater_than("user.profile.age", 25),
            &data
        )
        .unwrap());
    }

    #[test]
    fn test_field_not_found() {
        let data = json!({"status": "active"});

        let result =
            ConditionEvaluator::evaluate(&Condition::equals("missing_field", "value"), &data);

        assert!(result.is_err());
    }

    #[test]
    fn test_type_mismatch() {
        let data = json!({"name": "Alice"});

        // Trying to compare string with number
        let result = ConditionEvaluator::evaluate(&Condition::greater_than("name", 10), &data);

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_regex() {
        let data = json!({"text": "hello"});

        let result = ConditionEvaluator::evaluate(
            &Condition::Matches {
                field: "text".to_string(),
                pattern: "[invalid(".to_string(),
            },
            &data,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_always_true() {
        let data = json!({});
        assert!(ConditionEvaluator::evaluate(&Condition::Always, &data).unwrap());
    }

    #[test]
    fn test_never_false() {
        let data = json!({});
        assert!(!ConditionEvaluator::evaluate(&Condition::Never, &data).unwrap());
    }

    #[test]
    fn test_string_to_number_conversion() {
        let data = json!({"age": "30", "limit": 25});

        // Should handle string-to-number conversion
        assert!(ConditionEvaluator::evaluate(&Condition::greater_than("age", 25), &data).unwrap());
    }
}
