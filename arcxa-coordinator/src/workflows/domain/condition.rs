//! Workflow Conditions - Boolean expressions evaluated on input data
//!
//! Conditions are the core of the routing logic, determining which routes
//! should be activated for a given input.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A boolean expression evaluated on input data
///
/// Conditions can be simple comparisons or complex nested expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Field equals value
    ///
    /// Example: `{"type": "Equals", "field": "status", "value": "active"}`
    Equals { field: String, value: JsonValue },

    /// Field not equals value
    NotEquals { field: String, value: JsonValue },

    /// Field greater than value (numeric comparison)
    GreaterThan { field: String, value: JsonValue },

    /// Field less than value (numeric comparison)
    LessThan { field: String, value: JsonValue },

    /// Field greater than or equal to value
    GreaterThanOrEqual { field: String, value: JsonValue },

    /// Field less than or equal to value
    LessThanOrEqual { field: String, value: JsonValue },

    /// String field contains substring
    Contains { field: String, substring: String },

    /// String field matches regex pattern
    Matches { field: String, pattern: String },

    /// Field exists in input
    Exists { field: String },

    /// Field is null or missing
    IsNull { field: String },

    /// Value is in a set
    In {
        field: String,
        values: Vec<JsonValue>,
    },

    /// All conditions must be true
    And(Box<Vec<Condition>>),

    /// At least one condition must be true
    Or(Box<Vec<Condition>>),

    /// Negates the condition
    Not(Box<Condition>),

    /// Always evaluates to true
    Always,

    /// Always evaluates to false
    Never,
}

impl Condition {
    /// Create an Equals condition
    pub fn equals(field: impl Into<String>, value: impl Into<JsonValue>) -> Self {
        Self::Equals {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a NotEquals condition
    pub fn not_equals(field: impl Into<String>, value: impl Into<JsonValue>) -> Self {
        Self::NotEquals {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a GreaterThan condition
    pub fn greater_than(field: impl Into<String>, value: impl Into<JsonValue>) -> Self {
        Self::GreaterThan {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a LessThan condition
    pub fn less_than(field: impl Into<String>, value: impl Into<JsonValue>) -> Self {
        Self::LessThan {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a Contains condition
    pub fn contains(field: impl Into<String>, substring: impl Into<String>) -> Self {
        Self::Contains {
            field: field.into(),
            substring: substring.into(),
        }
    }

    /// Create an Exists condition
    pub fn exists(field: impl Into<String>) -> Self {
        Self::Exists {
            field: field.into(),
        }
    }

    /// Create an And condition
    pub fn and(conditions: Vec<Condition>) -> Self {
        Self::And(Box::new(conditions))
    }

    /// Create an Or condition
    pub fn or(conditions: Vec<Condition>) -> Self {
        Self::Or(Box::new(conditions))
    }

    /// Create a Not condition
    pub fn not(condition: Condition) -> Self {
        Self::Not(Box::new(condition))
    }

    /// Extract field value from JSON data using dot notation
    ///
    /// Supports nested field access: "user.address.city"
    pub fn extract_field<'a>(data: &'a JsonValue, field: &str) -> Option<&'a JsonValue> {
        let parts: Vec<&str> = field.split('.').collect();
        let mut current = data;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current)
    }

    /// Validate condition structure (recursive)
    ///
    /// Ensures condition is well-formed before execution
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Equals { field, .. }
            | Self::NotEquals { field, .. }
            | Self::GreaterThan { field, .. }
            | Self::LessThan { field, .. }
            | Self::GreaterThanOrEqual { field, .. }
            | Self::LessThanOrEqual { field, .. }
            | Self::Contains { field, .. }
            | Self::Exists { field, .. }
            | Self::IsNull { field, .. }
            | Self::In { field, .. } => {
                if field.is_empty() {
                    anyhow::bail!("Field name cannot be empty");
                }
                Ok(())
            }

            Self::Matches { field, pattern } => {
                if field.is_empty() {
                    anyhow::bail!("Field name cannot be empty");
                }
                // Validate regex pattern
                regex::Regex::new(pattern)
                    .with_context(|| format!("Invalid regex pattern: {}", pattern))?;
                Ok(())
            }

            Self::And(conditions) | Self::Or(conditions) => {
                if conditions.is_empty() {
                    anyhow::bail!("AND/OR must have at least one condition");
                }
                for cond in conditions.iter() {
                    cond.validate()?;
                }
                Ok(())
            }

            Self::Not(condition) => condition.validate(),

            Self::Always | Self::Never => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_condition_builders() {
        let cond = Condition::equals("status", "active");
        match cond {
            Condition::Equals { field, value } => {
                assert_eq!(field, "status");
                assert_eq!(value, json!("active"));
            }
            _ => panic!("Expected Equals condition"),
        }

        let cond = Condition::greater_than("age", 18);
        match cond {
            Condition::GreaterThan { field, value } => {
                assert_eq!(field, "age");
                assert_eq!(value, json!(18));
            }
            _ => panic!("Expected GreaterThan condition"),
        }
    }

    #[test]
    fn test_extract_field_simple() {
        let data = json!({"name": "Alice", "age": 30});

        let value = Condition::extract_field(&data, "name").unwrap();
        assert_eq!(value, &json!("Alice"));

        let value = Condition::extract_field(&data, "age").unwrap();
        assert_eq!(value, &json!(30));
    }

    #[test]
    fn test_extract_field_nested() {
        let data = json!({
            "user": {
                "address": {
                    "city": "New York",
                    "zip": "10001"
                }
            }
        });

        let value = Condition::extract_field(&data, "user.address.city").unwrap();
        assert_eq!(value, &json!("New York"));

        let value = Condition::extract_field(&data, "user.address.zip").unwrap();
        assert_eq!(value, &json!("10001"));
    }

    #[test]
    fn test_extract_field_missing() {
        let data = json!({"name": "Alice"});

        let value = Condition::extract_field(&data, "age");
        assert!(value.is_none());

        let value = Condition::extract_field(&data, "user.address.city");
        assert!(value.is_none());
    }

    #[test]
    fn test_validate_valid_conditions() {
        assert!(Condition::equals("field", "value").validate().is_ok());
        assert!(Condition::greater_than("age", 18).validate().is_ok());
        assert!(Condition::contains("name", "Alice").validate().is_ok());
        assert!(Condition::exists("field").validate().is_ok());
        assert!(Condition::Always.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_field() {
        let cond = Condition::Equals {
            field: String::new(),
            value: json!("value"),
        };
        assert!(cond.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_regex() {
        let cond = Condition::Matches {
            field: "text".to_string(),
            pattern: "[invalid regex(".to_string(),
        };
        assert!(cond.validate().is_err());
    }

    #[test]
    fn test_validate_empty_and() {
        let cond = Condition::And(Box::new(vec![]));
        assert!(cond.validate().is_err());
    }

    #[test]
    fn test_validate_nested() {
        let cond = Condition::and(vec![
            Condition::equals("status", "active"),
            Condition::or(vec![
                Condition::greater_than("age", 18),
                Condition::contains("name", "Admin"),
            ]),
        ]);
        assert!(cond.validate().is_ok());
    }

    #[test]
    fn test_serde_equals() {
        let cond = Condition::equals("status", "active");
        let json = serde_json::to_string(&cond).unwrap();
        let deserialized: Condition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, deserialized);
    }

    #[test]
    fn test_serde_nested() {
        let cond = Condition::and(vec![
            Condition::equals("type", "user"),
            Condition::greater_than("age", 21),
        ]);
        let json = serde_json::to_string(&cond).unwrap();
        let deserialized: Condition = serde_json::from_str(&json).unwrap();
        assert_eq!(cond, deserialized);
    }
}
